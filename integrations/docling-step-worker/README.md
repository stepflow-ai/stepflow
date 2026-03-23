# Docling Step Worker

A Stepflow component server that wraps [docling](https://github.com/docling-project/docling)'s `DocumentConverter` library directly, providing document processing as native Stepflow components.

## Components

| Component | Description |
|---|---|
| `/docling/classify` | Lightweight document probing (page count, text layer detection, table heuristics) |
| `/docling/convert` | Full document conversion via `DocumentConverter.convert()` |
| `/docling/chunk` | Semantic chunking via `HybridChunker` |

## Architecture

Unlike the proxy-based docling integration, this worker runs docling **in-process** — no HTTP sidecar needed. Documents are passed as in-memory streams via docling's `DocumentStream`, and binary data flows through Stepflow's blob store.

Pipeline configurations are fixed at converter construction time (docling's `DocumentConverter` does not accept per-request options). The classify step recommends a config name, and the server routes to the appropriate pre-built converter.

## Quick Start

```bash
# Install
cd integrations/docling-step-worker
uv sync

# Run the server
uv run docling-step-worker-server
```

## Stepflow Configuration

```yaml
plugins:
  docling:
    type: grpc
    queueName: docling
    command: uv
    args: ["--project", "integrations/docling-step-worker", "run", "docling-step-worker-server"]

routes:
  "/docling/{*component}":
    - plugin: docling
```

## Flow Usage

```yaml
name: process-my-document
steps:
  - id: classify
    component: /docling/classify
    input:
      source: { $input: "source" }
      source_kind: { $input: "source_kind" }

  - id: convert
    component: /docling/convert
    input:
      source: { $input: "source" }
      source_kind: { $input: "source_kind" }
      pipeline_config: { $step: "classify", path: "recommended_config" }

  - id: chunk
    component: /docling/chunk
    input:
      document: { $step: "convert", path: "document" }

output:
  document: { $step: "convert", path: "document" }
  chunks: { $step: "chunk", path: "chunks" }
```

## Docker

```bash
docker build -f docker/Dockerfile -t docling-step-worker .
docker run -p 8080:8080 docling-step-worker
```

## Testing

```bash
cd integrations/docling-step-worker
uv sync --group dev

# Unit tests
uv run pytest tests/unit

# Integration tests (requires docling models)
DOCLING_INTEGRATION_TESTS=1 uv run pytest tests

# End-to-end tests (requires Rust toolchain + docling models)
uv run poe test-e2e
```

The e2e tests use `stepflow test` to run the full classify → convert → chunk pipeline against the real orchestrator. The poe task builds stepflow from `../../stepflow-rs` via `cargo run`, so a Rust toolchain is the only external prerequisite. See `tests/e2e/` for the test definitions.

## Development

See [CLAUDE.md](CLAUDE.md) for development setup, testing, and architecture details.
