# Proposal: Pull-Based Task Transport with Protocol Buffers + gRPC

**Status:** Complete
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
  optional string run_id = 5;
}

message TaskHeartbeatResponse {
  bool should_abort = 1;      // Worker should stop execution
  TaskStatus status = 2;      // Reason for abort (or IN_PROGRESS)
}

enum TaskStatus {
  TASK_STATUS_UNSPECIFIED = 0;
  TASK_STATUS_IN_PROGRESS = 1;      // Task is yours, continue
  TASK_STATUS_ALREADY_CLAIMED = 2;  // Different worker owns this task
}
```

**Behavior on first call (task in Queued phase):**
- Transitions task to Executing, records `worker_id`
- Returns `IN_PROGRESS` — worker should proceed with execution

**Behavior on subsequent calls:**
- Same `worker_id`: records heartbeat, returns `IN_PROGRESS`
- Different `worker_id`: returns `ALREADY_CLAIMED` — worker should abort

**Error conditions (gRPC error codes):**
- Task not found (completed, timed out, or run moved): gRPC `NOT_FOUND`
- Run being recovered: gRPC `UNAVAILABLE`

On `NOT_FOUND` or `UNAVAILABLE`, the worker calls `TasksService.GetOrchestratorForRun` to discover the current orchestrator and redirects all RPCs for this task.

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

### CompleteTask retry and orchestrator discovery

The Python gRPC worker retries `CompleteTask` with exponential backoff on transient failures:

- **gRPC `UNAVAILABLE`:** Orchestrator is down or recovering. On first failure, the worker calls `GetOrchestratorForRun` to discover the current orchestrator. If the URL changed, retries reset for the new URL.
- **gRPC `NOT_FOUND`:** Task not on this orchestrator (run moved). The worker discovers the new orchestrator and retries once on the new URL.
- 5 retries with exponential backoff (2s, 4s, 8s, 16s, 30s cap).

All `OrchestratorService` RPCs (`CompleteTask`, `TaskHeartbeat`, `SubmitRun`, `GetRun`) share a per-task `OrchestratorTracker` that caches the current orchestrator URL. Discovery in any one RPC updates the tracker for all others.

## Implementation summary

### Proto files + gRPC server

- 8 proto files with `google.api.http` annotations in `proto/stepflow/v1/`
- `stepflow-grpc` crate with `tonic-prost-build` + `tonic-rest-build`
- All gRPC service implementations on the orchestrator (multiplexed on same port as HTTP)
- REST route codegen from proto annotations
- Structured gRPC errors via `tonic-types` (`INVALID_ARGUMENT`, `NOT_FOUND`, etc.)

### Pull-based task transport

- `TaskTransport` trait with `InMemoryTaskTransport` implementation
- `StepflowQueuePlugin` implementing the `Plugin` trait via `TaskRegistry`
- `TasksService.PullTasks` streaming endpoint for task dispatch
- `OrchestratorService.TaskHeartbeat` for unified task claiming + liveness
- `OrchestratorService.CompleteTask` for result reporting
- Python gRPC worker (`grpc_worker.py`) with `grpc.aio`, concurrent task execution
- `GrpcContext` for worker callbacks (blobs, sub-runs — all via gRPC)
- Config: `type: grpc` plugin with subprocess management and queue timeout

### Heartbeat + worker deduplication

- Unified `TaskHeartbeat` RPC: first call claims task, subsequent calls report liveness
- `worker_id` tracking: if two workers receive the same task, first heartbeat wins (second gets `ALREADY_CLAIMED`)
- Two-phase timeout: queue timeout (configurable) + heartbeat crash detection (5s) + optional execution cap
- 6 task metrics (`task.queue_duration_seconds`, `task.success_total`, etc.)

### Crash recovery (#746)

- Task IDs journalled in `TasksStarted` events, persisted in checkpoints
- Same task_id reused on all retries (transport errors, orchestrator recovery)
- `TaskRegistry` decouples dispatch from result delivery — first `CompleteTask` wins
- Python worker retries `CompleteTask` with orchestrator discovery on `UNAVAILABLE`/`NOT_FOUND`
- Recovery integration tests migrated to gRPC pull transport (12 tests)

## Follow-up work

These are tracked as separate issues and are not part of the core pull-based transport proposal:

- **Cross-orchestrator result routing (#777):** `GetOrchestratorForRun` RPC for workers to find the current run owner when a run migrates to a different orchestrator
- **Route swap (aide → tonic-rest):** Replace aide-generated REST routes with proto-generated routes at `/api/v1`
- **Queue-based transports:** SQLite, NATS, or Kafka transport implementations for multi-orchestrator deployments
- **JSON-RPC deprecation:** Remove push-based HTTP transport after pull-based is validated in production

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
    type: grpc
    command: uv
    args: ["--project", "../sdks/python", "run", "stepflow_worker"]
    queueTimeoutSecs: 30       # Max time without heartbeat (default 30)
    executionTimeoutSecs: null  # Max execution time (default null = no limit)

routes:
  "/python_grpc/{*component}":
    - plugin: python_grpc
```
