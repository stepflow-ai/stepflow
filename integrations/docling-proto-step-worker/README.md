# Docling Proto Step Worker

A Stepflow component server that wraps [docling](https://github.com/docling-project/docling)'s `DocumentConverter` library directly, providing document processing as native Stepflow components over **gRPC pull-based transport** (Protocol Buffers).

This is the gRPC variant of [`docling-step-worker`](../docling-step-worker/). Same processing semantics, same response shapes — only the transport changes.

## Components

| Component | Description |
|---|---|
| `/docling/classify` | Lightweight document probing (page count, text layer detection, table heuristics) |
| `/docling/convert` | Full document conversion via `DocumentConverter.convert()` |
| `/docling/chunk` | Semantic chunking via `HybridChunker` |

## Architecture

The worker runs docling **in-process** and connects to the orchestrator as a gRPC client. No HTTP server, no load balancer needed. The orchestrator manages work distribution via a pull-based task queue.

```
┌──────────────────────────┐
│  Orchestrator (gRPC:7837)│
│  TasksService            │◄──── Worker connects and pulls tasks
│  BlobService             │
└──────────────────────────┘
         ▲
         │ PullTasks / CompleteTask / Heartbeat
         │
┌────────┴─────────────────┐
│  docling-proto-step-worker│
│  Components:              │
│    classify, convert, chunk│
│  Transport: gRPC pull     │
└──────────────────────────┘
```

## Quick Start

```bash
# Install
cd integrations/docling-proto-step-worker
uv sync

# Run the worker (connects to orchestrator at localhost:7837)
uv run docling-proto-step-worker
```

## Stepflow Configuration

```yaml
plugins:
  docling:
    type: grpc
    command: uv
    args: ["--project", "integrations/docling-proto-step-worker", "run", "docling-proto-step-worker"]
    queueName: docling

routes:
  "/docling/{*component}":
    - plugin: docling
```

## Docker

```bash
# Build from repo root
docker build -f integrations/docling-proto-step-worker/docker/Dockerfile \
  -t stepflow-docling-proto-worker:latest .
```

The gRPC worker is a client — no ports need to be exposed.

## Testing

```bash
cd integrations/docling-proto-step-worker
uv sync --group dev

# Unit tests
uv run pytest tests/unit

# End-to-end tests (requires Rust toolchain + docling models)
uv run poe test-e2e
```

## Development

See [CLAUDE.md](CLAUDE.md) for development setup, testing, and architecture details.
