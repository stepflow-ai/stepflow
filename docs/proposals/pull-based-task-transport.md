# Proposal: Pull-Based Task Transport with Protocol Buffers + gRPC

**Status:** In Progress (Phase 3 complete, Phases 5a–5c complete, Phase 6 in progress)
**Created:** March 2026

## Summary

Change Stepflow's orchestrator-worker communication model from *orchestrator pushes tasks to workers* to *workers pull tasks from the orchestrator*. This reversal fundamentally changes the RPC direction: instead of the orchestrator calling into workers (requiring initialization, load balancing, and bidirectional SSE for callbacks), workers initiate all connections — pulling tasks via a gRPC stream and calling back to the orchestrator for heartbeats, completion, sub-run submission, and blob operations.

As part of this transition, we adopt Protocol Buffers + gRPC as the wire protocol. gRPC provides the streaming primitives needed for pull-based dispatch (server-streaming `PullTasks`), supports Python 3.10+ (unlike some HTTP/2 libraries), and generates typed client/server code for all SDK languages.

## Motivation

### Why pull-based?

The push-based architecture (orchestrator → worker RPCs) has fundamental limitations:

1. **Stateful initialization (#716):** The orchestrator must initialize each worker with configuration (blob API URLs, thresholds) before sending tasks. Behind a load balancer, this creates race conditions — different orchestrator instances may hit uninitialized workers, or a worker restart loses its initialization state.

2. **No backpressure:** The orchestrator POSTs tasks directly to workers. A busy worker has no way to signal "I'm full" — it must accept and queue internally, or reject and hope the orchestrator retries.

3. **Bidirectional complexity:** Workers need to call back to the orchestrator during execution (for sub-run submission, blob operations). The push model requires a reverse channel — implemented via Server-Sent Events (SSE) where the worker opens an SSE connection and the orchestrator correlates responses by `instance_id`. This is complex and fragile.

4. **Recovery difficulty:** When an orchestrator crashes and restarts, it has no way to reconnect to workers that were executing tasks. The push model assumes the orchestrator controls the connection lifecycle.

**Pull-based solves all four:**
- Workers self-configure from environment variables — no initialization handshake
- Workers control concurrency via `max_concurrent` in `PullTasks` — natural backpressure
- Workers call the orchestrator directly for callbacks — no SSE reverse channel needed
- Workers reconnect automatically after orchestrator restart — the orchestrator just re-registers task_ids and workers' retries land

### Why gRPC?

The pull-based model reverses the RPC direction, requiring workers to stream tasks from the orchestrator and make callbacks. gRPC is a natural fit:

1. **Server-streaming for task dispatch:** `PullTasks` returns a stream of `TaskAssignment` messages — the orchestrator pushes tasks to connected workers as they become available.
2. **Typed callbacks:** Worker→orchestrator RPCs (`TaskHeartbeat`, `CompleteTask`, `SubmitRun`) are defined in proto and code-generated for all SDKs.
3. **Python 3.10 support:** `grpcio` supports Python 3.10+ with async (`grpc.aio`), enabling the worker to handle multiple concurrent tasks.
4. **Multi-language code generation:** Proto files generate typed stubs for Python, Go, TypeScript, etc. — replacing the fragile JSON Schema → `datamodel-code-generator` pipeline.

## Architecture

### RPC direction change

**Before (push-based):**
```
Orchestrator ──HTTP POST──> Worker (component execution)
Worker ──SSE stream──> Orchestrator (sub-run submission, blob ops)
```
The orchestrator initiates connections to workers. Workers need a reverse channel (SSE) for callbacks.

**After (pull-based):**
```
Worker ──gRPC PullTasks stream──> Orchestrator (receives tasks)
Worker ──gRPC TaskHeartbeat──> Orchestrator (claims task, reports liveness)
Worker ──gRPC CompleteTask──> Orchestrator (delivers result)
Worker ──gRPC SubmitRun/GetRun──> Orchestrator (sub-run callbacks)
Worker ──gRPC PutBlob/GetBlob──> Orchestrator (blob operations)
```
All connections are initiated by the worker. The orchestrator is a server that workers connect to. No reverse channel needed.

### Proto files as the source of truth

All service definitions live in `proto/stepflow/v1/*.proto`:

| Proto file | Service | Direction | Purpose |
|---|---|---|---|
| `common.proto` | — | — | Shared types: enums, error types, item results |
| `health.proto` | `HealthService` | Client → Orchestrator | Service health check |
| `flows.proto` | `FlowsService` | Client → Orchestrator | Flow CRUD |
| `runs.proto` | `RunsService` | Client → Orchestrator | Run lifecycle, SSE events |
| `blobs.proto` | `BlobService` | Worker → Orchestrator | Content-addressed blob storage |
| `components.proto` | `ComponentsService` | Client → Orchestrator | Component listing |
| `tasks.proto` | `TasksService` | Worker → Orchestrator | Pull-based task dispatch |
| `orchestrator.proto` | `OrchestratorService` | Worker → Orchestrator | Heartbeat, completion, sub-runs |

Client-facing services (flows, runs, health, components) are exposed as both REST and gRPC. Worker-facing services (tasks, orchestrator, blobs) are gRPC only.

REST routes are auto-generated from `google.api.http` annotations using `tonic-rest-build`.

### Pull-based task dispatch

Workers pull tasks from the orchestrator via a gRPC server-streaming RPC:

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
                    OrchestratorService.TaskHeartbeat + CompleteTask
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

### Task lifecycle

Tasks move through a well-defined lifecycle:

```
Dispatched ──TaskHeartbeat──> Executing ──CompleteTask──> Completed
    |            (first)           |           |
    |                              |           v
    v                              v        (result delivered
 Queue timeout              Heartbeat       via TaskRegistry)
 (no heartbeat              timeout
  received)                 (worker crash)
    |                              |
    v                              v
 Failed                        Failed
 (TRANSPORT_ERROR)            (TRANSPORT_ERROR)
    |                              |
    v                              v
 Retry with same task_id     Retry with same task_id
```

**Key invariants:**
- **Task IDs are stable across retries.** When a task fails (timeout, crash, or transport error), the retry uses the same `task_id`. This allows late results from the original execution to still land via `CompleteTask`.
- **First result wins.** If both a worker retry and a re-dispatched execution complete the same `task_id`, the first `CompleteTask` call wins. The second gets `NOT_FOUND`.
- **Worker deduplication.** Each worker identifies itself with a `worker_id`. If two workers claim the same task, the first one wins — the second gets `ALREADY_CLAIMED` on its heartbeat.

### Task ID journalling and crash recovery

Task IDs are journalled in `TasksStarted` events so they survive orchestrator crashes:

1. **Before dispatch:** The executor generates a `task_id`, journals it in `TasksStarted`, then dispatches to the plugin.
2. **On crash recovery:** Journal replay identifies in-flight tasks (those with `TasksStarted` but no `TaskCompleted`). The recovered task_ids are reused when re-dispatching.
3. **Checkpoint support:** In-flight task_ids are included in execution checkpoints, so checkpoint-accelerated recovery also preserves them.
4. **Worker retry:** After an orchestrator crash, the worker retries `CompleteTask` with the original task_id. When the orchestrator recovers and re-registers the same task_id, the retry delivers the result — avoiding unnecessary re-execution.
5. **At-least-once semantics:** In the worst case (worker also crashed, or retry exhausted), the task is re-dispatched with the same task_id and re-executed. Components should be idempotent.

**Recovery flow:**
```
                    Orchestrator crashes
                           |
Worker has in-flight task  |  Orchestrator restarts
  - Completes execution   |  - Replays journal
  - CompleteTask fails     |  - Finds in-flight task_ids
  - Retries with backoff   |  - Re-dispatches with same task_id
                           |
                    ┌──────┴──────┐
                    v             v
              Worker retry    Re-dispatched task
              lands first     picked up by worker
                    |             |
                    v             v
              First CompleteTask wins
              Second gets NOT_FOUND (harmless)
```

### Unified heartbeat protocol

The `TaskHeartbeat` RPC combines the initial task claim with ongoing liveness reporting. Workers call it immediately before starting execution (replacing the former `StartTask` RPC) and periodically during execution.

```protobuf
message TaskHeartbeatRequest {
  string task_id = 1;
  string worker_id = 2;      // Stable per-worker, used for deduplication
  optional float progress = 3;
  optional string status_message = 4;
}

message TaskHeartbeatResponse {
  bool should_abort = 1;      // Worker should stop execution
  TaskStatus status = 2;      // Reason for abort (or IN_PROGRESS)
}

enum TaskStatus {
  TASK_STATUS_UNSPECIFIED = 0;
  TASK_STATUS_IN_PROGRESS = 1;      // Task is yours, continue
  TASK_STATUS_ALREADY_CLAIMED = 2;  // Different worker owns this task
  TASK_STATUS_COMPLETED = 3;        // Task already has a result
  TASK_STATUS_TIMED_OUT = 4;        // Task expired in queue
  TASK_STATUS_NOT_FOUND = 5;        // Task ID not recognized
}
```

**Behavior on first call (task in Queued phase):**
- Transitions task to Executing, records `worker_id`
- Returns `IN_PROGRESS` — worker should proceed with execution

**Behavior on subsequent calls:**
- Same `worker_id`: records heartbeat, returns `IN_PROGRESS`
- Different `worker_id`: returns `ALREADY_CLAIMED` — worker should abort
- Task already completed: returns `NOT_FOUND` — worker should abort

### Result delivery architecture

Result delivery is decoupled from task dispatch via the shared `TaskRegistry`:

```
TaskRegistry (stepflow-plugin)
  - In-memory DashMap<task_id, oneshot::Sender>
  - register(task_id) -> Receiver
  - complete(task_id, result) -> bool
  - Executor registers, awaits receiver
  - Plugins/workers call complete()

PendingTasks (stepflow-grpc)
  - Augments TaskRegistry with gRPC-specific lifecycle
  - Phase tracking (Queued -> Executing)
  - Worker_id tracking for deduplication
  - Heartbeat timeout detection (5s)
  - Queue timeout detection (configurable)
  - Delegates result delivery to TaskRegistry
```

**Plugin trait:**
```rust
trait Plugin {
    async fn start_task(
        &self,
        task_id: &str,        // Pre-generated by executor
        component: &Component,
        run_context: &Arc<RunContext>,
        step: Option<&StepId>,
        input: ValueRef,
        attempt: u32,
    ) -> Result<()>;          // Result delivered via TaskRegistry
}
```

Synchronous plugins (builtins, protocol, MCP) call `TaskRegistry.complete()` inline. Queue-based plugins dispatch to the queue and return — the worker delivers via `CompleteTask` RPC.

### CompleteTask retry

The Python gRPC worker retries `CompleteTask` with exponential backoff on transient failures:

- **UNAVAILABLE:** Orchestrator is down (restarting). Reconnects gRPC channel.
- **NOT_FOUND:** Orchestrator is up but hasn't re-registered this `task_id` yet (recovery in progress).
- 5 retries with exponential backoff (2s, 4s, 8s, 16s, 30s cap).

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
- `TaskRegistry` for correlating async results (decoupled from dispatch)
- `TasksService.PullTasks` streaming endpoint
- `OrchestratorService.CompleteTask` for result reporting
- Python gRPC worker (`grpc_worker.py`) with `grpc.aio`
- `GrpcContext` for worker callbacks (blobs via gRPC, sub-runs via gRPC)
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

### Phase 5b: Unified heartbeat + worker deduplication (complete)

Unified `TaskHeartbeat` RPC combining task claiming and liveness reporting:

**Heartbeat-based lifecycle:**
- Workers call `TaskHeartbeat` before execution (claims the task) and periodically during execution (reports liveness)
- Each call includes a `worker_id` for deduplication
- Response includes `TaskStatus` enum and `should_abort` flag
- If two workers receive the same task, first heartbeat wins — second gets `ALREADY_CLAIMED`

**Two-phase timeout system:**
- **Queue timeout** — configurable per-plugin (default 30s). Task fails if no heartbeat arrives within this window.
- **Heartbeat timeout** — fixed 5s. Executing tasks that stop heartbeating are presumed crashed.
- **Execution timeout** — optional cap on total execution time from first heartbeat to completion.

**Configuration:**
```yaml
plugins:
  python_grpc:
    type: pull
    command: uv
    args: [...]
    queueTimeoutSecs: 30       # Max time without heartbeat (default 30)
    executionTimeoutSecs: null  # Max execution time (default null = no limit)
```

**Metrics** (6 metrics with `task.` prefix):
- `task.queue_duration_seconds` — histogram of time from dispatch to first heartbeat
- `task.execution_duration_seconds` — histogram of time from first heartbeat to `CompleteTask`
- `task.queue_timeout_total` — counter of tasks that timed out in queue
- `task.execution_timeout_total` — counter of tasks that timed out during execution
- `task.success_total` — counter of successfully completed tasks
- `task.failure_total` — counter of failed tasks (errors + timeouts)

### Phase 5c: Task ID journalling + crash recovery (complete)

**Task ID stability:**
- Task IDs generated before journalling `TasksStarted` events
- Same task_id reused on all retries (transport errors, recovery)
- In-flight task_ids persisted in execution checkpoints
- Journal replay and checkpoint recovery both restore in-flight task_ids

**Recovery integration tests** (`tests/recovery/`):
- All 12 recovery tests migrated to gRPC pull transport
- Worker uses dual gRPC pull loops (one per orchestrator) with persistent reconnection
- `test_orchestrator_crash_worker_delivers_result`: verifies worker CompleteTask retry delivers result after orchestrator recovery without re-execution
- Tests account for at-least-once semantics (some steps may complete via worker retry)

**Python worker CompleteTask retry:**
- Retries on `UNAVAILABLE` (orchestrator down) and `NOT_FOUND` (recovery in progress)
- Exponential backoff: 2s, 4s, 8s, 16s, 30s cap, 5 retries max

## Remaining work

### Route swap (aide -> tonic-rest)

The tonic-rest REST routes currently serve at `/proto/api/v1` alongside the existing aide routes at `/api/v1`. Swapping requires:

- **JSON shape alignment:** Proto `CreateRunResponse` wraps `RunSummary` in a nested `summary` field, while aide uses `#[serde(flatten)]` for a flat JSON shape. The CLI (`submit.rs`) depends on the flat shape.
- **Options:** (a) Inline `RunSummary` fields into `CreateRunResponse` proto, (b) update CLI to handle nested shape, or (c) add serde flatten support to prost types.
- **Integration tests:** 27+ tests in `stepflow-server/tests/integration_tests.rs` hit `/api/v1` paths and expect aide response shapes.

### Cross-orchestrator result routing (#777)

When a run migrates to a different orchestrator (lease expiry), the worker's `CompleteTask` retry hits the wrong orchestrator. Proposed:
- `GetOrchestratorForRun(run_id)` RPC on `TasksService` using the lease manager
- Worker calls this on first `CompleteTask` failure to find the current owner

### Queue-based transports

The current `InMemoryTaskTransport` works for single-orchestrator deployments. For production:

- **SQLite transport:** Write tasks to shared SQLite, `TasksService` polls for pending tasks
- **NATS/Kafka transport:** Publish `TaskAssignment` to message broker topics
- **Autoscaling metrics:** Expose queue depth per plugin as Prometheus metrics

### Deprecate JSON-RPC transport

After pull-based transport is validated in production:

- Mark JSON-RPC as deprecated in config docs
- Remove: `http/client.rs`, `http/bidirectional_driver.rs`, `http_server.py`, `message_decoder.py`
- Remove Pingora load balancer crate (no longer needed for worker dispatch)

## Testing

### Unit tests

- `stepflow-plugin`: `TaskRegistry` register/complete/remove, duplicate detection
- `stepflow-grpc`: `PendingTasks` timeout tests (queue, heartbeat, execution), worker deduplication (`AlreadyClaimed`)
- `stepflow-execution`: Recovery tests for task_id preservation (journal replay + checkpoint-accelerated)

### Integration tests

Tests in `tests/grpc/` validate the full pull-based transport:

| Test | What it validates |
|---|---|
| `echo_test.yaml` | Basic gRPC pull transport round-trip |
| `error_handling_test.yaml` | Python exceptions surfaced as `FlowError` via gRPC |
| `multi_step_test.yaml` | Chained pipeline across same gRPC worker |
| `blob_roundtrip_test.yaml` | `put_blob`/`get_blob` via gRPC callbacks during task execution |
| `sub_run_test.yaml` | `submit_run` via `OrchestratorService` gRPC callback |

Run with: `../scripts/test-integration.sh tests/grpc/`

### Recovery integration tests

Tests in `tests/recovery/` validate crash recovery with gRPC pull transport:

| Test | What it validates |
|---|---|
| `test_restart_recovery_sequential` | Orchestrator restart (same ID), sequential workflow |
| `test_restart_recovery_parallel` | Orchestrator restart, parallel workflow with at-least-once semantics |
| `test_failover_recovery_*` | Orchestrator failover (permanent kill, second orchestrator claims) |
| `test_subflow_*` | Subflow recovery (resumed in-place, not restarted) |
| `test_worker_crash_single_retry` | Worker crash, orchestrator retries via heartbeat timeout |
| `test_worker_crash_exhausts_retries` | Worker stays down, retries exhaust |
| `test_worker_and_orchestrator_crash` | Both crash and restart, full recovery |
| `test_orchestrator_crash_worker_delivers_result` | Worker delivers result via CompleteTask retry after orchestrator recovery |

Run with: `./scripts/check-recovery.sh`

## Configuration

```yaml
plugins:
  python_grpc:
    type: pull
    command: uv
    args: ["--project", "../sdks/python", "run", "stepflow_worker"]
    queueTimeoutSecs: 30       # Max time without heartbeat (default 30)
    executionTimeoutSecs: null  # Max execution time (default null = no limit)

routes:
  "/python_grpc/{*component}":
    - plugin: python_grpc
```
