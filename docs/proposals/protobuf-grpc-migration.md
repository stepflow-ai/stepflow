# Proposal: Protocol Buffers + gRPC Migration

**Status:** In Progress (Phase 3 complete, Phase 5a–5b complete)
**Branch:** `protobuf-exploration`
**Created:** March 2026

## Summary

Migrate Stepflow's orchestrator-worker protocol from JSON-RPC over HTTP to Protocol Buffers + gRPC. This replaces ad-hoc JSON-RPC with a well-typed, code-generated protocol and introduces a pull-based task dispatch model that eliminates the initialization handshake and enables backpressure.

## Motivation

The current architecture has four pain points:

1. **Stateful initialization (#716):** Workers must be initialized by the orchestrator with blob API URLs and thresholds before serving requests. Behind a load balancer, this creates race conditions where different orchestrator instances may hit uninitialized workers.

2. **No backpressure:** The orchestrator POSTs directly to workers with no queueing, potentially overloading busy workers.

3. **Code generation pain:** JSON Schema to Python types via `datamodel-code-generator` produces fragile, inconsistent output. This will worsen with additional SDK languages.

4. **SSE complexity:** The bidirectional SSE mechanism (worker sends requests as SSE events, orchestrator responds via POST with `instance_id` header) is complex and fragile.

## Architecture

### Proto files as the source of truth

All service definitions live in `proto/stepflow/v1/*.proto`:

| Proto file | Service | Transport | Purpose |
|---|---|---|---|
| `common.proto` | — | — | Shared types: enums, error types, item results |
| `health.proto` | `HealthService` | REST + gRPC | Service health check |
| `flows.proto` | `FlowsService` | REST + gRPC | Flow CRUD |
| `runs.proto` | `RunsService` | REST + gRPC | Run lifecycle, SSE events |
| `blobs.proto` | `BlobService` | REST + gRPC | Content-addressed blob storage |
| `components.proto` | `ComponentsService` | REST + gRPC | Component listing |
| `tasks.proto` | `TasksService` | gRPC only | Pull-based task dispatch |
| `orchestrator.proto` | `OrchestratorService` | gRPC only | Worker callbacks (sub-runs, completion) |

REST routes are auto-generated from `google.api.http` annotations using `tonic-rest-build`.

### Pull-based task dispatch

Instead of the orchestrator pushing tasks to workers, workers pull tasks from the orchestrator:

```
StepflowQueuePlugin (implements Plugin trait)
    |
    v
InMemoryTaskTransport --> PullTaskQueue
                              |
                              v
                    TasksService.PullTasks (gRPC stream)
                              |
                              v
                    Python worker (grpc_worker.py)
                              |
                              v
                    OrchestratorService.CompleteTask
```

Each `TaskAssignment` carries a `TaskContext` with `orchestrator_service_url`, eliminating the initialization handshake entirely. Static configuration (blob URL, thresholds) is provided via environment variables at deployment time.

### Self-describing messages

Workers no longer need pre-initialization. Each task carries the orchestrator URL for callbacks:

```protobuf
message TaskContext {
  string orchestrator_service_url = 1;
}
```

Static worker configuration is set via environment variables:
- `STEPFLOW_BLOB_URL`: Blob service address
- `STEPFLOW_BLOB_THRESHOLD_BYTES`: Auto-blobification threshold

## What's implemented

### Phase 1: Proto files + Rust crate (complete)

- 6 proto files with `google.api.http` annotations
- `stepflow-grpc` crate with `tonic-prost-build` + `tonic-rest-build`
- REST route codegen from proto annotations
- `prost-wkt-types` for serde-enabled WKTs (Value, Struct, etc.)

### Phase 2: gRPC + REST server (complete)

- All 6 gRPC service implementations on the orchestrator
- REST routes mounted at `/proto/api/v1` alongside existing aide routes
- Binary-aware blob routes with content negotiation
- Proto-generated OpenAPI spec (`schemas/openapi-proto.json`)
- Journal-based SSE streaming for `GetRunEvents`

### Phase 3: Pull-based task transport (complete)

- `TaskTransport` trait with `InMemoryTaskTransport` implementation
- `StepflowQueuePlugin` implementing the `Plugin` trait
- `TaskCompletionRegistry` for correlating async results
- `TasksService.PullTasks` streaming endpoint
- `OrchestratorService.CompleteTask` for result reporting
- Python gRPC worker (`grpc_worker.py`) with `grpc.aio`
- `GrpcContext` for worker callbacks (blobs via HTTP, sub-runs via gRPC)
- Config: `type: pull` plugin with subprocess management

### Phase 5a: Richer error model (complete)

- `tonic-types` for structured gRPC error details
- `stepflow-grpc/src/error.rs` module with:
  - `invalid_field()` -> `INVALID_ARGUMENT` + `BadRequest` field violations
  - `not_found()` -> `NOT_FOUND` + `ErrorInfo` with resource type/ID
  - `failed_precondition()` -> `FAILED_PRECONDITION` + `ErrorInfo` with reason
  - `internal()` -> `INTERNAL` + `DebugInfo` with error chain
  - `from_execution_error()` for mapping domain errors
- All service implementations use structured errors

### Phase 5b: Task lifecycle hardening (complete)

Two-phase timeout and heartbeat system covering three failure modes:

**Failure modes:**
1. **Queuing failure** — task dispatched but no worker picks it up. Guarded by configurable queue timeout (default 30s).
2. **Worker crash** — worker called `StartTask` but hard-crashed. Detected by heartbeat timeout (5s without a heartbeat).
3. **Worker exception** — component raised an error caught by the worker. Reported via `CompleteTask` with error string.

**Two-phase state machine** (`TaskCompletionRegistry`):
- **Queued phase** — from `register()` to `start_task()`. A spawned watcher task enforces the queue timeout. If `StartTask` arrives after timeout, responds with `timed_out = true`.
- **Executing phase** — from `start_task()` to `complete()`. A heartbeat watcher loop checks freshness every second. If no heartbeat arrives within 5s, the task is failed. An optional execution timeout (configurable, default None) bounds total execution time.

**Proto additions:**
- `StartTaskResponse.timed_out` (bool) — signals the worker to skip execution
- `TaskAssignment.heartbeat_interval_secs` — suggested heartbeat cadence (currently 1s)
- `TaskAssignment.execution_timeout_secs` — optional execution time cap

**Python worker changes:**
- Calls `StartTask` before execution; skips if `timed_out`
- Sends heartbeats from a background `asyncio.Task` every `heartbeat_interval_secs`
- Reuses a shared gRPC channel per task for `StartTask`, heartbeats, and `CompleteTask`
- Handles `should_cancel` from heartbeat responses

**Configuration:**
```yaml
plugins:
  python_grpc:
    type: pull
    command: uv
    args: [...]
    queueTimeoutSecs: 30       # Max time in queue (default 30, null = no timeout)
    executionTimeoutSecs: null  # Max execution time (default null = no limit)
```

**Metrics** (6 new metrics with `task.` prefix):
- `task.queue_duration_seconds` — histogram of time from dispatch to `StartTask`
- `task.execution_duration_seconds` — histogram of time from `StartTask` to `CompleteTask`
- `task.queue_timeout_total` — counter of tasks that timed out in queue
- `task.execution_timeout_total` — counter of tasks that timed out during execution
- `task.success_total` — counter of successfully completed tasks
- `task.failure_total` — counter of failed tasks (errors + timeouts)

**Testing:** 21 tests including deterministic timeout tests using `tokio::time::pause()`.

## Remaining work

### Route swap (aide -> tonic-rest)

The tonic-rest REST routes currently serve at `/proto/api/v1` alongside the existing aide routes at `/api/v1`. Swapping requires:

- **JSON shape alignment:** Proto `CreateRunResponse` wraps `RunSummary` in a nested `summary` field, while aide uses `#[serde(flatten)]` for a flat JSON shape. The CLI (`submit.rs`) depends on the flat shape.
- **Options:** (a) Inline `RunSummary` fields into `CreateRunResponse` proto, (b) update CLI to handle nested shape, or (c) add serde flatten support to prost types.
- **Integration tests:** 27+ tests in `stepflow-server/tests/integration_tests.rs` hit `/api/v1` paths and expect aide response shapes.

### Queue-based transports (Phase 5c)

The current `InMemoryTaskTransport` works for single-orchestrator deployments. For production:

- **SQLite transport:** Write tasks to shared SQLite, `TasksService` polls for pending tasks
- **NATS/Kafka transport:** Publish `TaskAssignment` to message broker topics
- **Autoscaling metrics:** Expose queue depth per plugin as Prometheus metrics
- **Retry coordination:** Orchestrator-side retry with `RetryInfo` from worker errors

### Python SDK improvements

- Package `grpcio-tools` generated stubs properly
- Add `--grpc` CLI flag to `stepflow_py` main entry point
- Support concurrent task execution with `asyncio.Semaphore`

### Deprecate JSON-RPC transport

After pull-based transport is validated in production:

- Mark JSON-RPC as deprecated in config docs
- Remove: `http/client.rs`, `http/bidirectional_driver.rs`, `http_server.py`, `message_decoder.py`
- Remove Pingora load balancer crate (no longer needed for worker dispatch)

## Testing

Integration tests in `tests/grpc/` validate the full pull-based transport:

| Test | What it validates |
|---|---|
| `echo_test.yaml` | Basic gRPC pull transport round-trip |
| `error_handling_test.yaml` | Python exceptions surfaced as `FlowError` via gRPC |
| `multi_step_test.yaml` | Chained pipeline across same gRPC worker |
| `blob_roundtrip_test.yaml` | `put_blob`/`get_blob` via HTTP callbacks during task execution |
| `sub_run_test.yaml` | `submit_run` via `OrchestratorService` gRPC callback |

Run with: `../scripts/test-integration.sh tests/grpc/`

## Configuration

```yaml
plugins:
  python_grpc:
    type: pull
    command: uv
    args: ["--project", "../sdks/python", "run", "python", "-m", "stepflow_py.worker.grpc_worker"]
    queueTimeoutSecs: 30       # Max time in queue before failing (default 30)
    executionTimeoutSecs: null  # Max execution time (default null = no limit)

routes:
  "/python_grpc/{*component}":
    - plugin: python_grpc
```
