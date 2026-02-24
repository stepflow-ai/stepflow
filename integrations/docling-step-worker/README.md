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
    type: stepflow
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

## Development

See [CLAUDE.md](CLAUDE.md) for development setup, testing, and architecture details.
