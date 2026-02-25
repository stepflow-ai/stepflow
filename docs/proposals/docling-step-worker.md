# Proposal: Docling Step Worker — Drop-in Replacement for docling-serve

**Status:** Draft  
**Authors:** Nate McCall  
**Created:** February 2026  
**Prerequisite:** Existing `integrations/docling/` package and K8s deployment (see `docling-integration.md`)  
**Related:** `rust-native-docling.md` (longer-term pipeline disaggregation and Rust migration)

## Summary

Build a drop-in replacement for the docling-serve sidecar that exposes **the same REST API** while calling docling's `DocumentConverter` library directly in-process. The existing `integrations/docling/` package (Langflow integration) and any other callers of docling-serve's HTTP API continue to work unchanged — they just point `DOCLING_SERVE_URL` at the step worker instead of the sidecar.

**Current state:** Python `StepflowDoclingServer` → HTTP → `docling-serve` sidecar → `DocumentConverter`  
**Target state:** Python `StepflowDoclingServer` → HTTP → `docling-step-worker` → `StepflowClient` → Stepflow flow → `DocumentConverter` directly (no sidecar)

The key constraint: **callers cannot tell the difference.** The step worker accepts the same request format, returns the same response format, and supports the same options as docling-serve. Internally, all processing — sync and async — is orchestrated through Stepflow, providing distributed execution state, blob-store-backed results, and horizontal scaling that docling-serve has never achieved. This also solves docling-serve's known architectural flaw where async task state is pinned to a single process.

## Motivation

### Primary Goal: Eliminate the Sidecar While Preserving the Contract

Today, every docling-worker pod runs two processes: the Stepflow component server (Python) and a docling-serve sidecar (Python + FastAPI + Uvicorn). The component server is a thin HTTP proxy — it receives Stepflow JSON-RPC requests, reformats them, and forwards to the sidecar on localhost:5001. This is wasteful:

- **Double the Python runtime overhead.** Two separate processes, two sets of dependencies, two memory footprints.
- **HTTP serialization for in-process data.** Documents are serialized to HTTP, sent over localhost, deserialized, processed, serialized again, sent back. All within the same pod.
- **Sidecar lifecycle complexity.** Health checks, startup ordering, crash recovery — all for a localhost HTTP hop.

The step worker eliminates this by loading `DocumentConverter` directly and presenting the same HTTP interface that docling-serve provides. The existing Langflow integration (`integrations/docling/`) doesn't change — its `DoclingServeClient` already speaks the docling-serve v1 API. We just retarget `DOCLING_SERVE_URL`.

### Secondary Goal: Establish the Stepflow Flow Abstraction

Behind the HTTP facade, document processing is orchestrated as a Stepflow flow with discrete steps (convert, chunk). This creates the control plane that later improvements plug into: conditional routing, per-step observability, pipeline disaggregation, and eventually Rust-native components. But **the flow is an internal implementation detail** — external callers interact with the docling-serve REST API, not Stepflow directly.

### What Stays Unchanged

- **`integrations/docling/`** — The Langflow integration package, its `DoclingServeClient`, and all Langflow component names (`DoclingInlineComponent`, `DoclingRemoteComponent`, `ChunkDoclingDocument`, `ExportDoclingDocument`). These are not modified. They continue to speak docling-serve's HTTP API.
- **External caller contracts** — Any system hitting `POST /v1/convert/source`, `POST /v1/convert/file`, the async variants, or the chunking endpoints continues to work identically.
- **Request options surface** — `do_ocr`, `force_ocr`, `ocr_engine`, `table_mode`, `pdf_backend`, `to_formats`, `from_formats`, `image_export_mode`, `page_range`, `document_timeout`, `abort_on_error`, etc. All accepted and honored.

### Incremental Improvement Path

1. **This proposal:** Drop-in replacement. Same API, no sidecar. Validates that direct `DocumentConverter` integration produces identical output.
2. **Next:** Add classification step behind the facade. Use observability data to identify bottlenecks. Introduce conditional routing for pipeline optimization.
3. **Later:** Disaggregate the convert step — fan out layout analysis across pages using Stepflow's `/map` component.
4. **Eventually:** Swap in Rust-native components (chunking, ONNX inference) behind the same facade.

Each step is a change behind the HTTP facade. External callers never see it.

## Design

### Architecture

```
External callers                          Existing Langflow integration
(curl, Java clients, Open WebUI)         (integrations/docling/)
         │                                        │
         │  POST /v1/convert/source               │  DoclingServeClient
         │  POST /v1/convert/file                  │  (unchanged, just repoint URL)
         │  POST /v1/convert/source/async          │
         │  POST /v1/convert/chunked/...           │
         └──────────────┬─────────────────────────┘
                        │
                        ▼
         ┌──────────────────────────────────┐
         │   docling-step-worker            │
         │   FastAPI HTTP facade            │
         │                                  │
         │   Same endpoints as docling-serve│
         │   Same request/response models   │
         │   Same options surface           │
         └──────────────┬───────────────────┘
                        │
                        ▼
         ┌──────────────────────────────────┐
         │   StepflowClient                 │
         │   (Python SDK)                   │
         │                                  │
         │   Sync:  client.run(flow_id, ..) │
         │   Async: client.submit(flow_id..)│
         │   Poll:  client.get_run(run_id)  │
         └──────────────┬───────────────────┘
                        │
                        ▼
         ┌──────────────────────────────────┐
         │   Stepflow Server                │
         │   (Rust orchestrator)            │
         │                                  │
         │   Persistent execution state     │
         │   Distributed across pods        │
         │   Blob store for results         │
         └──────────────┬───────────────────┘
                        │  routes to
                        ▼
         ┌──────────────────────────────────┐
         │   /docling/convert component     │
         │   /docling/chunk component       │
         │   (registered on step worker)    │
         │                                  │
         │   DocumentConverter              │
         │   (docling library, in-process)  │
         │                                  │
         │   LRU cache of converter         │
         │   instances keyed by options     │
         │   hash (same as docling-serve)   │
         │                                  │
         │   ThreadedStandardPdfPipeline    │
         │   Layout model (ONNX, loaded once│
         │   TableFormer (loaded once)      │
         │   OCR engine (on demand)         │
         │   docling-parse PDF backend      │
         └──────────────────────────────────┘
```

No sidecar. No HTTP proxy to localhost. The docling library runs in the same process as the Stepflow component server. All processing — sync and async — is orchestrated through Stepflow, giving us distributed execution state, blob-store-backed results, and horizontal scaling that docling-serve has never achieved.

### HTTP API — docling-serve v1 Compatibility

The step worker exposes the same endpoints as docling-serve v1. The request and response models must be byte-compatible.

#### Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/v1/convert/source` | POST | Sync convert from URL or base64 source |
| `/v1/convert/file` | POST | Sync convert from multipart file upload |
| `/v1/convert/source/async` | POST | Async convert, returns task_id |
| `/v1/convert/file/async` | POST | Async convert from file, returns task_id |
| `/v1/status/poll/{task_id}` | GET | Poll async task status |
| `/v1/status/ws/{task_id}` | WS | WebSocket status updates (stretch goal) |
| `/v1/convert/chunked/{chunker_type}/source` | POST | Convert + chunk from source |
| `/v1/convert/chunked/{chunker_type}/file` | POST | Convert + chunk from file |
| `/v1/clear/converters` | POST | Clear converter cache |
| `/health` | GET | Health check |

#### Request Model — `ConvertDocumentsRequest`

Matches docling-serve exactly:

```json
{
  "options": {
    "from_formats": ["pdf", "docx", "pptx", "html", "image"],
    "to_formats": ["md", "json", "html", "text", "doctags"],
    "image_export_mode": "placeholder",
    "do_ocr": true,
    "force_ocr": false,
    "ocr_engine": "easyocr",
    "ocr_lang": ["en"],
    "pdf_backend": "dlparse_v2",
    "table_mode": "fast",
    "abort_on_error": false,
    "do_table_structure": true,
    "include_images": true,
    "images_scale": 2.0,
    "page_range": null,
    "document_timeout": null,
    "pipeline": null
  },
  "http_sources": [{"url": "https://arxiv.org/pdf/2501.17887"}],
  "file_sources": [{"base64_string": "...", "filename": "doc.pdf"}]
}
```

All options are optional. Missing options use the same defaults as docling-serve.

#### Response Model — `ConvertDocumentResponse`

Matches docling-serve exactly:

```json
{
  "document": {
    "md_content": "# Title\n\nBody text...",
    "html_content": "<h1>Title</h1><p>Body text...</p>",
    "text_content": "Title\nBody text...",
    "json_content": { ... },
    "doctags_content": "...",
  },
  "errors": [],
  "status": "success",
  "processing_time": 28.4,
  "timings": {
    "total": 28.4,
    "pdf_parse": 2.1,
    "layout_analysis": 18.3,
    "table_structure": 6.2,
    "assembly": 1.8
  }
}
```

The `document` contains rendered content in each requested `to_format`. The `json_content` field is the full `DoclingDocument` dict (via `export_to_dict()`).

### Converter Caching — LRU by Options Hash

docling-serve maintains an LRU cache of `DocumentConverter` instances keyed by the hash of the conversion options. We replicate this exactly:

```python
from functools import lru_cache

@lru_cache(maxsize=CACHE_SIZE_OPTIONS)
def _get_converter(options_hash: int) -> DocumentConverter:
    """Get or create a DocumentConverter for the given options.
    
    DocumentConverter.convert() does NOT accept per-request pipeline options.
    Options are fixed at construction time via format_options. So we maintain
    an LRU cache of converter instances, one per distinct options combination.
    This matches docling-serve's caching strategy exactly.
    """
    ...
```

When a request arrives with specific options (`do_ocr=True, table_mode="accurate"`), we:
1. Build `PdfPipelineOptions` from the request options
2. Hash the options to get a cache key
3. Look up or create a `DocumentConverter` with those options
4. Call `converter.convert(source=stream)`

For the common case where callers use default options, the same converter instance is reused for every request. The LRU evicts least-recently-used converters when the cache is full (configurable via `CACHE_SIZE_OPTIONS`, matching docling-serve's `DOCLING_SERVE_CACHE_SIZE_OPTIONS` env var).

### Response Preparation

After `DocumentConverter.convert()` returns a `ConversionResult`, we need to render the `DoclingDocument` into each requested output format. This mirrors what docling-serve's `response_preparation.py` does:

```python
def prepare_response(result: ConversionResult, options: ConvertDocumentsOptions) -> ConvertDocumentResponse:
    doc = result.document  # DoclingDocument
    
    response_doc = {}
    for fmt in options.to_formats:
        if fmt == "md":
            response_doc["md_content"] = doc.export_to_markdown(image_mode=options.image_export_mode)
        elif fmt == "html":
            response_doc["html_content"] = doc.export_to_html(image_mode=options.image_export_mode)
        elif fmt == "text":
            response_doc["text_content"] = doc.export_to_text()
        elif fmt == "json":
            response_doc["json_content"] = doc.export_to_dict()
        elif fmt == "doctags":
            response_doc["doctags_content"] = doc.export_to_document_tokens()
    
    return ConvertDocumentResponse(
        document=response_doc,
        errors=[],
        status="success",
        processing_time=result.timings.total,
        timings=result.timings,
    )
```

This is where `to_formats` matters — the same `DoclingDocument` is rendered into multiple output formats based on what the caller requested.

### Chunking Endpoints

The `/v1/convert/chunked/{chunker_type}/source` and `/file` endpoints perform conversion + chunking in one step. This matches docling-serve's chunking endpoints:

1. Convert the document (same as `/v1/convert/source`)
2. Apply `HybridChunker` or `HierarchicalChunker` to the resulting `DoclingDocument`
3. Return chunks in the `ChunkDocumentResponse` format

### Stepflow Orchestration — Fixing docling-serve's Async Architecture

All processing — sync and async — routes through Stepflow via the `StepflowClient` Python SDK. This solves a fundamental architectural flaw in docling-serve: async task state is pinned to the process that received the request, stored in an in-memory dict. It breaks with multiple Uvicorn workers, can't scale horizontally, and doesn't survive pod restarts. Even docling-serve's Redis Queue backend still relies on in-memory task registries (filed as [docling-serve #378](https://github.com/docling-project/docling-serve/issues/378), [#317](https://github.com/docling-project/docling-serve/issues/317)).

Stepflow already has distributed execution state, persistent result storage (blob store), and multi-pod scheduling. We use it for everything.

#### Endpoint → StepflowClient Method Mapping

| docling-serve endpoint | StepflowClient method | Behavior |
|---|---|---|
| `POST /v1/convert/source` (sync) | `client.run(flow_id, input)` | Submit + block until complete. Returns `ConvertDocumentResponse` directly. |
| `POST /v1/convert/file` (sync) | `client.run(flow_id, input)` | Same, after reading multipart upload into input. |
| `POST /v1/convert/source/async` | `client.submit(flow_id, input)` | Fire-and-forget. Returns `{"task_id": run_id}` immediately (HTTP 202). |
| `POST /v1/convert/file/async` | `client.submit(flow_id, input)` | Same, after reading multipart upload into input. |
| `GET /v1/status/poll/{task_id}` | `client.get_run(run_id)` | Query execution status from any pod. Optionally long-poll with `wait=True`. |
| `POST /v1/convert/chunked/{type}/source` | `client.run(chunk_flow_id, input)` | Submit convert+chunk flow, block until complete. |
| `POST /v1/convert/chunked/{type}/file` | `client.run(chunk_flow_id, input)` | Same, after reading multipart upload. |

The `task_id` returned by async endpoints IS the Stepflow `run_id`. No separate task registry needed.

#### What Stepflow Provides (That docling-serve Doesn't)

- **Distributed state.** Any pod can serve `GET /v1/status/poll/{task_id}` because execution state lives in the Stepflow server, not in-process memory.
- **Horizontal scaling.** Multiple step worker pods can process requests concurrently. Stepflow routes component executions to available workers.
- **Result persistence.** Conversion results are stored in the blob store. They survive pod restarts and can be retrieved after the fact.
- **Long-polling.** `client.get_run(run_id, wait=True)` blocks server-side until the execution completes, avoiding busy-poll loops.

#### Flow Definitions

Two flows are registered at startup — one for conversion, one for conversion + chunking:

**Convert flow** (used by `/v1/convert/source`, `/v1/convert/file`, and their async variants):

```yaml
name: docling-convert
steps:
  convert:
    component: /docling/convert
    input:
      source: $input.source
      source_kind: $input.source_kind
      options: $input.options

output:
  document: $step.convert.document
  status: $step.convert.status
  processing_time: $step.convert.processing_time
  timings: $step.convert.timings
```

**Convert + chunk flow** (used by `/v1/convert/chunked/{type}/source` and `/file`):

```yaml
name: docling-convert-and-chunk
steps:
  convert:
    component: /docling/convert
    input:
      source: $input.source
      source_kind: $input.source_kind
      options: $input.options

  chunk:
    component: /docling/chunk
    input:
      document: $step.convert.document
      chunker_type: $input.chunker_type
      chunk_options: $input.chunk_options

output:
  document: $step.convert.document
  chunks: $step.chunk.chunks
  status: $step.convert.status
  processing_time: $step.convert.processing_time
```

Note: no classify step in Phase 1. docling-serve doesn't do classification — it accepts whatever options the caller provides. Classification is a Phase 2 optimization that the flow can introduce transparently behind the same facade.

### Worker Lifecycle

**Startup:**
1. Download model artifacts if not cached: `StandardPdfPipeline.download_models_hf()`
2. Connect `StepflowClient` to the Stepflow server
3. Register `docling-convert` and `docling-convert-and-chunk` flows via `client.store_flow()`
4. Register `/docling/convert` and `/docling/chunk` components with the Stepflow component server
5. Initialize FastAPI app with docling-serve v1 endpoints
6. Start Uvicorn HTTP server

**Per-request (sync, e.g. `POST /v1/convert/source`):**
1. Parse request into `ConvertDocumentsRequest`
2. Build flow input (source, options)
3. Call `client.run(flow_id, input)` — blocks until Stepflow execution completes
4. Stepflow routes `/docling/convert` component execution to a worker pod
5. Worker: get/create `DocumentConverter` from LRU cache, wrap source as `DocumentStream`, call `converter.convert()`
6. Worker: render `DoclingDocument` into requested output formats, return result
7. `client.run()` returns with result — HTTP facade returns `ConvertDocumentResponse`

**Per-request (async, e.g. `POST /v1/convert/source/async`):**
1. Parse request into `ConvertDocumentsRequest`
2. Build flow input (source, options)
3. Call `client.submit(flow_id, input)` — returns immediately with `run_id`
4. Return `{"task_id": run_id}` (HTTP 202)
5. Stepflow executes the flow in the background on any available worker pod
6. Caller polls `GET /v1/status/poll/{task_id}` → `client.get_run(run_id)` from any pod

**Shutdown:**
1. Graceful drain of in-flight component executions
2. Close `StepflowClient` connection
3. Model memory freed with process exit

## Deployment

### Container Image

Single container, no sidecar:

```dockerfile
FROM python:3.12-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    libgl1-mesa-glx libglib2.0-0 && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY pyproject.toml .
RUN pip install --no-cache-dir .

# Pre-download model artifacts
RUN python -c "from docling.pipeline.standard_pdf_pipeline import StandardPdfPipeline; \
    StandardPdfPipeline.download_models_hf(force=True)"

COPY src/ /app/src/
ENTRYPOINT ["docling-step-worker-server"]
```

### Resource Profile

| Resource | Current (proxy + sidecar) | Step Worker |
|----------|--------------------------|-------------|
| Memory | ~2.5GB (500MB proxy + 2GB serve) | ~2GB (single process) |
| CPU | 4 cores (shared) | 4 cores |
| Startup | ~15s (sidecar model loading) | ~10s (direct model loading) |
| Disk | ~500MB models (per pod or PVC) | Same |

### Migration Path

1. Deploy step worker pods alongside existing docling-serve pods
2. Run parity tests: send same requests to both, compare responses
3. Update `DOCLING_SERVE_URL` in the Langflow integration to point to step worker
4. Verify Langflow flows produce identical results
5. Remove docling-serve sidecar from pod spec
6. Scale down old pods

The migration is a URL change. No code changes in the Langflow integration.

## Scope

### In Scope

- HTTP facade implementing docling-serve v1 API (all endpoints listed above)
- Direct `DocumentConverter` integration (no sidecar)
- LRU converter cache matching docling-serve's caching strategy
- Response preparation rendering DoclingDocument to all `to_formats`
- Stepflow-orchestrated sync and async conversion (all requests go through `StepflowClient`)
- Distributed async task state via Stepflow execution tracking (no in-memory task registry)
- Chunking endpoints (hybrid and hierarchical)
- Health check endpoint
- `DocumentStream` for in-memory document passing (no temp files)

### Not In Scope (Deferred)

- **Document classification.** No `/docling/classify` step in Phase 1. Callers provide options explicitly, same as docling-serve.
- **Page-level disaggregation.** `ThreadedStandardPdfPipeline` handles intra-document parallelism.
- **Rust-native components.** All processing uses docling's Python library.
- **Langflow integration changes.** `integrations/docling/` is not modified.
- **WebSocket status endpoint.** `/v1/status/ws/{task_id}` is a stretch goal. Polling via `/v1/status/poll/{task_id}` is sufficient for Phase 1.
- **Presigned URL targets.** docling-serve v1 supports writing output to presigned URLs. Deferred.

## Known docling-serve Issues Addressed

Beyond eliminating the sidecar, routing through Stepflow addresses several documented architectural limitations in docling-serve. This section catalogues what we fix, what we partially mitigate, and what later phases unlock.

### Fixed in Phase 1

**1. In-memory task state prevents horizontal scaling ([#378](https://github.com/docling-project/docling-serve/issues/378), [#317](https://github.com/docling-project/docling-serve/issues/317))**

docling-serve's orchestrator stores async task state in an in-memory dict (`self.tasks: dict[str, Task] = {}`). Issue #317 demonstrates the result: the same `task_id` intermittently returns 200 or 404 depending on which Uvicorn worker handles the request. Issue #378 reports that even the Redis Queue backend inherits the base orchestrator's in-memory task tracking, so the problem persists.

The practical consequence: users are told to set `UVICORN_WORKERS=1` and not scale horizontally ([#317](https://github.com/docling-project/docling-serve/issues/317)).

**How Stepflow fixes it:** All execution state lives in the Stepflow server, not in-process memory. `client.submit()` returns a `run_id`; `client.get_run(run_id)` queries the Stepflow server, which any pod can serve. Results are persisted in the blob store and survive pod restarts. See the [Stepflow Orchestration](#stepflow-orchestration--fixing-docling-serves-async-architecture) section above.

**2. No horizontal scaling path ([#10](https://github.com/docling-project/docling-serve/issues/10), [#257](https://github.com/docling-project/docling-serve/issues/257), [Discussion #1890](https://github.com/docling-project/docling/discussions/1890))**

Issue #10 notes that docling-serve uses vanilla FastAPI with no dynamic batching or autoscaling. Issue #257 reports a user unable to process 500-1000 pages in under 2 minutes despite tuning. Discussion #1890 explains the root cause: each instance runs an in-process orchestrator with a pool of worker threads, and each Uvicorn worker process is an isolated orchestrator with no shared queue or state. Users are advised to use sticky sessions.

**How Stepflow fixes it:** Multiple step worker pods register with the same Stepflow server. Stepflow routes component executions to available workers with shared state. No sticky sessions needed. Document-level fan-out via `/map` distributes batches across all available pods.

**3. Sync timeout handling ([#317](https://github.com/docling-project/docling-serve/issues/317))**

Issue #317 reports that `DOCLING_SERVE_MAX_SYNC_WAIT=600` has no effect beyond ~250 seconds, suggesting a hard-coded limit in the ASGI/Uvicorn stack. Users processing large PDFs on CPU hit this regularly, and are forced to switch to async mode — which brings them back to the in-memory task state problem from #1.

**How Stepflow fixes it:** Sync endpoints use `client.run(flow_id, input, wait_timeout=N)`. Per the `StepflowClient.run()` docstring: "If the workflow takes longer [than wait_timeout], the server returns the current status rather than an error." The timeout is managed by the Stepflow server, not layered through ASGI polling. We control timeouts at two clean levels: the HTTP client timeout and the Stepflow server-side `wait_timeout`, independently configurable.

### Partially Mitigated in Phase 1

**4. Orphaned work on client disconnect ([#401](https://github.com/docling-project/docling-serve/issues/401))**

Issue #401 reports that when a sync client times out or disconnects, docling-serve continues processing the document to completion. Large conversion requests effectively block the service for new requests.

**What Phase 1 changes:** In our architecture, the sync endpoint calls `client.run()`, which submits the work to Stepflow. If the HTTP client disconnects from our FastAPI facade, the Stepflow execution continues — so the compute work (GPU/CPU) is still consumed. However, the HTTP facade itself is not blocked; it's an async call awaiting `client.run()`, not a thread running `DocumentConverter.convert()` directly. The Stepflow execution completes and the result is persisted in the blob store even if no one polls for it.

**What Phase 1 does not fix:** Stepflow does not currently have a run cancellation API. We cannot abort an in-flight `DocumentConverter.convert()` call. The compute resources remain occupied.

**What later phases unlock:** With pipeline disaggregation (page-level fan-out via `/map`), cancellation becomes feasible at step boundaries — stop dispatching new pages while allowing in-flight pages to complete. This would require adding a cancel API to Stepflow, which is a natural extension of the existing `submit()`/`get_run()` contract.

### Addressed by Later Phases (Rust / Pipeline Disaggregation)

**5. GIL-bound intra-document parallelism ([Discussion #1890](https://github.com/docling-project/docling/discussions/1890), [#419](https://github.com/docling-project/docling-serve/issues/419))**

Discussion #1890 explains that docling-serve's workers use Python threads, and the GIL limits parallelism for CPU-bound work. Issue #419 reports that setting VLM pipeline `concurrency=10` achieves only ~2x speedup instead of the expected 10x, with the bottleneck appearing to be that only 4 pages are processed in parallel at any time.

Phase 1 does not change intra-document parallelism — `DocumentConverter.convert()` still uses `ThreadedStandardPdfPipeline` within a single Python process. The Phase 1 improvement is inter-document: multiple pods processing different documents concurrently via Stepflow.

Later phases break the single-document pipeline into per-page Stepflow steps. Layout analysis, table extraction, and OCR become separate component executions that Stepflow fans out via `/map`, distributing across pods and sidestepping the GIL entirely. Rust-native ONNX inference (from `rust-native-docling.md`) moves the most expensive compute out of Python altogether.

**6. All-or-nothing batch failure ([#404](https://github.com/docling-project/docling-serve/issues/404))**

Issue #404 reports that a single error during processing (e.g., a missing relationship key in an OpenXML document) marks the entire task as failed. Users cannot obtain partially successful results.

Phase 1 does not change single-document error behavior — that's internal to `DocumentConverter.convert()`. However, for batch processing via Stepflow's `/map` component, each document is an independent flow execution. The `MapOutput` type tracks per-item results with `successful` and `failed` counts (`results: Vec<FlowResult>, successful: u32, failed: u32` — see `stepflow-builtins/src/map.rs`). 49 documents can succeed while 1 fails, and the caller gets all 49 results plus the error.

With later pipeline disaggregation, the same pattern applies within a single document: per-page processing via `/map` means a failure on page 23 doesn't prevent pages 1-22 from completing.

## Open Questions

1. ~~**Async task management.**~~ Resolved: All processing (sync and async) routes through `StepflowClient`. Async endpoints use `client.submit()` which returns a Stepflow `run_id` as the `task_id`. Status polling uses `client.get_run(run_id)` which queries the Stepflow server — execution state is persistent and distributed, not pinned to a process. This solves docling-serve's known architectural flaw ([#378](https://github.com/docling-project/docling-serve/issues/378), [#317](https://github.com/docling-project/docling-serve/issues/317)) where in-memory task registries break horizontal scaling. No in-memory task store, no Redis dependency, no single-process pinning.

2. **DoclingDocument serialization size.** The full `json_content` can be large for image-heavy PDFs. docling-serve returns it inline. Should we consider a streaming or pagination approach for very large documents?

3. **Options parity testing.** How do we systematically verify that every docling-serve option is correctly mapped to `PdfPipelineOptions`? A comprehensive test matrix against a running docling-serve instance would be ideal.

4. ~~**Temp file management.**~~ Resolved: `DocumentConverter` accepts `DocumentStream` (in-memory `BytesIO` wrapper). No temp files needed.

5. ~~**Pipeline options passthrough.**~~ Resolved: We use docling-serve's approach — LRU cache of `DocumentConverter` instances keyed by options hash. Per-request options create/reuse converter instances. No named configs.
