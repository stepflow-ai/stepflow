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
- Compiles the `.proto` files from `stepflow-rs/proto/` during `cargo build`
- Exports all generated Rust types at the crate root

When `stepflow-proto` is published to crates.io, switch the path dep to a version dep:
```toml
stepflow-proto = "0.12"
```

## Environment variables

`WorkerConfig` reads all fields from environment variables via clap's `env` feature.
No code changes needed to configure workers in different environments.

| Variable | Default | Description |
|----------|---------|-------------|
| `STEPFLOW_TASKS_URL` | `http://127.0.0.1:7837` | Tasks gRPC service URL |
| `STEPFLOW_QUEUE_NAME` | `default` | Worker queue name |
| `STEPFLOW_MAX_CONCURRENT` | `10` | Max concurrent tasks |
| `STEPFLOW_MAX_RETRIES` | `10` | Max reconnect attempts |
| `STEPFLOW_SHUTDOWN_GRACE_SECS` | `30` | Graceful shutdown timeout |
| `STEPFLOW_BLOB_URL` | *(none)* | Blob API URL override |
| `STEPFLOW_ORCHESTRATOR_URL` | *(none)* | OrchestratorService URL override |

## Integration tests

Integration tests live in `crates/stepflow-worker/tests/integration.rs`.  They are
marked `#[ignore]` so `cargo test` skips them honestly (they show as "ignored", not
"passed").  When run explicitly without `STEPFLOW_DEV_BINARY` they **fail** — there
is no silent pass.

```bash
# Build the orchestrator binary first (from the repo root or stepflow-rs/)
cd ../../stepflow-rs
cargo build -p stepflow-cli --no-default-features

# Run integration tests from sdks/rust/
cd ../sdks/rust
STEPFLOW_DEV_BINARY=../../stepflow-rs/target/debug/stepflow \
  cargo test --test integration -- --include-ignored
```

`check-rust-sdk.sh` automatically runs integration tests when `STEPFLOW_DEV_BINARY`
is set in the environment.

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
