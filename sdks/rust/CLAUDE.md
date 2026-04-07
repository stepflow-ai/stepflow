# Rust SDK Development Guide

This guide covers development of the `sdks/rust/` Rust SDK workspace.

See the root `/CLAUDE.md` for project overview and the `stepflow-rs/CLAUDE.md` for
Rust conventions (error handling, module structure, logging, etc.) that also apply here.

## Crates

| Crate | Purpose |
|-------|---------|
| `stepflow-client` | Flow authoring (`FlowBuilder`, `ValueExpr`) + orchestrator client (`StepflowClient`) |
| `stepflow-worker` | Worker implementation (`Worker`, `ComponentRegistry`, `ComponentContext`) |

`stepflow-worker` does **not** depend on `stepflow-client`. Users who want to build flows
inside a worker component can add `stepflow-client` as a separate dependency and call
`flow.to_json()?` to get a `serde_json::Value` for `ctx.evaluate_flow(...)`.

## Building

```bash
# From sdks/rust/
cargo build           # builds both crates
cargo test            # runs all unit tests
cargo clippy          # lint
cargo fmt --check     # formatting check
```

**Prerequisites**: `protoc` must be installed (same requirement as building `stepflow-rs`),
because `stepflow-proto` is a path dependency that compiles `.proto` files during `cargo build`.

## Running the hello_world example

```bash
# Start the Stepflow orchestrator first
cd ../../stepflow-rs
cargo run -- serve --port 7840

# In a second terminal:
cd sdks/rust
STEPFLOW_TASKS_URL=http://127.0.0.1:7837 cargo run --example hello_world
```

## Module conventions

- Use `foo.rs` files, not `foo/mod.rs` — enforced by `clippy::mod_module_files`
- Follow error handling patterns from `stepflow-rs/CLAUDE.md` (thiserror enums)
- All public items must have `///` doc comments

## Proto dependency

`stepflow-proto` is a path dependency pointing to
`../../stepflow-rs/crates/stepflow-proto`. This crate:
- Compiles the `.proto` files from `stepflow-rs/crates/stepflow-proto/proto/` during `cargo build`
- Exports all generated Rust types at the crate root
- Published to crates.io (proto files are included in the package)

Path + version deps allow local development while also enabling `cargo publish`:
```toml
stepflow-proto = { version = "0.12", path = "../../../../stepflow-rs/crates/stepflow-proto" }
```

## Features

| Feature | Description |
|---------|-------------|
| `nats` | Enables NATS JetStream transport (`async-nats` dependency) |

```bash
# Build with NATS support
cargo build --features nats
```

## Environment variables

`WorkerConfig` reads all fields from environment variables via clap's `env` feature.
No code changes needed to configure workers in different environments.

### Common

| Variable | Default | Description |
|----------|---------|-------------|
| `STEPFLOW_TRANSPORT` | `grpc` | Transport type: `grpc` or `nats` |
| `STEPFLOW_MAX_CONCURRENT` | `10` | Max concurrent tasks |
| `STEPFLOW_MAX_RETRIES` | `10` | Max reconnect attempts |
| `STEPFLOW_SHUTDOWN_GRACE_SECS` | `30` | Graceful shutdown timeout |
| `STEPFLOW_BLOB_URL` | *(none)* | Blob API URL override |
| `STEPFLOW_ORCHESTRATOR_URL` | *(none)* | OrchestratorService URL override |
| `STEPFLOW_BLOB_THRESHOLD_BYTES` | `0` | Auto-blobification threshold in bytes (0 = disabled) |

### gRPC transport

| Variable | Default | Description |
|----------|---------|-------------|
| `STEPFLOW_TASKS_URL` | `http://127.0.0.1:7837` | Tasks gRPC service URL |
| `STEPFLOW_QUEUE_NAME` | `default` | Worker queue name |

### NATS transport (requires `nats` feature)

| Variable | Default | Description |
|----------|---------|-------------|
| `STEPFLOW_NATS_URL` | `nats://localhost:4222` | NATS server URL |
| `STEPFLOW_NATS_STREAM` | `STEPFLOW_TASKS` | JetStream stream name |
| `STEPFLOW_NATS_CONSUMER` | `stepflow-default` | Durable consumer name |

These match the Python SDK's NATS environment variables for cross-SDK parity.

## Integration tests

Integration tests live in `crates/stepflow-worker/tests/integration.rs`.  They are
marked `#[ignore]` so `cargo test` skips them honestly (they show as "ignored", not
"passed").  When run explicitly without `STEPFLOW_DEV_BINARY` they **fail** — there
is no silent pass.

```bash
# Build the orchestrator binary first (from the repo root or stepflow-rs/)
cd ../../stepflow-rs
cargo build -p stepflow-server --no-default-features

# Run integration tests from sdks/rust/
cd ../sdks/rust
STEPFLOW_DEV_BINARY=../../stepflow-rs/target/debug/stepflow-server \
  cargo test --test integration -- --include-ignored
```

`check-rust-sdk.sh` automatically runs integration tests when `STEPFLOW_DEV_BINARY`
is set in the environment.

### NATS integration tests

NATS tests live in `crates/stepflow-worker/tests/nats_integration.rs` and require
the `nats` feature, `STEPFLOW_DEV_BINARY` (built with `--features nats`), and Docker.

```bash
# Build the orchestrator binary with NATS support
cd ../../stepflow-rs
cargo build -p stepflow-server --features nats

# Run NATS integration tests from sdks/rust/
cd ../sdks/rust
STEPFLOW_DEV_BINARY=../../stepflow-rs/target/debug/stepflow-server \
  cargo test --test nats_integration --features nats -- --include-ignored
```

These tests spin up a NATS server via `testcontainers`, configure the orchestrator
with a NATS plugin, and run the Rust worker using NATS transport end-to-end.

### LocalOrchestrator API (stepflow-client)

The `local-server` feature of `stepflow-client` exposes
`stepflow_client::local_server::LocalOrchestrator` for use in your own integration
tests.  Enable it with:

```toml
[dev-dependencies]
stepflow-client = { path = "...", features = ["local-server"] }
```

## Embedding WorkerConfig in your own CLI

```rust
#[derive(clap::Parser)]
struct Cli {
    #[command(flatten)]
    worker: WorkerConfig,
    // ... your own args
}
```
