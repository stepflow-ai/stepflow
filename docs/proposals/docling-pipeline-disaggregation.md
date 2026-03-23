# Proposal: Docling Pipeline Disaggregation — Per-Page Fan-Out with Dual-Pipeline Support

**Status:** Draft
**Authors:** Nate McCall
**Created:** March 2026
**Supersedes:** `docling-step-worker.md` (DEPRECATED — HTTP push architecture), `stepflow-docling-namespace.md` (DEPRECATED — Pingora LB topology)
**Builds on:** `integrations/docling-proto-step-worker/` (gRPC pull worker), `examples/production/k8s/stepflow-docling-grpc/` (deployed K8s namespace)
**Related:** `rust-native-docling.md` (longer-term Rust migration), #757 (observability), #741 (queue metrics)

## Summary

Decompose docling's monolithic document conversion pipeline into independently scalable Stepflow stages with per-page fan-out across distributed workers. Support two pipeline variants behind the same fan-out skeleton: the **standard pipeline** (OCR + layout + table structure — exact parity with docling-serve) and the **VLM pipeline** (GraniteDocling single-pass inference — naturally page-parallel, API-extensible). Both paths produce a `DoclingDocument` that flows through the existing chunking component unchanged.

**Current state:** Single `convert` component runs the entire `DocumentConverter.convert()` monolithically on one worker. A 50-page document occupies one worker for ~31 seconds.

**Target state:** `preprocess_pages` extracts per-page data, `MapComponent` fans out N page-level tasks to the worker pool, an assembly component merges results with full cross-page awareness, `chunk` runs on the assembled document. A 50-page document's inference work is distributed across all available workers, with the serial preprocess and assembly steps as bounded overhead.

## What Exists Today

### Deployed and Running (from #766)

The `stepflow-docling-grpc` namespace is live on the local kind cluster:

- **Orchestrator pod** (`docling-orchestrator`): facade (`:5001`, NodePort 30080) + stepflow-server (`:7840` HTTP, `:7837` gRPC). Image: `localhost/stepflow-server:alpine-0.10.0`.
- **3× worker pods** (`docling-worker`): gRPC pull transport, `STEPFLOW_QUEUE_NAME=docling`, `STEPFLOW_MAX_CONCURRENT=1`. Image: `localhost/stepflow-docling-proto-worker:latest`.
- **Shared observability** (`stepflow-o11y`): OTel Collector, Grafana (`:3000`), Jaeger (`:16686`), Prometheus (`:9090`), Loki.

### Current Flow

```yaml
# docling-process-document.yaml
classify → convert → chunk
```

Three components registered by the worker: `/docling/classify`, `/docling/convert`, `/docling/chunk`. The `convert` component calls `DocumentConverter.convert()` which internally runs docling's full `StandardPdfPipeline` (5 threaded stages) within a single worker process.

### Integration Codebase

- `integrations/docling-proto-step-worker/` — Python package with gRPC pull worker
  - `server.py` — Entry point, component registration, model warm-up, readiness sentinel
  - `convert.py` — Wraps `DocumentConverter.convert()`, handles blob I/O, page_range support
  - `classify.py`, `chunk.py` — Classification and chunking components
  - `converter_cache.py` — Caches `DocumentConverter` instances keyed by pipeline options
  - `facade/app.py` — FastAPI facade translating docling-serve API → flow submissions
  - `facade/translate.py` — Pure functions: request→flow_input, flow_output→response
- `examples/production/k8s/stepflow-docling-grpc/` — K8s manifests, apply/teardown scripts, parity test

> **Note on docling versioning:** All API references in this proposal (class names, import paths, option flags) have been verified against both the research base (commit 684f59f2, Feb 2026) and current docling v2.80.0 (March 2026). Key current-version facts: `StandardPdfPipeline` is the multi-threaded production pipeline; the sequential variant is `LegacyStandardPdfPipeline`; remote services require `enable_remote_services=True` on `PdfPipelineOptions`.

## Motivation: Why Disaggregate?

### Performance

The monolithic `convert` component runs all five pipeline stages sequentially on one worker. Layout analysis takes ~633ms/page, table structure ~1.74s/table. A 50-page document with 10 tables: ~31s wall time, all on one pod. With per-page fan-out across 3 workers, the inference time divides by the worker count. The serial bottlenecks (preprocess + assembly) are I/O-bound and measured in seconds, not tens of seconds.

### Scalability

Today, adding workers only helps with concurrent documents — each document still occupies one worker. With fan-out, a single large document benefits from the entire worker pool. This matters for the SaaS context: unknown document mixes, large documents arriving unpredictably.

### Architectural Flexibility

The dual-pipeline approach lets callers choose between the standard pipeline (highest quality, exact parity) and the VLM pipeline (single-pass, API-extensible, different quality profile). Both share the fan-out skeleton, assembly contract, and chunking step. Adding future pipeline variants (hybrid standard+VLM, Rust-native stages) means adding a new per-page component and assembly variant, not rearchitecting the flow.

## Docling Pipeline Internals — Why Naive Fan-Out Breaks

> This section summarizes research into docling's `StandardPdfPipeline` source code (commit 684f59f2, Feb 2026) via DeepWiki analysis. All claims have been verified against current docling source (v2.80.0 as of March 2026).
>
> **Pipeline class naming:** In current docling, `StandardPdfPipeline` (in `docling/pipeline/standard_pdf_pipeline.py`) IS the multi-threaded production pipeline. The old sequential single-threaded variant has been moved to `LegacyStandardPdfPipeline`. Our worker correctly imports and uses `StandardPdfPipeline`.

### Phase 1: Per-Page Model Inference (parallelizable)

Five stages, each in its own thread with bounded queues (`ThreadedQueue`, max 128 items):

1. **Preprocess** (`PreprocessThreadedStage` / `PagePreprocessingModel`) — Lazy-loads page backend via `backend.load_page(page_no - 1)` (0-indexed — docling page numbers are 1-indexed), extracts page images at multiple scales, populates programmatic text cells (character/word/line level with bounding boxes from configured PDF backend, default `dlparse_v2`).
2. **OCR** (`BaseOcrModel`) — Tesseract/EasyOCR/RapidOCR on page images. Batch size 64. Only runs where programmatic text is absent unless `force_full_page_ocr=True`.
3. **Layout** (`LayoutModel` — HERON/EGRET, RT-DETR architecture, ONNX Runtime) — Object detection for bounding boxes (text blocks, figures, tables, headers). Batch size 64. Always runs.
4. **Table Structure** (`TableStructureModel` — TableFormer FAST/ACCURATE, ONNX Runtime) — Row/column structure within table bounding boxes. Batch size 4 (expensive: 2-6s per table on CPU). Gated by `do_table_structure=True` (default True).
5. **Page Assembly** (`PageAssembleModel`) — Combines OCR + layout + table results into `page.assembled`. Batch size 1. Always runs.

All five stages are per-page. Docling already exploits page-level parallelism within a single process via its threading model.

### Phase 2: Document-Level Assembly (needs ALL pages)

After all pages complete Phase 1:

1. **Layout Postprocessing** (`LayoutPostprocessor` at `docling/utils/layout_postprocessor.py`) — Resolves overlapping bounding box clusters (R-tree + interval tree spatial indexing, UnionFind grouping). Maps text cells to clusters. Merges hierarchical elements. Type-specific overlap thresholds.
2. **Reading Order** (`ReadingOrderModel` at `docling/models/readingorder_model.py`) — **CROSS-PAGE AWARE.** Sorts elements across all pages. Associates captions with figures/tables across page boundaries. Merges hyphenated text at page breaks. Groups consecutive list items. Places headers/footers in furniture layer.
3. **Enrichment** (optional) — Picture classification, description, chart extraction, code/formula detection. All default to `False`. See "Enrichment Models in the Disaggregated Context" section.

### The Cross-Page Problem

Running `DocumentConverter.convert(source, page_range="N-N")` independently per page produces INCORRECT results:
- Reading order is per-page only — cross-page element ordering is lost
- Hyphenation merging at page breaks doesn't happen
- Caption-to-figure association across pages fails
- Tables spanning pages get disconnected structures
- Each call re-parses the full PDF to extract one page

**Conclusion:** The fan-out boundary must be BETWEEN Phase 1 (per-page inference) and Phase 2 (assembly). Per-page inference tasks produce intermediate results; assembly runs once on the complete set.

## Architecture

### Dual-Pipeline Skeleton

```
                     ┌─ standard path ─────────────────────────────────────┐
                     │ classify → preprocess_pages → map(per_page_inference)│
facade ──┤           │            → assemble_standard → chunk              │
         │           └──────────────────────────────────────────────────────┘
         │           ┌─ vlm path ──────────────────────────────────────────┐
         └───────────│ preprocess_pages → map(vlm_infer_page)              │
                     │            → assemble_doctags → chunk               │
                     └──────────────────────────────────────────────────────┘
```

Both paths share: `preprocess_pages`, `chunk`, the MapComponent fan-out/fan-in skeleton, and the `DoclingDocument` output contract.

### Facade Routing

The facade gains path-based routing — our first intentional API extension beyond docling-serve:

- `/v1/convert/source` — Standard pipeline (default, docling-serve parity)
- `/v1/vlm/convert/source` — VLM pipeline

The change in `facade/app.py` is minimal: `_submit_and_respond()` already accepts a flow ID. The route handler selects which registered flow to submit to. `translate.py` is unchanged.

### Flow Definitions

**Standard pipeline with fan-out** (`docling-process-document-fanout.yaml`):
```yaml
steps:
  - id: classify
    component: /docling/classify
    input:
      source: { $input: "source" }
      source_kind: { $input: "source_kind" }

  - id: preprocess
    component: /docling/preprocess_pages
    input:
      source: { $input: "source" }
      source_kind: { $input: "source_kind" }
      pipeline_config: { $step: "classify", path: "recommended_config" }

  - id: inference
    component: /builtin/map
    input:
      workflow: { ... }  # per-page inference sub-flow
      items: { $step: "preprocess", path: "pages" }
      max_concurrency: 10  # bounded fan-out

  - id: assemble
    component: /docling/assemble_standard
    input:
      page_results: { $step: "inference", path: "results" }
      pipeline_config: { $step: "classify", path: "recommended_config" }
      to_formats: { $input: "to_formats" }
      image_export_mode: { $input: "image_export_mode" }

  - id: chunk
    component: /docling/chunk
    input:
      document: { $step: "assemble", path: "document_dict" }
      chunk_options: { $input: "chunk_options" }
```

**VLM pipeline** (`docling-process-document-vlm.yaml`):
```yaml
steps:
  - id: preprocess
    component: /docling/preprocess_pages
    input:
      source: { $input: "source" }
      source_kind: { $input: "source_kind" }

  - id: inference
    component: /builtin/map
    input:
      workflow: { ... }  # per-page VLM sub-flow
      items: { $step: "preprocess", path: "pages" }
      max_concurrency: 20  # VLM tasks are lighter (HTTP calls)

  - id: assemble
    component: /docling/assemble_doctags
    input:
      page_results: { $step: "inference", path: "results" }
      to_formats: { $input: "to_formats" }
      image_export_mode: { $input: "image_export_mode" }

  - id: chunk
    component: /docling/chunk
    input:
      document: { $step: "assemble", path: "document_dict" }
      chunk_options: { $input: "chunk_options" }
```

### Component Specifications

#### `preprocess_pages` (NEW — shared)

Fetches the source document once, extracts per-page data, stores each page as an individual blob.

**Input:**
- `source` — Blob reference, URL, or base64 content
- `source_kind` — `"blob"`, `"url"`, or `"base64"`
- `pipeline_config` — (optional) Named config from classify step

**Processing:**
1. Fetch full PDF bytes (one blob GET)
2. Create `PdfDocument` (reads xref table only — fast)
3. For each page N:
   - `load_page(N)` — loads just that page
   - Extract page image at configured `images_scale` (default 2.0)
   - Extract programmatic text cells (character/word/line level with bounding boxes) via the configured docling-parse backend (default: `dlparse_v2`, passed through from `pipeline_config` — do NOT hardcode V4, see note below)
   - Extract page dimensions
   - Store as blob: `{page_no, image_bytes, text_cells, dimensions}`
4. Return ordered list of page blob references

**Output:**
```json
{
  "page_count": 50,
  "pages": [
    {"page_no": 1, "blob_ref": "$blob:abc123"},
    {"page_no": 2, "blob_ref": "$blob:def456"},
    ...
  ]
}
```

**Performance:** Serial, I/O-bound. pypdfium2 renders at ~50-100ms/page, so 500 pages ≈ 25-50s. This is the cost of avoiding 25GB of blob traffic (vs. 500 workers each fetching a 50MB PDF).

**Note — backend selection:** Always pass the `pdf_backend` value from `pipeline_config` through to the backend constructor. The docling-serve default is `dlparse_v2`. `dlparse_v4` is a valid enum value but has a known memory accumulation issue on long documents (>20GB for 500-page PDFs, docling issue #2077) — exactly the documents that motivate fan-out. Using it unconditionally would violate parity for default-configured clients and introduce a memory regression.

**Note — page indexing:** The backend `load_page()` API is **0-indexed**. DoclingDocument page numbers are 1-indexed. Always call `backend.load_page(page_no - 1)` — this is how `PreprocessThreadedStage` calls it internally.

#### `per_page_inference` (NEW — standard pipeline only)

Runs the expensive ML models on a single page.

**Input:** Page blob reference (containing image, text cells, dimensions)

**Processing:**
1. Fetch page blob
2. Run OCR on page image (if enabled)
3. Run layout model (HERON/EGRET) — produces bounding boxes for text, figures, tables, headers
4. For each detected table: run TableFormer — produces row/column structure
5. Run page assembly — combines all results into `page.assembled` structure

**Output:** Serialized page assembly result (layout clusters with bounding boxes, text cells mapped to clusters, table structures with row/column spans). Stored as blob.

**Notes:** This component invokes the same model classes that `StandardPdfPipeline` uses internally, but directly — without the `ThreadedPipelineStage` wrappers. The relevant classes are `PagePreprocessingModel`, `BaseOcrModel` (and its implementations), `LayoutModel`, `TableStructureModel`, and `PageAssembleModel`. The canonical reference for how to initialize them standalone is `StandardPdfPipeline._init_models()` — that method instantiates all five models in one place. The `ConverterCache` pattern still applies for model loading across tasks.

#### `vlm_infer_page` (NEW — VLM pipeline only)

Thin HTTP client to a VLM serving endpoint.

**Input:** Page blob reference (containing page image)

**Processing:**
1. Fetch page blob, extract image
2. POST to VLM endpoint (VLLM, Ollama, or any OpenAI-compatible `/v1/chat/completions` API)
3. Parse response — extract DocTags markup

**Output:** `{page_no, doctags_string, image_dimensions}`

**Notes:** No ML model loading on the worker. The VLM serving infrastructure is a separate deployment concern (see "VLM Serving" section). The component supports the same endpoint configuration as docling's `ApiVlmModel`: URL, params, headers, timeout, concurrency.

#### `assemble_standard` (NEW)

Takes ordered per-page inference results and produces a fully assembled `DoclingDocument` with correct cross-page reading order.

**Input:** Ordered list of per-page assembly results + pipeline config + export options

**Processing:**
1. Deserialize per-page results from blobs
2. Run `_integrate_results()` — final per-page cleanup/integration step, merges page elements into `ConversionResult.assembled`
3. Run `LayoutPostprocessor` (`docling/utils/layout_postprocessor.py`) via `_assemble_document()` — overlap resolution, cell-to-cluster mapping, hierarchy merging
4. Run `ReadingOrderModel` (`docling/models/readingorder_model.py`) via `_assemble_document()` — cross-page element sorting, caption association, hyphenation merging, list grouping, header/footer classification
5. Construct `DoclingDocument` from assembled structure
6. Run exports (markdown, HTML, etc.) per requested `to_formats`

**Output:** Same shape as current `convert` component: `{document, document_dict, status, errors, processing_time, timings}`

**Notes:** `ReadingOrderModel` and `LayoutPostprocessor` are pure computation (spatial sorting, R-tree indexing). No GPU, no large model weights. They need to be instantiated outside of `StandardPdfPipeline.__init__()` — initialization requirements need verification (`StandardPdfPipeline._init_models()` is the reference).

This is the most complex new component. The two key methods from `standard_pdf_pipeline.py` run in this order:
1. `_integrate_results()` — final per-page cleanup/integration step, merges page elements into `ConversionResult.assembled`
2. `_assemble_document()` — runs `LayoutPostprocessor`, then `ReadingOrderModel`, constructs `DoclingDocument`, generates page/element images, aggregates confidence scores

Method names are stable; avoid relying on specific line numbers as these shift with each release. The implementation should trace these methods in order.

#### `assemble_doctags` (NEW)

Takes ordered per-page DocTags strings and produces a `DoclingDocument`.

**Input:** Ordered list of `{page_no, doctags_string, image_dimensions}`

**Processing:**
1. Collect DocTags strings and page images in order
2. Call `DocTagsDocument.from_doctags_and_image_pairs(doctags_strings, page_images)` — returns a `DocTagsDocument` (not yet a `DoclingDocument`)
3. Call `DoclingDocument.load_from_doctags(doctags_doc, document_name=name)` — converts to the final `DoclingDocument`
4. Optionally extract text from PDF backend if `force_backend_text=True`
5. Run exports per requested `to_formats`

**Output:** Same shape as `assemble_standard`.

**Notes:** Simpler than `assemble_standard` — DocTags already encode element types, hierarchy, and bounding boxes. Reading order is implicit in the DocTags sequence. No `ReadingOrderModel` needed.

**Import paths:** `DocTagsDocument` lives in `docling_core.types.doc.document` (note the `.document` submodule, not the top-level `docling_core.types.doc`). `DoclingDocument` is in `docling_core.types.doc`. Both steps are required — calling only `from_doctags_and_image_pairs()` and treating the result as a `DoclingDocument` will fail at runtime:
```python
from docling_core.types.doc import DoclingDocument
from docling_core.types.doc.document import DocTagsDocument

doctags_doc = DocTagsDocument.from_doctags_and_image_pairs(doctags_strings, page_images)
doc = DoclingDocument.load_from_doctags(doctags_doc, document_name="document")
```

#### `chunk` (EXISTING — unchanged)

`HybridChunker` on the assembled `DoclingDocument`. Operates on the full document's semantic element tree. Cross-page chunk merging (heading inheritance, undersized chunk combination) works correctly because it receives a fully assembled document with correct reading order.

### Worker Topology

**Phase 2: Shared workers (Option 3).** All workers load the standard ML models (OCR, layout, table). They handle `per_page_inference` locally. For VLM (Phase 3), the same workers handle `vlm_infer_page` as HTTP calls to a remote VLLM server — no additional model loading.

**Future: Separate pools (Option 2).** Add a second `type: pull` plugin (`docling-vlm` queue) with dedicated VLM worker pods that host the model locally. The component interface (`vlm_infer_page` takes page image, returns DocTags) doesn't change. Migration requires only K8s manifest changes and orchestrator config, no code changes.

### VLM Serving (Phase 3)

A future `integrations/vlm-inference-server/` integration packages:
- VLLM deployment manifests for running GraniteDocling-258M as a service
- K8s Service + health checks
- Configuration for Ollama, LM Studio, or cloud API alternatives

This is intentionally a separate integration from the docling worker — it's reusable by any Stepflow component needing VLM inference. The `vlm_infer_page` component is agnostic to how the VLM is hosted.

### Page Distribution Design

**Decision: preprocess extracts pages, stores as individual blobs (Option B).**

The alternative — each worker fetches the full PDF and loads its assigned page — was rejected. While pypdfium2 supports lazy page loading (`PdfDocument()` reads only the xref table, `load_page(N)` loads just that page), the full PDF bytes still transit the blob store for each worker. A 500-page × 50MB document means 500 × 50MB = 25GB of blob traffic. Under Option B: one 50MB read + 500 × ~500KB page blob writes + 500 × ~500KB reads ≈ 550MB total — a 45× reduction.

**Cross-page boundaries are not a concern for the fan-out phase.** All cross-page work (reading order, hyphenation merging, caption association) happens in the assembly component AFTER fan-in on the complete ordered result set.

**Secondary benefit:** Page blobs are reusable across pipeline runs with different options (e.g., FAST vs ACCURATE table mode on the same document). This is a future optimization.

## Phasing

### Phase 1: Proposal + Current State Documentation (this document)

Document the deployed architecture, the docling pipeline research findings, and the dual-pipeline design decisions. Mark `docling-step-worker.md` and `stepflow-docling-namespace.md` as deprecated.

### Phase 2: Standard Pipeline Disaggregation

Implement the standard pipeline fan-out. Components: `preprocess_pages`, `per_page_inference`, `assemble_standard`. New flow YAML. MapComponent wiring.

**This phase is done when:** The fan-out flow produces identical output to the current monolithic `convert` component. `test-parity.py` passes against the new flow. Observability shows per-page tasks distributed across workers.

**Key risks:**
1. Serializing `page.assembled` through the blob store — docling's internal types (`DocItemLabel`, `Cluster`, `TableCell`) must be JSON-serializable. They're Pydantic-based (docling-core), which is promising.
2. Instantiating `ReadingOrderModel` and `LayoutPostprocessor` outside `StandardPdfPipeline.__init__()`. Both are pure computation, but their initialization path needs verification.
3. `MapComponent.execute_batch()` — implemented in `stepflow-plugin/src/context.rs` (not in builtins as previously thought). Confirmed working. See note below on `max_concurrency`.
4. Intermediate data contract — the per-page result schema must be well-defined and versioned.
5. **`max_concurrency` not yet enforced in `execute_batch`** — `context.rs` accepts the parameter but has a `// TODO: Pass max_concurrency to subflow submission` comment. For a 500-page document this means 500 concurrent subflows all competing for blob store writes simultaneously. The fix is a semaphore in `execute_via_channel` gating the submission rate. Must be resolved before testing with large documents. See also "Future Ideas Under Discussion".

### Phase 3: VLM Pipeline

Add the VLM path. Components: `vlm_infer_page`, `assemble_doctags`. New flow YAML. Facade routing. VLM serving integration.

**This phase is done when:** The VLM flow produces valid `DoclingDocument` output. The facade routes `/v1/vlm/convert/source` to the VLM flow. A VLLM deployment serves GraniteDocling in the cluster.

**Why Phase 3 and not Phase 2:** The VLM path is simpler — `vlm_infer_page` is an HTTP POST returning a string, `assemble_doctags` calls one well-defined API. Building the fan-out skeleton for the harder standard pipeline first ensures the architecture handles complex intermediate data. The VLM path then slots in trivially.

## Enrichment Models in the Disaggregated Context

### What Enrichment Models Are

Docling's pipeline has a third optional phase that runs after document assembly. These models operate on fully assembled `DoclingDocument` elements — not on raw pages — and are all **disabled by default**. They are gated individually by `PdfPipelineOptions` flags:

| Model | Gate flag | Default | Notes |
|---|---|---|---|
| `DocumentPictureClassifier` | `do_picture_classification` | `False` | EfficientNet-B0, 16 picture classes |
| `PictureDescriptionVlmModel` | `do_picture_description` | `False` | VLM captioning; supports inline HF models or OpenAI-compatible API |
| `CodeFormulaModel` | `do_code_enrichment` / `do_formula_enrichment` | `False` / `False` | Single model, two flags; HF `AutoModelForImageTextToText` |

### The Page Backend Dependency

All three enrichment models crop image regions from page images to do their work. In the monolithic pipeline, they can do this because `_release_page_resources()` (called after `PageAssembleModel`) sets `keep_backend=True` when any enrichment model is enabled — the page backend stays in memory through the enrichment phase.

In the disaggregated design, `per_page_inference` workers do not hold page backends after they return their results. The page blob contains the **rendered page image**, not a live backend handle. This means:

**Enrichment models cannot run in `per_page_inference`. They must run in `assemble_standard`.**

`assemble_standard` already has access to the page images (they are in the page blobs), so it can crop element bounding boxes from them to feed enrichment models. The flow is:

1. Fan-in completes — `assemble_standard` has all per-page blobs
2. `_integrate_results()` + `_assemble_document()` → `DoclingDocument` constructed
3. If any enrichment flags are enabled: run enrichment models using page images from blobs
4. Export to requested `to_formats`

### Implementation Implications

**`assemble_standard` must receive enrichment flags.** The input schema needs `do_picture_classification`, `do_picture_description`, `do_code_enrichment`, `do_formula_enrichment` from the original request options — either passed directly or forwarded through `pipeline_config`.

**Page images must be retained in blobs until assembly completes.** Option B's page blob design already satisfies this — blobs persist until the assembly component finishes. No architectural change needed, but blob TTL policy (a future concern, see Blob Store section below) must not expire page blobs before enrichment runs.

**`CodeFormulaModel` requires `keep_backend=True` in the original pipeline** because it needs PDF backend access for some operations. In the disaggregated design, this translates to: the page image in the blob is sufficient for code/formula crops, but any operation requiring the raw PDF backend (e.g., text cell access) is not available post-fan-in. This needs verification when implementing enrichment in `assemble_standard`.

**`PictureDescriptionVlmModel` with API backend** requires `enable_remote_services=True` on `PdfPipelineOptions`. When not set, docling raises `OperationNotAllowed`. This is an external HTTP call — structurally similar to `vlm_infer_page`. At high enrichment volume it may warrant its own worker queue rather than blocking `assemble_standard`. This is a future optimization.

### Default Behaviour and Parity

Since all enrichment models default to `False`, the initial Phase 2 implementation of `assemble_standard` can defer enrichment support entirely without breaking parity. A docling-serve client using default options will see identical output. Enrichment support in the disaggregated path can be added as a follow-on once the core fan-out is validated.

## Blob Store Considerations (Future)

The in-memory blob store is sufficient for the local kind cluster and development. At scale, per-page blob traffic becomes significant: a 500-page document produces ~500 page blobs during preprocess, ~500 result blobs during inference, all consumed by assembly. Consider:
- Blob TTL / auto-expiry for intermediate page blobs (they're only needed until assembly completes)
- Blob store backend selection (in-memory vs persistent) based on document size
- Compression of page blobs (page images are the dominant size)

These are not blockers for the initial implementation but should be tracked as the system handles larger workloads.

## Future Ideas Under Discussion

This section captures ideas that are deliberately deferred from the current proposal scope. They are worth preserving for future sessions rather than losing in conversation.

### Short-Document Fan-Out Bypass

For documents below a page threshold (likely 3-5 pages), the fan-out overhead — blob store round-trips for page extraction, MapComponent subflow setup, assembly component startup — may exceed the ML inference savings. The monolithic `convert` component path should remain registered, and a future optimisation could route short documents directly to it, bypassing the fan-out flow entirely. The classify step already has visibility into page count, making it a natural routing point.

Deliberately deferred: adds conditional routing complexity and the threshold requires empirical data from real workloads to set correctly. For now, every document regardless of page count goes through the fan-out path. "Every page is a page."

### `max_concurrency` Enforcement in `MapComponent`

`execute_batch` in `context.rs` currently accepts `max_concurrency` but has a `// TODO: Pass max_concurrency to subflow submission` comment — the parameter is not yet enforced. For a 500-page document this means 500 concurrent subflows all competing for blob store writes simultaneously. This is a real Phase 2 implementation item and should be resolved before testing with large documents. The fix is a semaphore in `execute_via_channel` gating the submission rate.

### `preprocess_pages` in Rust via `pdfium-render`

`preprocess_pages` is the serial bottleneck that scales with document size: N page renders + N concurrent blob writes. The PDF rendering is already C++ (pdfium) regardless of language — pypdfium2 is a thin Python wrapper. The `pdfium-render` Rust crate wraps the same underlying library. A Rust implementation would benefit from native async concurrent blob writes without `asyncio.to_thread` overhead, and would scale better as document size grows. Most compelling for the large-document case (100+ pages) that motivates fan-out in the first place. Track after Phase 2 is validated.

### `/docling/chunk` as a Rust Worker Component

`HybridChunker` is algorithm + token counting — pure CPU, no ML, no GPU path. The `tokenizers` crate is literally the Rust library that `pip install tokenizers` wraps. A Rust `chunk` component would eliminate the Python worker overhead and could run in the same pod as `assemble_standard` to avoid a blob store write/read cycle for the assembled document dict. This is the highest-payoff Rust component because:
- No new dependencies: `tokenizers` crate already exists
- Well-encapsulated: input is a `DoclingDocument` JSON blob, output is a chunk list
- Naturally CPU-bound: no GPU consideration needed
- HybridChunker logic is algorithmic and straightforward to port

Note: this runs as a proper gRPC pull worker, not as an in-process builtin, per the principle that no flow execution runs in the orchestrator.

### Rust Inference via `ort` Crate (ONNX Runtime Bindings)

Docling's layout model (HERON/RT-DETR) and table structure model (TableFormer) are already ONNX models — no conversion step needed. The `ort` crate provides production-grade Rust bindings for ONNX Runtime and supports CUDA, TensorRT, and OpenVINO execution providers. This is the right Rust path for running the models in `per_page_inference` natively, not Candle (see below). Relevant when considering a full Rust `per_page_inference` worker that eliminates the Python process entirely.

### HuggingFace Candle for Enrichment Models

Candle is HuggingFace's Rust ML framework — actively developed, production-ready, CUDA/Metal support, genuinely no longer Python-only. However, Candle is optimised for Transformer/LLM-style architectures. The docling models relevant to `per_page_inference` (HERON RT-DETR layout, TableFormer table structure) are object-detection models best served by the `ort` crate via their existing ONNX representations.

Candle becomes relevant for the enrichment model path: `CodeFormulaModel` and `PictureDescriptionVlmModel` use HuggingFace Transformers and could eventually run natively in Rust via Candle. For models that are already ONNX (`ort`) or need to stay in the HuggingFace Python ecosystem for now, Candle is lower priority. Worth revisiting when enrichment support is being added to `assemble_standard` in earnest.

### Per-Model Fan-Out Within `per_page_inference`

For workloads dominated by table-heavy documents (financial reports, regulatory filings), individual pages with many complex tables become stragglers under page-level fan-out. A deeper decomposition would fan out at the model level: layout inference → per-table TableFormer inference → page assembly, with each stage having its own worker queue and independent scaling.

This would require owning the coordination between model stages and tracking docling's model API surface as it evolves. The right trigger is profiling data from real workloads showing table structure as the dominant bottleneck. Deliberately deferred until page-level fan-out is validated and real traffic patterns are understood.

## Success Criteria

1. **Functional parity:** Standard fan-out flow produces output identical to monolithic convert. `test-parity.py` passes.
2. **Performance improvement:** A 50-page document completes faster with 3 workers than with 1 (inference time divides, bounded by preprocess + assembly overhead).
3. **Observability:** Per-page tasks visible in Jaeger traces. Queue depth and worker utilization visible in Grafana. Dispatched-vs-pulled correlation healthy.
4. **Dual-path routing:** Facade correctly routes standard and VLM requests to their respective flows.
5. **Assembly independence:** Both `assemble_standard` and `assemble_doctags` produce valid `DoclingDocument` output that the existing `chunk` component handles correctly.
