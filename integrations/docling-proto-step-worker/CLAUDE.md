# CLAUDE.md — Docling Proto Step Worker

## About

This package wraps docling's `DocumentConverter` library directly as Stepflow components using **gRPC pull-based transport** (Protocol Buffers). Three components are registered: `classify`, `convert`, `chunk` (no leading slash — the gRPC worker strips "/" from orchestrator paths at lookup time). The `/docling/` prefix is added by the routing config, not the worker itself.

This is the gRPC variant of `integrations/docling-step-worker/` (which uses HTTP JSON-RPC push transport). The domain logic (classify, convert, chunk, blob_utils, etc.) is identical — only the transport and server wiring differ.

## Architecture Differences from docling-step-worker

| Aspect | docling-step-worker (HTTP) | docling-proto-step-worker (gRPC) |
|--------|---------------------------|----------------------------------|
| Transport | HTTP JSON-RPC (push) | gRPC pull (Protocol Buffers) |
| Server pattern | `DoclingStepWorkerServer` class | Module-level `server` + `@server.component` decorators |
| Component names | `/classify`, `/convert`, `/chunk` | `classify`, `convert`, `chunk` (no leading slash) |
| Plugin type | `type: stepflow` | `type: grpc` |
| Worker role | HTTP server (accepts connections) | gRPC client (connects to orchestrator) |
| Load balancer | Required (HTTP proxy) | Not needed (orchestrator manages queue) |
| Blob store | Contextvars via `server._blob_api_url` | Same — HTTP REST blob API via `STEPFLOW_BLOB_API_URL` |
| nest-asyncio | Required | Not needed |

## Development

### Setup

```bash
cd integrations/docling-proto-step-worker
uv sync --group dev
```

### Running

```bash
# Default: gRPC mode
uv run docling-proto-step-worker

# With explicit transport
uv run docling-proto-step-worker --grpc

# HTTP mode (for testing)
uv run docling-proto-step-worker --http --port 8080
```

### Testing

```bash
# Unit tests only
uv run pytest tests/unit

# With coverage
uv run pytest tests --cov=docling_proto_step_worker --cov-report=term-missing

# End-to-end tests (requires docling models; builds stepflow via cargo)
uv run poe test-e2e
```

### Linting & Formatting

```bash
uv run poe fmt-check    # Check formatting
uv run poe fmt-fix      # Fix formatting
uv run poe lint-check   # Check linting
uv run poe lint-fix     # Fix linting
uv run poe type-check   # Run mypy
uv run poe check        # All checks + tests
```

## Key Files

- `src/docling_proto_step_worker/server.py` — gRPC component registration, entry point
- `src/docling_proto_step_worker/classify.py` — Document probing via pypdfium2
- `src/docling_proto_step_worker/convert.py` — DocumentConverter wrapper
- `src/docling_proto_step_worker/converter_cache.py` — LRU converter cache
- `src/docling_proto_step_worker/response_builder.py` — docling-serve response shapes
- `src/docling_proto_step_worker/chunk.py` — HybridChunker wrapper
- `src/docling_proto_step_worker/config.py` — Named pipeline configurations
- `src/docling_proto_step_worker/blob_utils.py` — Blob store helpers
- `flows/` — Stepflow flow definitions
