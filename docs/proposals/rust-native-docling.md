# Proposal: Rust-Native Docling Integration

**Status:** Draft  
**Authors:** Nate McCall  
**Created:** February 2026  
**Prerequisite:** Existing `integrations/docling/` package and K8s deployment (see `docling-integration.md`)

## Summary

This proposal describes incrementally replacing the Python-based Docling integration with Rust, moving from a thin HTTP proxy to native document processing over three phases. Each phase delivers standalone value while structurally enabling the next. The entire approach works **without forking or modifying any docling project code**.

**Current state:** Python `StepflowDoclingServer` → HTTP → `docling-serve` sidecar (~31s/paper avg on CPU)  
**End state:** Rust component server with native PDF parsing, ONNX inference, and chunking — falling back to docling-serve for unsupported formats.

## Motivation

### Operational Overhead

The current Python-based `StepflowDoclingServer` adds significant operational weight to each docling-worker pod. The worker itself is a thin HTTP proxy — it receives Stepflow JSON-RPC requests, reformats them, and forwards to the docling-serve sidecar on localhost. Despite this simplicity, it requires a full Python runtime, `uv` dependency resolution, and ~150-200MB of memory overhead.

### Performance at Scale

From our K8s performance testing (`examples/production/k8s/run-pdf-workflow.sh`), the sweet spot for our Kind node is 3 docling pods with parallelism=4, yielding ~31s/paper. CPU contention is the binding constraint — at `OMP_NUM_THREADS=4, cpu limit=4000m`, adding a 4th pod saturates the node rather than improving throughput. Reducing per-pod overhead directly increases the headroom available for actual document processing.

### Path Toward Native Processing

As document ingestion scales, the HTTP round-trip through docling-serve adds latency and complexity. A Rust-based server creates the foundation for eventually performing document processing natively — particularly chunking (CPU-bound, no ML) and ONNX inference (the layout model already uses ONNX Runtime).

## Background

### Current Architecture

```
docling-worker pod
├── StepflowDoclingServer (Python)
│   └── httpx async client → docling-serve HTTP API
│   └── Input normalization, output formatting
│   └── 4 components matching Langflow class names
└── docling-serve sidecar
    └── Layout analysis (ONNX Runtime)
    └── TableFormer (PyTorch)
    └── OCR (EasyOCR/Tesseract)
    └── docling-parse PDF backend
```

### Performance Baseline

From the Docling technical report, per-component timing on CPU:

| Component | CPU Time (per unit) | Frequency |
|-----------|-------------------|-----------|
| Layout analysis (ONNX) | 633ms / page | Every page |
| TableFormer (PyTorch) | 1.74s / table | ~28% of pages |
| OCR - EasyOCR | 13s / page | Only scanned pages |
| PDF backend (docling-parse) | ~100ms / page | Every page |

For born-digital PDFs without tables, processing is ~733ms/page. Our 31s/paper average includes table-heavy arxiv papers. Enterprise DOCX/PDF without complex tables would be significantly faster.

### Key Discovery: Models Already in ONNX Format

The layout analysis model — the most frequently invoked ML component — **already runs on ONNX Runtime** in docling's own pipeline. From the Docling technical report:

> *"For inference, our implementation relies on the onnxruntime. The Docling pipeline feeds page images at 72 dpi resolution, which can be processed on a single CPU with sub-second latency."*

TableFormer uses PyTorch natively, but community ONNX exports exist on HuggingFace. This means a Rust-native inference path does not require model conversion work — only loading existing ONNX weights via the `ort` crate.

### DoclingDocument Schema

A published JSON schema exists at [`docling-core/docs/DoclingDocument.json`](https://github.com/docling-project/docling-core/blob/main/docs/DoclingDocument.json) (currently v1.9.0). The `DoclingDocument` is a Pydantic model with well-defined structure:

- **Content items:** `texts` (TextItem), `tables` (TableItem), `pictures` (PictureItem), `key_value_items`
- **Content structure:** `body` tree, `furniture` tree, `groups` — all using JSON pointer refs (`#/texts/0`, etc.)
- **Provenance:** Bounding boxes, page numbers, confidence scores

This schema enables generating Rust types via `serde` without any Python dependency.

## Design

### Core Abstraction: `DocumentProcessor` Trait

The central design decision is a trait that enables incremental migration. Phase 1 implements only the proxy variant; subsequent phases substitute native implementations behind the same interface.

```rust
#[async_trait]
pub trait DocumentProcessor: Send + Sync {
    async fn convert(
        &self,
        sources: Vec<DocumentSource>,
        options: Option<ConversionOptions>,
    ) -> Result<ConversionResult>;

    async fn chunk(
        &self,
        sources: Vec<DocumentSource>,
        chunker_type: ChunkerType,
    ) -> Result<ChunkResult>;

    async fn export(
        &self,
        data: ExportInput,
        format: ExportFormat,
    ) -> Result<ExportResult>;

    async fn health(&self) -> Result<HealthStatus>;
}
```

Phase 1 provides `ProxyProcessor` (HTTP → docling-serve). Phase 2 adds `NativeChunker`. Phase 3 adds `OnnxProcessor`. A `HybridProcessor` composes these, routing each operation to the best available backend:

```rust
pub struct HybridProcessor {
    proxy: ProxyProcessor,
    chunker: Option<NativeChunker>,
    onnx: Option<OnnxProcessor>,
}
```

### Crate Structure

```
stepflow-rs/crates/stepflow-docling/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── main.rs                  # Binary entrypoint
│   ├── error.rs                 # DoclingError types (thiserror + error-stack)
│   ├── config.rs                # Config (env vars, CLI args)
│   ├── components.rs            # Component registration & dispatch
│   ├── models/                  # Docling data types (from JSON schema)
│   │   ├── mod.rs
│   │   ├── document.rs          # DoclingDocument, TextItem, TableItem, etc.
│   │   ├── conversion.rs        # ConversionOptions, DocumentSource
│   │   ├── chunking.rs          # ChunkerType, Chunk, ChunkMeta
│   │   └── export.rs            # ExportFormat types
│   └── processing/              # DocumentProcessor implementations
│       ├── mod.rs               # Trait definition + HybridProcessor
│       ├── proxy.rs             # Phase 1: HTTP proxy to docling-serve
│       ├── chunking.rs          # Phase 2: Native Rust chunking
│       └── inference.rs         # Phase 3: ONNX inference
```

### Component Registration

Components match existing Langflow class names exactly, maintaining routing compatibility:

| Component Name | Operation |
|---------------|-----------|
| `DoclingInlineComponent` | `convert()` |
| `DoclingRemoteComponent` | `convert()` |
| `ChunkDoclingDocument` | `chunk()` |
| `ExportDoclingDocument` | `export()` |

Plus lfx-style path aliases (`docling_inline/DoclingInlineComponent`, etc.) for `known_components` routing.

### Configuration

The Stepflow plugin config remains unchanged — the Rust binary is a drop-in replacement:

```yaml
plugins:
  docling_k8s:
    type: stepflow
    url: "http://docling-load-balancer.stepflow.svc.cluster.local:8080"

routes:
  "/langflow/core/lfx/components/docling/{*component}":
    - plugin: docling_k8s
```

## Phased Implementation

### Phase 1: Rust Component Server (HTTP Proxy)

**Goal:** Replace the Python `StepflowDoclingServer` + `DoclingServeClient` with a Rust binary that implements the Stepflow JSON-RPC protocol and proxies requests to docling-serve.

**Scope:**
- Implement `ProxyProcessor` using `reqwest` to call docling-serve's v1 API
- Port input normalization logic (URL, base64, bytes, nested Langflow format)
- Port output formatting for Langflow compatibility
- Register all 8 component names (4 primary + 4 lfx aliases)
- Build on `stepflow-server` crate patterns for the JSON-RPC server side
- Docker multi-stage build producing a minimal container image

**Projected operational impact:**

| Metric | Python (current) | Rust (projected) |
|--------|-----------------|-------------------|
| Container image size | ~800MB | ~30MB |
| Pod startup time | 3-5s | <100ms |
| Memory per worker | 150-200MB RSS | 10-20MB RSS |
| Cold-start penalty | 2-3s Python overhead | Eliminated |

**No impact on per-document processing time** — the bottleneck remains inside docling-serve. The win is purely operational: smaller images, faster scaling, more CPU/memory headroom for the sidecar.

**Testing strategy:**
- Unit tests: mock `DocumentProcessor`, test input normalization and output formatting
- Integration tests: run against docling-serve in Docker
- Compatibility: run `run-pdf-workflow.sh` and compare output structure

### Phase 2: Native Rust Chunking

**Goal:** Implement document chunking natively in Rust, eliminating the docling-serve round-trip for chunking operations.

**Why chunking first:**
- Pure computation — no ML models, no GPU, no external dependencies
- CPU-bound — benefits directly from Rust's zero-overhead abstractions
- GIL-free parallelism — chunk multiple documents concurrently without contention
- Well-defined interface — takes DoclingDocument JSON, returns chunks
- High-frequency — every document in a RAG pipeline goes through chunking

**Data flow (no Python needed):**

```
docling-serve (convert) → DoclingDocument JSON → Rust deserialize → NativeChunker → chunks
```

**HybridChunker algorithm** (re-implemented from MIT-licensed `docling-core` specification):

1. **Hierarchical pass:** one chunk per document element, merge list items
2. **Split pass:** split oversized chunks at token boundaries
3. **Merge pass:** merge undersized peer chunks sharing the same heading context
4. **Contextualize:** prepend heading hierarchy + captions to each chunk's serialized text

**Tokenizer:** HuggingFace `tokenizers` Rust crate. This is the *native* Rust implementation — the Python `tokenizers` library wraps it. Aligns with docling's own HybridChunker which supports HuggingFace tokenizers.

**Optional: PyO3 bridge.** If Python consumers should also benefit from the Rust chunker, we can expose it via PyO3 as a `stepflow_chunking` Python module. This is an enhancement, not a requirement. The primary path is pure Rust consuming DoclingDocument JSON.

**Expected performance (chunking only):**

| Metric | Python (docling) | Rust (native) |
|--------|-----------------|---------------|
| Single document | ~50-200ms | ~2-10ms |
| 100 docs concurrent | GIL-bound, sequential | Truly parallel |

This won't dramatically change per-paper wall clock time (chunking is a fraction of the 31s), but matters at scale in RAG pipelines processing thousands of documents.

### Phase 3: ONNX-Based Native Inference

**Goal:** Replace docling-serve's ML inference with Rust-native ONNX Runtime for the most common processing path (born-digital PDF → structured text), retaining docling-serve as fallback.

**Scope — the "80% path" (born-digital PDFs):**

1. PDF page rasterization → `pdfium-render` crate
2. Layout analysis → `ort` crate loading existing ONNX weights from HuggingFace
3. Text extraction → PDF text layer (no OCR needed for born-digital)
4. Table structure → `ort` with community ONNX TableFormer weights
5. Post-processing → NMS on bounding boxes, region-text intersection
6. Assembly → DoclingDocument JSON output

**What falls outside the 80% (automatic fallback to docling-serve):**
- Scanned PDFs requiring OCR
- DOCX, PPTX parsing (docling has custom parsers)
- Images requiring full OCR pipeline
- Forced OCR mode (`force_ocr: true`)

**Key Rust dependencies:**

| Crate | Purpose | Status |
|-------|---------|--------|
| `ort` | ONNX Runtime bindings | Stable, production-ready |
| `pdfium-render` | PDF → image rasterization | Stable, wraps Google's pdfium |
| `image` | Image preprocessing | Very stable |
| `tokenizers` | HuggingFace tokenizers | Production-ready, native Rust |
| `ndarray` | Tensor operations | Stable |

**Graceful degradation pattern:**

```rust
async fn convert(&self, sources, options) -> Result<ConversionResult> {
    if self.can_process_natively(&sources, &options) {
        match self.onnx.convert(sources, options).await {
            Ok(result) => Ok(result),
            Err(e) => {
                log::warn!("Native processing failed, falling back: {}", e);
                self.proxy.convert(sources, options).await
            }
        }
    } else {
        self.proxy.convert(sources, options).await
    }
}
```

**Prerequisites before committing to Phase 3:**
1. Profile docling-serve to confirm where the 31s actually goes per component
2. Verify ONNX model weights load correctly via `ort` with expected accuracy
3. Analyze actual document corpus distribution (% born-digital PDF vs. scanned vs. DOCX)

## No-Fork Analysis

A critical constraint: all three phases must work **without modifying any docling project code**. This is achievable because we consume only public interfaces.

### Phase 1: No fork required ✅

Consumes docling-serve's documented REST API. The Rust server is a drop-in replacement for the Python `StepflowDoclingServer`. The only contracts are the HTTP API and the Stepflow JSON-RPC protocol.

### Phase 2: No fork required ✅

Re-implements the chunking algorithm in Rust using:
- Published `DoclingDocument` JSON schema → Rust types via `serde`
- HybridChunker algorithm documented in docling concepts and MIT-licensed source
- HuggingFace `tokenizers` crate (this IS the native Rust implementation)

Chunking operates on the *serialized* DoclingDocument. We deserialize JSON, walk the tree, and produce chunks. The `docling-core` source serves as our specification, not a runtime dependency.

### Phase 3: No fork required ✅

Consumes publicly available artifacts:
- Layout analysis ONNX model weights from HuggingFace (`ds4sd/docling-models`)
- TableFormer ONNX weights (community exports on HuggingFace)
- `ort` crate for inference, `pdfium-render` for PDF handling
- Pre/post-processing pipeline re-implemented from technical report + MIT source

The fallback pattern eliminates the need for feature parity. Anything we can't handle natively routes to docling-serve automatically.

**Summary table:**

| Capability | Fork needed? | Approach |
|-----------|-------------|----------|
| HTTP proxy to docling-serve | No | Public REST API |
| DoclingDocument deserialization | No | Published JSON schema |
| Chunking algorithm | No | Re-implement from MIT-licensed spec |
| HuggingFace tokenizer | No | `tokenizers` crate IS the native impl |
| Layout analysis inference | No | ONNX weights on HuggingFace + `ort` |
| TableFormer inference | No | Community ONNX exports + `ort` |
| PDF text extraction | No | `pdfium-render` or `docling-parse` via FFI |
| Pre/post-processing | No | Re-implement from technical report |
| DOCX/PPTX parsing | No | Fallback to docling-serve |
| OCR pipeline | No | Fallback to docling-serve |

## Python ↔ Rust Interop (Phase 2, Optional)

If we want Python consumers to use the Rust chunker, [PyO3](https://pyo3.rs) compiles Rust code to a Python extension module. No HTTP, no subprocess — Python calls Rust directly:

```rust
use pyo3::prelude::*;

#[pyclass]
struct HybridChunker { max_tokens: usize }

#[pymethods]
impl HybridChunker {
    #[new]
    fn new(max_tokens: usize) -> Self { Self { max_tokens } }

    fn chunk(&self, document_json: &str) -> PyResult<Vec<Chunk>> {
        let doc: DoclingDocument = serde_json::from_str(document_json)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(self.chunk_internal(&doc))
    }
}
```

**Data transfer recommendation:** JSON serialization across the boundary. ~1-5ms overhead for a typical document, maximum decoupling, easy to evolve independently. [maturin](https://www.maturin.rs/) handles cross-compilation and wheel packaging.

## Migration Strategy

### Parallel Deployment

During each phase transition, run both implementations side-by-side using weighted routing:

```yaml
routes:
  "/langflow/core/lfx/components/docling/{*component}":
    - plugin: docling_rust
      weight: 10
    - plugin: docling_python
      weight: 90
```

### Validation

For each phase:
1. **Output equivalence:** same input → same structured output
2. **Performance comparison:** run `run-pdf-workflow.sh` against both
3. **Error handling parity:** same error conditions produce appropriate errors
4. **Observability:** traces and metrics comparable between implementations

### Rollback

Each phase is independently deployable. Rollback = change the K8s deployment to point at the Python image.

## Future Consideration: Granite-Docling

IBM recently released [Granite-Docling-258M](https://www.ibm.com/new/announcements/granite-docling-end-to-end-document-conversion), an ultra-compact vision-language model (Apache 2.0) that does document understanding in a single pass — replacing the multi-model pipeline (layout + TableFormer + OCR) with one VLM. At 258M parameters, it's feasible to run via [candle](https://github.com/huggingface/candle) (HuggingFace's Rust ML framework) on CPU.

This could simplify Phase 3 further: instead of orchestrating three separate ONNX models, run a single small VLM. However, the multi-model pipeline is proven and well-understood while Granite-Docling is new. Worth monitoring as it matures.

## Open Questions

1. **Profiling data needed:** Before committing to Phase 3 scope, instrument docling-serve to determine exact per-component time breakdown on our actual workloads. This determines whether layout analysis, table recognition, or something else is the highest-ROI target.

2. **DOCX in scope for native processing?** Enterprise workloads are expected to be mostly DOCX + PDF. DOCX falls back to docling-serve in this proposal, but if it represents >50% of volume, a Rust DOCX parser (using the existing `docx-rs` crate) could be worth evaluating for a later phase.

3. **Model quantization:** Independent of the Rust rewrite, INT8 quantized ONNX models can be 2-4x faster on CPU. This optimization composes with Phase 3 and could be explored in parallel.

## References

- [Docling Technical Report](https://arxiv.org/abs/2408.09869) — Architecture, models, and benchmarks
- [DoclingDocument JSON Schema](https://github.com/docling-project/docling-core/blob/main/docs/DoclingDocument.json)
- [docling-core source (MIT)](https://github.com/docling-project/docling-core) — Chunking algorithm specification
- [Docling Chunking Concepts](https://docling-project.github.io/docling/concepts/chunking/)
- [Existing proposal: Docling Integration](./docling-integration.md)
- [Existing proposal: K8s Deployment](./docling-integration-k8s-followup.md)
- [Performance log](../../examples/production/k8s/) — `run-pdf-workflow.sh` results in `/tmp/stepflow-perf-log.md`
- [`stepflow-server` crate](../../stepflow-rs/crates/stepflow-server/) — Server-side reference for Phase 1
- [`stepflow-protocol` crate](../../stepflow-rs/crates/stepflow-protocol/) — JSON-RPC protocol types
- [HuggingFace `ort` crate](https://github.com/pykeio/ort) — ONNX Runtime Rust bindings
- [HuggingFace `tokenizers` crate](https://github.com/huggingface/tokenizers) — Native Rust tokenizer
- [Granite-Docling announcement](https://www.ibm.com/new/announcements/granite-docling-end-to-end-document-conversion)
