# Proposal: First-Class Docling Integration for Stepflow

**Status:** Draft  
**Authors:** Nate McCall  
**Created:** January 2026

## Summary

This proposal describes adding Docling as a routable Stepflow component, enabling document processing capabilities independent of Langflow while maintaining seamless integration when Langflow flows contain Docling components. The key design principle is to keep the docling translation layer generic and straight forward while routing layer manages the complexity. 

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

Langflow provides four Docling components in the `lfx.components.docling` module:

| Component Class | Purpose | Backend |
|----------------|---------|---------|
| `DoclingInlineComponent` | Process documents | Local model (in-process) |
| `DoclingRemoteComponent` | Process documents | Remote API (docling-serve) |
| `ChunkDoclingDocument` | Semantic chunking | HybridChunker / HierarchicalChunker |
| `ExportDoclingDocument` | Format conversion | Markdown, HTML, JSON, DocTags |

### Current State in Stepflow

The K8s production example deploys docling-serve as an infrastructure service. Langflow workers access it directly via `DOCLING_SERVE_URL`. This works but:

- Docling is invisible to Stepflow's routing and observability
- No independent scaling of document processing
- Translator would need special-case logic to handle Docling differently

## Proposal

### Design Principles

1. **Translator stays simple** — Maps Langflow component class paths match directly to Stepflow URL paths
2. **1:1 name mapping** — Langflow component names correspond exactly to Stepflow path suffixes
3. **Routing config gets smart** — Whether a component runs on langflow-worker or docling-worker is determined by route rules, not translation logic

### Langflow Translation

Though the translator already emits paths based on component class names, pending the work on [544](https://github.com/stepflow-ai/stepflow/issues/544) the translator will converts Langflow core component class paths to Stepflow paths mechanically:

| Langflow Component Class | Stepflow Path |
|-------------------------|---------------|
| `lfx.components.docling.DoclingInlineComponent` | `/langflow/core/lfx/components/docling/DoclingInlineComponent` |
| `lfx.components.docling.DoclingRemoteComponent` | `/langflow/core/lfx/components/docling/DoclingRemoteComponent` |
| `lfx.components.docling.ChunkDoclingDocument` | `/langflow/core/lfx/components/docling/ChunkDoclingDocument` |
| `lfx.components.docling.ExportDoclingDocument` | `/langflow/core/lfx/components/docling/ExportDoclingDocument` |

Translation formula: `/langflow/core/` + `<component_class_path>`


### Routing Configuration

Route rules determine where components execute, with prioritisation on finer grained routes. Without docling workers, all components run on langflow-worker:

```yaml
routes:
  "/langflow/core/{*component}":
    - plugin: langflow_k8s
```

The langflow-worker uses the component path to instantiate and execute the Langflow code. With docling workers, docling components offloaded to specialized workers:

```yaml
plugins:
  langflow_k8s:
    type: stepflow
    transport: http
    url: "http://langflow-load-balancer.stepflow.svc.cluster.local:8080"
    
  docling_k8s:
    type: stepflow
    transport: http
    url: "http://docling-load-balancer.stepflow.svc.cluster.local:8080"

routes:
  # Specific route for docling components (higher priority)
  "/langflow/core/lfx/components/docling/{*component}":
    - plugin: docling_k8s

  # General route for other langflow components
  "/langflow/core/{*component}":
    - plugin: langflow_k8s
    
  "/builtin/{*component}":
    - plugin: builtin
```

The docling-worker receives paths like `/DoclingInlineComponent` or `/DoclingRemoteComponent` (the suffix after route prefix matching).

### Docling Worker Implementation

Create `integrations/docling/` following the established Langflow worker pattern in `integrations/langflow`:

```
integrations/docling/
├── pyproject.toml
├── README.md
├── src/stepflow_docling/
│   ├── __init__.py
│   ├── server.py              # StepflowDoclingServer
│   ├── client.py              # DoclingServeClient
│   └── components/
│       ├── docling_inline.py
│       ├── docling_remote.py
│       ├── chunk_docling.py
│       └── export_docling.py
└── tests/
```

The worker registers components matching Langflow's class names exactly:

```python
class StepflowDoclingServer:
    def __init__(self):
        self.server = StepflowServer()
        self.client = DoclingServeClient(os.environ["DOCLING_SERVE_URL"])
        self._register_components()

    def _register_components(self):
        # Component names match Langflow's class names exactly
        @self.server.component(name="DoclingInlineComponent")
        async def docling_inline(input_data: dict, context: StepflowContext) -> dict:
            return await self.client.convert(input_data)

        @self.server.component(name="DoclingRemoteComponent")
        async def docling_remote(input_data: dict, context: StepflowContext) -> dict:
            return await self.client.convert(input_data)

        @self.server.component(name="ChunkDoclingDocument")
        async def chunk_docling(input_data: dict, context: StepflowContext) -> dict:
            return await self.client.chunk(input_data)

        @self.server.component(name="ExportDoclingDocument")
        async def export_docling(input_data: dict, context: StepflowContext) -> dict:
            return await self.client.export(input_data)
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
          value: "http://otel-collector.stepflow-o11y.svc.cluster.local:4317"
          
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

1. **No translator changes** — Adding docling workers is purely a routing/deployment change
2. **Gradual rollout** — Route specific components to docling workers while others stay on langflow-worker
3. **Consistent paths** — Same flow definition works with or without docling workers
4. **Unified observability** — Document processing requests traced through Stepflow's OTel stack
5. **Independent scaling** — Scale document processing separately from LLM workloads
6. **GPU routing** — Route to GPU-enabled workers for heavy OCR/table extraction

## Further Considerations

### Stepflow as Docling Orchestration Layer

The proposed architecture positions Stepflow to serve as a scalable front for document processing. By routing `/langflow/core/lfx/components/docling/*` through a dedicated load balancer, Stepflow can orchestrate a pool of docling workers as a single logical resource:

```
                    ┌─────────────────────────────────────┐
                    │          Stepflow Server            │
                    │  /langflow/core/.../docling/* →     │
                    │           docling_k8s               │
                    └──────────────────┬──────────────────┘
                                       │
                    ┌──────────────────▼──────────────────┐
                    │       Pingora Load Balancer         │
                    │        (docling worker pool)        │
                    └──────────────────┬──────────────────┘
                                       │
          ┌────────────────────────────┼────────────────────────┐
          ▼                            ▼                        ▼
   ┌─────────────┐            ┌─────────────┐            ┌─────────────┐
   │docling-wkr-0│            │docling-wkr-1│            │docling-wkr-2│
   │  + sidecar  │            │  + sidecar  │            │  + sidecar  │
   │(docling-srv)│            │(docling-srv)│            │(docling-srv)│
   └─────────────┘            └─────────────┘            └─────────────┘
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

Future work could introduce heterogeneous worker pools with fallback:

```yaml
routes:
  "/langflow/core/lfx/components/docling/{*component}":
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
2. Implement component handlers matching Langflow class names (`DoclingInlineComponent`, `DoclingRemoteComponent`, `ChunkDoclingDocument`, `ExportDoclingDocument`)
3. Add K8s manifests for sidecar deployment pattern (docling-worker + docling-serve)
4. Add Docling worker pool to Pingora load balancer configuration
5. Update `stepflow-config.yml` with docling-specific route rules
6. Update observability dashboards for document processing metrics

**Note:** No translator changes are required. The translator already emits paths based on component class names; routing configuration determines whether components execute on langflow-worker or docling-worker.

## References

- [Docling](https://github.com/docling-project/docling)
- [Docling-serve](https://github.com/docling-project/docling-serve)
- [Docling-serve OTel PR #456](https://github.com/docling-project/docling-serve/pull/456)
- [Langflow Docling bundle](https://docs.langflow.org/bundles-docling)
- [Stepflow K8s production example](https://github.com/stepflow-ai/stepflow/tree/main/examples/production/k8s)
