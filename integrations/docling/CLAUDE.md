# Docling Integration Development Guide

This guide covers development-specific information for the Stepflow Docling integration.

See the root `/CLAUDE.md` for general project overview and the `README.md` for user-facing documentation.

## Overview

The Docling integration enables document processing (PDF, Word, images) as routable Stepflow components. It connects to docling-serve for the actual processing and exposes components matching Langflow's Docling class names.

**Key design principle**: "translator stays simple, routing gets smart" - Langflow component class paths map directly to Stepflow URL paths, and routing configuration determines where components execute.

## Architecture

### Directory Structure

```
integrations/docling/
├── src/stepflow_docling/
│   ├── __init__.py           # Package exports
│   ├── exceptions.py         # Custom exceptions
│   ├── standalone_server.py  # Simple server entry point
│   ├── cli/
│   │   ├── __init__.py
│   │   └── main.py           # CLI commands (serve, health, convert)
│   ├── client/
│   │   ├── __init__.py
│   │   └── docling_client.py # HTTP client for docling-serve v1 API
│   └── server/
│       ├── __init__.py
│       └── docling_server.py # StepflowServer with Docling components
├── tests/
│   ├── __init__.py
│   └── unit/                 # Unit tests (no external dependencies)
│       ├── test_docling_client.py
│       └── test_docling_server.py
├── pyproject.toml            # Project configuration
├── README.md                 # User documentation
└── CLAUDE.md                 # This file
```

### Key Components

**DoclingServeClient** (`client/docling_client.py`):
- HTTP client for docling-serve v1 API
- Supports sync and async conversion
- Handles base64 encoding, polling, chunking
- OpenTelemetry instrumentation when available

**StepflowDoclingServer** (`server/docling_server.py`):
- Registers four components matching Langflow class names:
  - `DoclingInlineComponent` - Local/sidecar document processing
  - `DoclingRemoteComponent` - Remote API processing
  - `ChunkDoclingDocument` - Document chunking for RAG
  - `ExportDoclingDocument` - Format export
- Uses `@server.component(name="...")` decorator
- Calls docling-serve for actual processing

### Design Decisions

1. **Component name matching**: Components are registered with exact Langflow class names (`DoclingInlineComponent`, not `docling_inline`). This enables direct routing without translation.

2. **Sidecar pattern**: In K8s, each docling-worker pod includes a docling-serve sidecar. The worker calls `localhost:5001` for fast local processing.

3. **Input flexibility**: Accepts various input formats (URLs, base64, bytes, file paths) and normalizes them to DocumentSource objects.

4. **Output compatibility**: Formats output to match Langflow's expected structure (files list with content, status, etc.).

## Computing Component Code Hashes

Langflow uses `code_hash` values in workflow metadata to identify components. These hashes are used by the Langflow integration's `known_components.py` to route lfx components to the core executor instead of compiling custom code.

**Hash computation method** (from `lfx/custom/utils.py`):

```python
import hashlib

def get_component_hash(component_class):
    """Compute the code_hash for a Langflow/lfx component."""
    instance = component_class()
    if hasattr(instance, '_code') and instance._code:
        return hashlib.sha256(instance._code.encode('utf-8')).hexdigest()[:12]
    return None
```

**Example: Computing hashes for Docling components**:

```python
from lfx.components.docling.docling_inline import DoclingInlineComponent
from lfx.components.docling.chunk_docling_document import ChunkDoclingDocumentComponent
from lfx.components.docling.export_docling_document import ExportDoclingDocumentComponent

# Compute hashes
for cls in [DoclingInlineComponent, ChunkDoclingDocumentComponent, ExportDoclingDocumentComponent]:
    instance = cls()
    hash_val = hashlib.sha256(instance._code.encode('utf-8')).hexdigest()[:12]
    print(f"{cls.__name__}: {hash_val}")
```

**Current known hashes** (for `integrations/langflow/.../converter/known_components.py`):

| Component | code_hash | Module |
|-----------|-----------|--------|
| DoclingInlineComponent | `d76b3853ceb4` | `lfx.components.docling.docling_inline.DoclingInlineComponent` |
| ChunkDoclingDocumentComponent | `397fa38f89d7` | `lfx.components.docling.chunk_docling_document.ChunkDoclingDocumentComponent` |
| ExportDoclingDocumentComponent | `4de16ddd37ac` | `lfx.components.docling.export_docling_document.ExportDoclingDocumentComponent` |

**Note**: Hashes are version-specific. When lfx package versions change, the `_code` attribute may change, requiring hash updates.

## Development Commands

### Setup

```bash
cd integrations/docling
uv sync --dev
```

### Testing

```bash
# Run all tests
uv run pytest

# Run unit tests only (no docling-serve required)
uv run pytest tests/unit/

# Run with coverage
uv run pytest --cov=stepflow_docling --cov-report=html

# Run specific test file
uv run pytest tests/unit/test_docling_client.py -v
```

### Code Quality

```bash
# Format code
uv run poe fmt

# Lint and auto-fix
uv run poe lint

# Type checking
uv run poe typecheck

# Check dependencies
uv run poe dep-check

# Run all checks
uv run poe check
```

### Running the Server

```bash
# Start server (STDIO mode for stepflow orchestrator)
uv run stepflow-docling serve

# With custom docling-serve URL
uv run stepflow-docling serve --docling-url http://localhost:5001

# Check health
uv run stepflow-docling health

# Convert a document
uv run stepflow-docling convert https://arxiv.org/pdf/2408.09869.pdf
```

## Testing Strategy

### Unit Tests (`tests/unit/`)

Test components in isolation without external dependencies:

- `test_docling_client.py`: Client HTTP operations (uses `respx` for mocking)
- `test_docling_server.py`: Server input/output processing

**No docling-serve required** - uses mocks and fixtures.

### Integration Tests

For integration tests requiring a running docling-serve instance:

```bash
# Start docling-serve locally
docker run -p 5001:5001 quay.io/docling-project/docling-serve:v1.10.0

# Run integration tests
uv run pytest tests/integration/ -m integration
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `DOCLING_SERVE_URL` | `http://localhost:5001` | docling-serve API URL |
| `DOCLING_SERVE_API_KEY` | - | Optional API key |
| `STEPFLOW_LOG_LEVEL` | `INFO` | Log level |
| `STEPFLOW_LOG_DESTINATION` | `stderr` | Log destination |
| `STEPFLOW_OTLP_ENDPOINT` | - | OTLP endpoint for tracing |

## docling-serve API

The client wraps the docling-serve v1 API:

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check |
| `/v1/convert/source` | POST | Sync document conversion |
| `/v1/convert/source/async` | POST | Start async conversion |
| `/v1/status/poll/{task_id}` | GET | Poll async task status |
| `/v1/convert/chunked/{chunker}/source` | POST | Convert and chunk |
| `/v1/convert/file` | POST | Multipart file upload |

## Code Conventions

### Imports

```python
# Standard library
from __future__ import annotations
import asyncio
from typing import Any

# Third-party
import httpx
from pydantic import BaseModel

# Local
from stepflow_docling.exceptions import DoclingClientError
from stepflow_docling.client.docling_client import DoclingServeClient
```

### Error Handling

Use custom exceptions from `exceptions.py`:

```python
from stepflow_docling.exceptions import (
    DoclingIntegrationError,  # Base exception
    DoclingClientError,       # HTTP/network errors
    DoclingConversionError,   # Document conversion failures
    DoclingTimeoutError,      # Operation timeouts
    DoclingValidationError,   # Input validation errors
)
```

### Async Context Managers

The client supports async context managers:

```python
async with DoclingServeClient(base_url) as client:
    result = await client.convert_from_url(url)
# Client is automatically closed
```

## Dependencies

### Core Dependencies

- `stepflow-py`: Stepflow Python SDK for component servers
- `httpx`: Async HTTP client for docling-serve API
- `pydantic`: Data validation and serialization
- `click`: CLI framework
- `python-dotenv`: Environment variable loading
- `nest-asyncio`: Nested event loop support

### Optional Dependencies

- `opentelemetry-instrumentation-httpx`: HTTP request tracing

### Development Dependencies

- `pytest`, `pytest-asyncio`: Testing
- `respx`: HTTP mocking for httpx
- `ruff`: Linting and formatting
- `mypy`: Type checking

## Related Documentation

- Root `/CLAUDE.md`: General Stepflow development guide
- `/sdks/python/CLAUDE.md`: Python SDK development guide
- `README.md`: User-facing documentation
- `/examples/production/k8s/stepflow/`: K8s deployment examples
