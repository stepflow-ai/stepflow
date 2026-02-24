# CLAUDE.md — Docling Step Worker

## About

This package wraps docling's `DocumentConverter` library directly as Stepflow components, eliminating the need for the docling-serve HTTP sidecar. Three components are registered: `/classify`, `/convert`, `/chunk`. The `/docling/` prefix is added by the routing config, not the worker itself.

## Development

### Setup

```bash
cd integrations/docling-step-worker
uv sync --group dev
```

### Running

```bash
# Direct entry point
uv run docling-step-worker-server

# Via CLI
uv run docling-step-worker serve --host localhost --port 0
```

### Testing

```bash
# Unit tests only
uv run pytest tests/unit

# All tests including integration (requires docling models)
DOCLING_INTEGRATION_TESTS=1 uv run pytest tests

# With coverage
uv run pytest tests --cov=docling_step_worker --cov-report=term-missing

# End-to-end tests (requires docling models; builds stepflow via cargo)
uv run poe test-e2e
```

### End-to-End Tests

The `tests/e2e/` directory contains flow-level tests that run the full classify → convert → chunk pipeline against the real stepflow orchestrator using `stepflow test`.

**Prerequisites:**
- `uv sync --group dev` (installs all deps including HTTP server extras)
- Rust toolchain (cargo) — the poe task builds and runs stepflow from `../../stepflow-rs`
- Docling models downloaded (first run triggers download)

**How it works:** Each `.yaml` file in `tests/e2e/` is a self-contained flow with a `test:` section that defines plugin config (starts `docling-step-worker-server` as a subprocess), routes, and test cases. The test case embeds a small base64-encoded PDF so no external files or blob uploads are needed.

**Note:** E2e tests are intentionally excluded from `poe check` due to startup time (~2 min for model loading). Run them explicitly with `uv run poe test-e2e`.

**Note:** The `docling-step-worker-server` entry point runs an HTTP server and requires the `stepflow-py[http]` extra (fastapi, uvicorn, sse-starlette). This is included automatically via the package dependency — no separate install step needed.

### Linting & Formatting

```bash
uv run poe fmt-check    # Check formatting
uv run poe fmt-fix      # Fix formatting
uv run poe lint-check   # Check linting
uv run poe lint-fix     # Fix linting
uv run poe type-check   # Run mypy
uv run poe check        # All checks + tests
```

## Architecture

- **No sidecar** — docling library runs in-process
- **Pipeline configs** are constructor-level, not per-request. One `DocumentConverter` per named config, lazily initialized.
- **DocumentStream** — documents passed as in-memory `BytesIO` wrappers, no temp files
- **asyncio.to_thread()** — sync docling calls wrapped to avoid blocking the event loop
- **Blob store** — binary documents flow through Stepflow's blob store via `StepflowContext`

## Key Files

- `src/docling_step_worker/server.py` — Main server, component registration
- `src/docling_step_worker/classify.py` — Document probing via pypdfium2
- `src/docling_step_worker/convert.py` — DocumentConverter wrapper
- `src/docling_step_worker/chunk.py` — HybridChunker wrapper
- `src/docling_step_worker/config.py` — Named pipeline configurations
- `src/docling_step_worker/blob_utils.py` — Blob store helpers
- `flows/` — Stepflow flow definitions
