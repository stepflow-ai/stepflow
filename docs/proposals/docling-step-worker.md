# Proposal: Docling Step Worker — Native Library Integration

**Status:** Draft  
**Authors:** Nate McCall  
**Created:** February 2026  
**Prerequisite:** Existing `integrations/docling/` package and K8s deployment (see `docling-integration.md`)  
**Related:** `rust-native-docling.md` (longer-term pipeline disaggregation and Rust migration)

## Summary

Replace the current docling-serve HTTP proxy architecture with a `docling_step_worker` that wraps docling's Python library directly as Stepflow components. This eliminates the HTTP round-trip to docling-serve, enables conditional pipeline configuration per document, and establishes the flow-based orchestration pattern that later phases (pipeline disaggregation, Rust-native processing) build upon.

The key constraint: **zero duplication of docling internals.** All image manipulation, PDF parsing, model inference, and assembly logic uses docling's existing classes directly. If something can't be cleanly extracted as a separate component without reimplementing docling logic, it stays inside docling's `DocumentConverter`. Deeper disaggregation is deferred to later phases.

**Current state:** Python `StepflowDoclingServer` → HTTP → `docling-serve` sidecar  
**Target state:** Python `docling_step_worker` calling `DocumentConverter` directly, orchestrated by a Stepflow flow with conditional branching

## Motivation

### Why Now

The current architecture routes all document processing through docling-serve's HTTP API. This works, but introduces constraints that become painful as we scale:

- **HTTP serialization overhead.** Every document is base64-encoded into a JSON request body, sent to localhost, decoded by docling-serve, processed, then the result is serialized back. For large PDFs, this is measurable.
- **One-size-fits-all pipeline.** Every document runs through the same pipeline configuration regardless of content. A born-digital PDF with no tables still initializes TableFormer. A scanned document gets the same OCR settings as a clean digital one.
- **Opaque processing.** Stepflow sees a single "docling conversion" step. There's no visibility into whether time was spent on layout analysis, table extraction, OCR, or assembly.
- **Sidecar complexity.** The docling-serve sidecar adds operational weight: its own health checks, resource limits, startup ordering, and failure modes independent of the worker.

### What This Enables

- **Conditional pipeline selection.** Route documents to different `DocumentConverter` configurations based on classification: skip OCR for born-digital PDFs, use `TableFormerMode.ACCURATE` for financial reports, skip table extraction entirely for text-only documents.
- **Per-step observability.** Classification, conversion, and chunking are separate Stepflow steps with individual fastrace spans, timing, and failure tracking.
- **Document-level fan-out.** Batch processing naturally distributes across worker pods via the existing `/map` component, with Stepflow managing concurrency limits and partial failure.
- **Architectural runway.** The flow definition becomes the stable contract. Individual components can be replaced with deeper integrations (page-level fan-out, Rust-native chunking, ONNX inference) without changing the flow's external interface.

## Design

### Architecture

```
docling_step_worker pod (single container)
├── StepflowServer (Python SDK)
│   ├── /docling/classify     — lightweight document probing
│   ├── /docling/convert      — wraps DocumentConverter.convert() directly
│   └── /docling/chunk        — wraps HybridChunker
├── docling library (loaded in-process)
│   ├── StandardPdfPipeline / ThreadedStandardPdfPipeline
│   ├── Layout model (ONNX Runtime, loaded once at startup)
│   ├── TableFormer (loaded once at startup)
│   ├── OCR engine (loaded on demand)
│   └── docling-parse PDF backend
└── Model artifacts (~500MB, cached via PVC or pre-downloaded)
```

No sidecar. No HTTP proxy. The docling library runs in the same process as the Stepflow worker.

### Components

#### `/docling/classify`

Lightweight document probing to determine optimal pipeline configuration. Does not run the full conversion pipeline.

**Input:**
```json
{
  "source": "blob:sha256:abc123",
  "source_kind": "blob"
}
```

**Output:**
```json
{
  "page_count": 47,
  "has_text_layer": true,
  "estimated_tables": 12,
  "format": "pdf",
  "recommended_config": "born_digital_with_tables"
}
```

**Implementation approach:** Use `docling-parse` or `pypdfium2` directly (both are standalone libraries already used by docling) to extract basic document metadata. For table estimation, a fast heuristic approach — checking for ruling lines or cell-like structures in the text layer — avoids running the full layout model. The classification can start simple (just page count + has-text-layer) and add sophistication over time.

This is the one component that contains logic not already in docling, but it's intentionally minimal — a few dozen lines of heuristic checks, not model inference.

#### `/docling/convert`

Core conversion component wrapping `DocumentConverter` directly.

**Input:**
```json
{
  "source": "blob:sha256:abc123",
  "source_kind": "blob",
  "pipeline_options": {
    "do_ocr": false,
    "do_table_structure": true,
    "table_mode": "accurate",
    "generate_page_images": false
  }
}
```

**Output:**
```json
{
  "document": { ... },
  "status": "success",
  "page_count": 47,
  "table_count": 12,
  "processing_time_ms": 28400
}
```

The `document` field contains the serialized `DoclingDocument` (via `export_to_dict()`), which is docling's native interchange format.

**Implementation approach:** Create a `DocumentConverter` instance at worker startup with default options. On each invocation, construct `PdfPipelineOptions` from the input, override the converter's format options for this call, and invoke `converter.convert()`. All pipeline internals — page preprocessing, layout analysis, table extraction, OCR, assembly, reading order — run inside docling's own `ThreadedStandardPdfPipeline` exactly as they would in docling-serve.

The source document is retrieved from blob storage, written to a temporary file (docling expects filesystem paths or URLs), and cleaned up after conversion.

**Key design decisions:**

- **Pipeline instance reuse.** `DocumentConverter` caches initialized pipeline instances keyed by options hash. For a small set of distinct configurations (born-digital, born-digital-with-tables, scanned), the models are loaded once and reused across invocations.
- **Threaded pipeline.** Use `ThreadedStandardPdfPipeline` (not the sequential `StandardPdfPipeline`) to get docling's built-in intra-document parallelism — layout and table extraction run in separate threads with bounded queues.
- **No format transformation.** Output is docling's native `DoclingDocument` dict, not the Langflow-compatible wrapper used by the current integration. Downstream components consume the docling format directly. Langflow compatibility, if still needed, can be a thin formatting step in the flow.

#### `/docling/chunk`

Chunking component wrapping docling's `HybridChunker`.

**Input:**
```json
{
  "document": { ... },
  "chunk_options": {
    "tokenizer": "sentence-transformers/all-MiniLM-L6-v2",
    "max_tokens": 512,
    "merge_peers": true
  }
}
```

**Output:**
```json
{
  "chunks": [
    {
      "text": "...",
      "metadata": {
        "headings": ["Section 1", "Background"],
        "page": 3,
        "doc_items": [...]
      }
    }
  ],
  "chunk_count": 156
}
```

**Implementation approach:** Reconstitute the `DoclingDocument` from the dict (using `DoclingDocument.model_validate()`), then apply `HybridChunker`. This is essentially the existing `ChunkDoclingDocument` component from the current integration, adapted to consume the native docling format instead of the Langflow wrapper.

### Flow Definition

The per-document processing flow:

```yaml
name: docling-process-document
steps:
  classify:
    component: /docling/classify
    input:
      source: $input.source
      source_kind: $input.source_kind

  convert:
    component: /docling/convert
    input:
      source: $input.source
      source_kind: $input.source_kind
      pipeline_options:
        do_ocr:
          $if:
            condition: $step.classify.recommended_config == "scanned"
            then: true
            else: false
        do_table_structure:
          $if:
            condition: $step.classify.recommended_config == "born_digital_no_tables"
            then: false
            else: true
        table_mode:
          $if:
            condition: $step.classify.estimated_tables > 0
            then: "accurate"
            else: "fast"

  chunk:
    component: /docling/chunk
    input:
      document: $step.convert.document
      chunk_options: $input.chunk_options

output:
  document: $step.convert.document
  chunks: $step.chunk.chunks
  classification: $step.classify
```

For batch processing, an outer flow wraps this in `/map`:

```yaml
name: docling-batch-process
steps:
  process_all:
    component: /map
    input:
      workflow: <per-document flow above>
      items: $input.documents
      max_concurrency: $input.max_concurrency

output:
  results: $step.process_all.results
```

### Worker Lifecycle

**Startup:**
1. Download model artifacts if not cached: `StandardPdfPipeline.download_models_hf()`
2. Create `DocumentConverter` with default `PdfPipelineOptions` — this initializes the layout model and TableFormer once
3. Register components with `StepflowServer`
4. Start the HTTP server for Stepflow JSON-RPC communication

**Per-request:**
1. Stepflow routes a component execution to this worker
2. The component function runs, using the pre-loaded models
3. For `/docling/convert`, the `ThreadedStandardPdfPipeline` handles intra-document parallelism internally
4. Result is returned via JSON-RPC

**Shutdown:**
1. Graceful drain of in-flight requests
2. Model memory freed with process exit

### Data Flow

Documents and intermediate results flow through Stepflow's blob store:

```
Client uploads document
  → blob store (binary, content-addressed)
    → /docling/classify reads from blob store
    → /docling/convert reads from blob store, writes DoclingDocument to blob store
      → /docling/chunk reads DoclingDocument from blob store
        → Final result returned to client
```

Binary blobs (PDF files) use `put_blob_binary`/`get_blob_binary` for zero-overhead storage. Structured data (DoclingDocument, chunks) use `put_blob`/`get_blob` with JSON serialization.

## Relationship to Existing Work

### Current Integration (`integrations/docling/`)

The existing `StepflowDoclingServer` registers components matching Langflow class names (`DoclingInlineComponent`, `DoclingRemoteComponent`, `ChunkDoclingDocument`, `ExportDoclingDocument`) and proxies to docling-serve via HTTP.

This proposal **replaces** the proxy architecture but **reuses** the patterns:
- Same `StepflowServer` + `@server.component()` registration model
- Same blob store integration for document data
- Similar component naming (though simplified, since we don't need Langflow class name compatibility)

The existing integration can continue to serve Langflow-routed requests while the step worker handles native Stepflow flows. Both can coexist during migration.

### Rust-Native Docling Proposal (`rust-native-docling.md`)

The Rust-native proposal describes three phases of incremental migration plus pipeline disaggregation. This proposal is **complementary and preparatory**:

- The flow definitions created here become the orchestration layer that later phases plug into
- The `/docling/convert` component is the natural place to swap in deeper integrations: first page-level fan-out (using `/map`), then Rust-native chunking, then ONNX inference
- The `/docling/classify` component generates the routing decisions that the disaggregated pipeline needs
- Per-step observability data collected here directly informs Phase 3 scope decisions (which stages are worth disaggregating based on actual latency profiles)

### MapComponent

The existing `/map` component (in `stepflow-builtins/src/map.rs`) provides document-level fan-out for batch processing. It already supports `max_concurrency` in its interface (though wiring to the executor is pending — tracked as a prerequisite in the Rust-native proposal). For the batch flow, `/map` distributes documents across worker pods, each running the full per-document flow.

Page-level fan-out within a single document (the disaggregation described in the Rust-native proposal) is a future enhancement that builds on this foundation.

## Deployment

### Container Image

Single container based on the existing docling worker image, but without the docling-serve sidecar:

```dockerfile
FROM python:3.12-slim

# Install docling with all dependencies
RUN pip install docling[ocr] stepflow-py

# Pre-download model artifacts
RUN python -c "from docling.pipeline.standard_pdf_pipeline import StandardPdfPipeline; \
    StandardPdfPipeline.download_models_hf(force=True)"

COPY docling_step_worker/ /app/

ENTRYPOINT ["python", "-m", "docling_step_worker"]
```

Model artifacts (~500MB) are baked into the image to avoid download-on-first-use latency. Alternatively, a PVC-backed cache can be shared across pods.

### Resource Profile

Per-pod resource requirements (approximate):

| Resource | Current (proxy + sidecar) | Step Worker |
|----------|--------------------------|-------------|
| Memory | ~2.5GB (500MB proxy + 2GB serve) | ~2GB (single process) |
| CPU | 4 cores (shared) | 4 cores |
| Startup | ~15s (sidecar model loading) | ~10s (direct model loading) |
| Disk | ~500MB models (per pod or PVC) | Same |

Memory savings come from eliminating the separate Python runtime and HTTP server for docling-serve. The models themselves dominate memory usage and are unchanged.

### Routing Configuration

```yaml
plugins:
  docling_step:
    type: stepflow
    transport: http
    url: "http://docling-step-worker.stepflow.svc.cluster.local:8080"

routes:
  "/docling/{*component}":
    - plugin: docling_step
```

## Scope and Non-Goals

### In Scope

- Three components: `/docling/classify`, `/docling/convert`, `/docling/chunk`
- Per-document flow definition with conditional pipeline selection
- Batch flow using `/map` for document-level fan-out
- Direct `DocumentConverter` integration (no docling-serve dependency)
- Binary blob support for document data
- Basic document classification (page count, has text layer, table estimation)

### Not In Scope (Deferred to Later Phases)

- **Page-level disaggregation.** No fan-out within a single document's conversion. Docling's `ThreadedStandardPdfPipeline` handles intra-document parallelism.
- **Rust-native components.** All processing uses docling's Python library. No ONNX Runtime direct calls, no Rust chunking.
- **Custom model loading.** Uses docling's standard model artifact management. No custom model registry or hot-swapping.
- **Langflow compatibility wrappers.** The output format is docling-native (`DoclingDocument`). Langflow integration, if needed, is a separate formatting step.
- **Multi-tenant fairness controls.** Per-tenant queuing or priority scheduling at the component level. Stepflow's existing scheduling handles pod-level distribution.
- **OCR engine selection per-document.** Classification can recommend OCR on/off, but engine selection (EasyOCR vs RapidOCR vs Tesseract) is a worker-level configuration, not per-request.

## Open Questions

1. **Classification depth.** How sophisticated should `/docling/classify` be initially? A minimal version (page count + text layer presence) is trivial. Table estimation without running layout analysis requires heuristics against the PDF text layer structure. The component can start simple and gain sophistication based on observed routing accuracy.

2. **Pipeline options passthrough.** Should `/docling/convert` accept arbitrary `PdfPipelineOptions` fields, or only a curated subset? Full passthrough maximizes flexibility but exposes docling's internal configuration surface. A curated subset is more maintainable but may miss edge cases.

3. **DoclingDocument serialization size.** The full `DoclingDocument` dict can be large for document-heavy PDFs (with embedded images, etc.). Should we strip image data before passing to blob store, or let the blob store handle it? This affects the `/docling/chunk` input size.

4. **Existing integration migration.** What's the migration path for flows currently using the Langflow-compatible component names? A compatibility shim that maps old component names to the new ones, or a clean break requiring flow updates?

5. **Temp file management.** `DocumentConverter` expects filesystem paths. For documents arriving via blob store, we write to a temp file. Should this use a tmpfs mount for speed, or is disk I/O negligible compared to model inference time?
