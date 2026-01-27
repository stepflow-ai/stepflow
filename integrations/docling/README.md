# Stepflow Docling Integration

Docling integration for Stepflow enabling document processing as routable components. This integration allows Langflow's Docling components to execute on dedicated docling-serve infrastructure via Stepflow's routing system.

## Overview

The Stepflow Docling integration implements the "translator stays simple, routing gets smart" design principle:

- **Langflow component class paths** map directly to Stepflow URL paths
- **Routing configuration** determines where components execute
- **docling-serve** handles the actual document processing (PDF parsing, OCR, chunking)

### Component Names

The integration registers components matching Langflow's Docling class names exactly:

| Component | Description |
|-----------|-------------|
| `DoclingInlineComponent` | Process documents through local docling-serve (sidecar mode) |
| `DoclingRemoteComponent` | Process documents via remote docling-serve API |
| `ChunkDoclingDocument` | Split documents into semantic chunks for RAG |
| `ExportDoclingDocument` | Export documents to Markdown, HTML, or text |

## Installation

```bash
# Install the package
pip install stepflow-docling

# Or with UV
uv add stepflow-docling

# With OpenTelemetry support
pip install stepflow-docling[otel]
```

## Quick Start

### Running the Component Server

```bash
# Start the server (prints port to stdout for Stepflow discovery)
stepflow-docling serve

# With custom docling-serve URL
stepflow-docling serve --docling-url http://docling-serve:5001

# Check docling-serve health
stepflow-docling health --docling-url http://localhost:5001
```

### Converting Documents via CLI

```bash
# Convert a document from URL
stepflow-docling convert https://arxiv.org/pdf/2408.09869.pdf

# With OCR enabled
stepflow-docling convert https://example.com/scanned.pdf --ocr --ocr-engine tesseract

# Output as JSON
stepflow-docling convert https://example.com/doc.pdf --output-format json
```

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `DOCLING_SERVE_URL` | `http://localhost:5001` | URL of the docling-serve instance |
| `DOCLING_SERVE_API_KEY` | - | Optional API key for authentication |

### Stepflow Configuration

Add the Docling integration as a plugin in your `stepflow-config.yml`:

```yaml
plugins:
  docling:
    type: stepflow
    command: uv
    args: ["--project", "integrations/docling", "run", "stepflow-docling", "serve"]

  # Or connect to a remote docling worker
  docling_k8s:
    type: stepflow
    url: "http://docling-load-balancer.stepflow.svc.cluster.local:8080"

routes:
  # Route Docling components to the docling plugin
  "/langflow/core/lfx/components/docling/{*component}":
    - plugin: docling

  # Fallback to langflow-worker for other components
  "/langflow/{*component}":
    - plugin: langflow
```

## Architecture

### Sidecar Deployment Pattern

For production K8s deployments, we recommend the sidecar pattern:

```
docling-worker pod
├── docling-worker (StepflowDoclingServer)
│   └── listens on :8080
│   └── receives Stepflow JSON-RPC requests
│   └── calls docling-serve sidecar
└── docling-serve (sidecar)
    └── listens on :5001
    └── handles actual PDF/OCR processing
```

This pattern provides:
- **Resource isolation**: Heavy document processing in dedicated pods
- **GPU affinity**: docling-serve can access GPU for OCR acceleration
- **Horizontal scaling**: Scale docling workers independently
- **Clean separation**: Each worker has its own docling-serve instance

### How Routing Works

1. Langflow workflow references component: `DoclingInlineComponent`
2. Stepflow translator converts to path: `/langflow/core/lfx/components/docling/DoclingInlineComponent`
3. Route matches: `/langflow/core/lfx/components/docling/{*component}`
4. Request routes to: `docling` plugin
5. Plugin strips prefix and sends: `/DoclingInlineComponent`
6. StepflowDoclingServer executes the component

## Python API

### DoclingServeClient

Low-level HTTP client for docling-serve:

```python
from stepflow_docling import DoclingServeClient
from stepflow_docling.client.docling_client import ConversionOptions, OcrEngine

async def convert_document():
    async with DoclingServeClient("http://localhost:5001") as client:
        # Convert from URL
        result = await client.convert_from_url(
            "https://arxiv.org/pdf/2408.09869.pdf",
            ConversionOptions(
                to_formats=["md"],
                do_ocr=True,
                ocr_engine=OcrEngine.EASYOCR,
            )
        )
        print(result.document["md_content"])
```

### StepflowDoclingServer

Component server for Stepflow runtime:

```python
from stepflow_docling import StepflowDoclingServer

# Create and run the server
server = StepflowDoclingServer(
    docling_serve_url="http://localhost:5001",
    api_key="optional-api-key",
)
server.run()
```

## Development

### Setup

```bash
cd integrations/docling
uv sync --dev
```

### Running Tests

```bash
# Run all tests
uv run pytest

# Run unit tests only
uv run pytest tests/unit/

# With coverage
uv run pytest --cov=stepflow_docling
```

### Code Quality

```bash
# Format code
uv run poe fmt

# Lint and fix
uv run poe lint

# Type checking
uv run poe typecheck

# Run all checks
uv run poe check
```

## Observability

The integration supports OpenTelemetry for distributed tracing:

```bash
# Install with OTEL support
pip install stepflow-docling[otel]

# Configure OTLP endpoint
export OTEL_EXPORTER_OTLP_ENDPOINT="http://tempo:4317"
export STEPFLOW_SERVICE_NAME="stepflow-docling"
```

HTTP client requests to docling-serve are automatically instrumented when OpenTelemetry is available.

## Related Documentation

- [Stepflow Documentation](https://stepflow.org/)
- [Docling Documentation](https://docling-project.github.io/docling/)
- [docling-serve API](https://github.com/docling-project/docling-serve)

## License

Apache License 2.0 - See [LICENSE](../../LICENSE) for details.
