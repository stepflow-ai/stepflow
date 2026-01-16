# Proposal: First-Class Docling Integration for Stepflow

**Status:** Draft  
**Authors:** Nate McCall  
**Created:** January 2026

## Summary

This proposal describes adding Docling as a first-class Stepflow component namespace (`/docling/*`), enabling document processing capabilities independent of Langflow while maintaining seamless integration when Langflow flows contain Docling components.

## Background

### Docling Overview

[Docling](https://github.com/docling-project/docling) is IBM's document processing toolkit providing:

- Multi-format ingestion (PDF, DOCX, PPTX, HTML, images)
- AI-powered layout analysis (DocLayNet) and table extraction (TableFormer)
- OCR support via multiple engines (EasyOCR, Tesseract, RapidOCR)
- Semantic chunking for RAG pipelines
- Export to Markdown, JSON, HTML, DocTags

[Docling-serve](https://github.com/docling-project/docling-serve) exposes these capabilities as an HTTP API with OpenTelemetry support (merged January 2026).

### Langflow's Docling Components

Langflow provides four Docling components:

| Component | Purpose | Backend |
|-----------|---------|---------|
| Docling | Process documents | Local model (in-process) |
| Docling Serve | Process documents | Remote API |
| Chunk DoclingDocument | Semantic chunking | HybridChunker / HierarchicalChunker |
| Export DoclingDocument | Format conversion | Markdown, HTML, JSON, DocTags |

### Current State in Stepflow

The K8s production example deploys docling-serve as an infrastructure service. Langflow workers access it directly via `DOCLING_SERVE_URL`. This works but:

- Docling is invisible to Stepflow's routing and observability
- No independent scaling of document processing
- Langflow-only; other Stepflow flows cannot use Docling directly

## Proposal

### Component Namespace

Introduce `/docling/*` as a first-class Stepflow component namespace:

```
/docling/convert   - Document processing (PDF → structured data)
/docling/chunk     - Semantic chunking for RAG
/docling/export    - Format conversion (DoclingDocument → Markdown/HTML/JSON)
```

### Component Specifications

**`/docling/convert`**

```yaml
input:
  http_sources:                    # URL-based sources
    - url: "https://example.com/doc.pdf"
      headers: {}                  # Optional auth headers
  file_sources:                    # Base64-encoded files
    - base64_string: "..."
      filename: "doc.pdf"
  options:
    to_formats: ["md", "json"]     # Output formats
    do_ocr: true
    force_ocr: false
    ocr_engine: "easyocr"          # easyocr | tesseract | rapidocr
    table_mode: "fast"             # fast | accurate
    
output:
  documents:
    - format: "md"
      content: "# Title\n\n..."
    - format: "json"
      content: { ... }
  metadata:
    pages: 12
    processing_time_ms: 2340
```

**`/docling/chunk`**

```yaml
input:
  document: $step.convert.documents[0]
  chunker: "hybrid"                # hybrid | hierarchical
  max_tokens: 512
  tokenizer_provider: "huggingface"
  model_name: "sentence-transformers/all-MiniLM-L6-v2"

output:
  chunks:
    - text: "..."
      metadata:
        page: 1
        section: "introduction"
        headings: ["Chapter 1", "Overview"]
```

**`/docling/export`**

```yaml
input:
  document: $step.convert.documents[0]
  format: "markdown"               # markdown | html | text | doctags
  image_mode: "placeholder"        # placeholder | embedded | referenced

output:
  content: "# Document Title\n\n..."
```

### Worker Implementation

Create `integrations/docling/` following the established Langflow worker pattern:

```
integrations/docling/
├── pyproject.toml
├── README.md
├── src/stepflow_docling/
│   ├── __init__.py
│   ├── server.py              # StepflowDoclingServer
│   ├── client.py              # DoclingServeClient
│   └── components/
│       ├── convert.py
│       ├── chunk.py
│       └── export.py
└── tests/
```

The worker uses `stepflow_worker.StepflowServer` and delegates to docling-serve via HTTP:

```python
class StepflowDoclingServer:
    def __init__(self):
        self.server = StepflowServer()
        self.client = DoclingServeClient(os.environ["DOCLING_SERVE_URL"])
        self._register_components()

    def _register_components(self):
        @self.server.component(name="convert")
        async def convert(input_data: dict, context: StepflowContext) -> dict:
            return await self.client.convert(input_data)

        @self.server.component(name="chunk")
        async def chunk(input_data: dict, context: StepflowContext) -> dict:
            return await self.client.chunk(input_data)
```

### Routing Configuration

```yaml
# stepflow-config.yml
plugins:
  docling_k8s:
    type: stepflow
    transport: http
    url: "http://stepflow-load-balancer.stepflow.svc.cluster.local:8080"

routes:
  "/docling/{*operation}":
    - plugin: docling_k8s
```

### Langflow Translator Integration

When the translator encounters Langflow Docling components, emit corresponding Stepflow components:

| Langflow Component | Stepflow Component |
|-------------------|-------------------|
| Docling / Docling Serve | `/docling/convert` |
| Chunk DoclingDocument | `/docling/chunk` |
| Export DoclingDocument | `/docling/export` |

Example translation:

```yaml
# Langflow "Docling Serve" → Stepflow
- id: process_document
  component: /docling/convert
  input:
    http_sources:
      - url: $input.document_url
    options:
      to_formats: ["md"]
      do_ocr: true

# Langflow "Chunk DoclingDocument" → Stepflow
- id: chunk_document
  component: /docling/chunk
  input:
    document: $step.process_document.documents[0]
    chunker: "hybrid"
    max_tokens: 512
```

### K8s Deployment

Deploy as sidecar pattern (docling-worker + docling-serve in same pod):

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: docling-worker
  namespace: stepflow
spec:
  replicas: 3
  template:
    spec:
      containers:
      - name: worker
        image: stepflow-docling-worker:latest
        env:
        - name: DOCLING_SERVE_URL
          value: "http://localhost:5001"
        - name: OTEL_EXPORTER_OTLP_ENDPOINT
          value: "http://otel-collector.stepflow-o12y.svc.cluster.local:4317"
          
      - name: docling-serve
        image: quay.io/docling-project/docling-serve-cpu:latest
        env:
        - name: OMP_NUM_THREADS
          value: "4"
        - name: OTEL_SERVICE_NAME
          value: "docling-serve"
```

Sidecar benefits:

- GPU affinity (both containers scheduled to same node)
- No network hop between worker and docling-serve
- Models loaded once per pod
- Scale as a unit

## Benefits

1. **First-class component** — Use `/docling/convert` directly in any Stepflow flow, not just Langflow
2. **Unified observability** — Document processing requests traced through Stepflow's OTel stack
3. **Independent scaling** — Scale document processing separately from LLM workloads
4. **GPU routing** — Route to GPU-enabled workers for heavy OCR/table extraction
5. **Langflow compatibility** — Translated flows work transparently

## Further Considerations

### Stepflow as Docling Orchestration Layer

The proposed architecture positions Stepflow to serve as a scalable front for document processing. By routing `/docling/*` through the Pingora load balancer, Stepflow can orchestrate a pool of docling-serve instances as a single logical resource:

```
                    ┌─────────────────────────────┐
                    │      Stepflow Server        │
                    │   /docling/* → docling_k8s  │
                    └──────────────┬──────────────┘
                                   │
                    ┌──────────────▼──────────────┐
                    │    Pingora Load Balancer    │
                    │     (docling worker pool)   │
                    └──────────────┬──────────────┘
                                   │
          ┌────────────────────────┼────────────────────────┐
          ▼                        ▼                        ▼
   ┌─────────────┐          ┌─────────────┐          ┌─────────────┐
   │docling-wkr-0│          │docling-wkr-1│          │docling-wkr-2│
   │ + sidecar   │          │ + sidecar   │          │ + sidecar   │
   │(docling-srv)│          │(docling-srv)│          │(docling-srv)│
   └─────────────┘          └─────────────┘          └─────────────┘
```

This provides:

- **Unified API** — External callers see one endpoint; Stepflow handles distribution
- **Horizontal scaling** — Add workers to increase throughput
- **Load balancing** — Pingora distributes across healthy workers
- **Instance affinity** — Async jobs route back to originating worker
- **Backpressure handling** — Circuit breaking when workers are saturated
- **Operational metrics** — Request latency, queue depth, error rates per worker

This pattern effectively makes Stepflow a "document processing gateway" — callers submit documents to a single endpoint without awareness of the underlying worker pool, model loading, or GPU allocation.

### GPU Worker Tiers

Future work could introduce heterogeneous worker pools:

```yaml
routes:
  "/docling/convert":
    - plugin: docling_gpu    # Prefer GPU workers
    - plugin: docling_cpu    # Fallback to CPU
```

With container variants:
- `docling-serve-cpu` — General workloads
- `docling-serve-cu126` — NVIDIA GPU acceleration for heavy OCR

### Model Serving Strategy

Production deployments should consider:

- **Pre-baked vs. mounted models** — Container images with models baked in vs. PVC-mounted model cache
- **Model versioning** — Pin container tags to specific model versions
- **Warm-up** — Health checks that verify models are loaded before accepting traffic

## Implementation Plan

1. Create `integrations/docling/` with StepflowDoclingServer
2. Implement `/docling/convert`, `/docling/chunk`, `/docling/export` components
3. Add K8s manifests for sidecar deployment pattern
4. Update Langflow translator to emit `/docling/*` components
5. Add Docling worker pool to Pingora load balancer configuration
6. Update observability dashboards for document processing metrics

## References

- [Docling](https://github.com/docling-project/docling)
- [Docling-serve](https://github.com/docling-project/docling-serve)
- [Docling-serve OTel PR #456](https://github.com/docling-project/docling-serve/pull/456)
- [Langflow Docling bundle](https://docs.langflow.org/bundles-docling)
- [Stepflow K8s production example](https://github.com/stepflow-ai/stepflow/tree/main/examples/production/k8s)
