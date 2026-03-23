---
sidebar_position: 4
---

# Implementing Workers

This guide describes how to implement a Stepflow worker in any programming language. Workers are standalone processes that pull tasks from the orchestrator, execute components, and report results back.

While the [Python SDK](./custom-components.md) provides a high-level API that handles protocol details automatically, you can implement workers in any language by following this specification.

## What is a Worker?

A **worker** is a process that hosts one or more workflow components and executes them on behalf of the orchestrator. Workers are registered through [routing configuration](../configuration.md), which maps component paths to named queues. Workers:

- Pull task assignments from a named queue
- Execute components and report results via `CompleteTask`
- Send heartbeats to signal liveness during execution
- Can make callbacks to the orchestrator (e.g., sub-run submission)
- Run as independent processes, enabling language flexibility and fault isolation

See the [Protocol Overview](../protocol/index.md) for how workers fit into the overall architecture, and the [Task Lifecycle](../protocol/task-lifecycle.md) for the detailed task flow.

## Requirements

### Protocol Requirements

| Requirement | Description |
|------------|-------------|
| **Queue Connection** | Workers MUST connect to the configured queue backend (gRPC `PullTasks` or NATS consumer) |
| **Task Completion** | Workers MUST report results via `CompleteTask` on `OrchestratorService` |
| **Heartbeats** | Workers MUST send `TaskHeartbeat` before and during execution |
| **Component Listing** | Workers MUST handle `list_components` task assignments |
| **Component Execution** | Workers MUST handle `execute` task assignments |
| **Error Reporting** | Workers MUST report failures with appropriate `TaskErrorCode` |
| **Bidirectional Calls** | Workers MAY call `SubmitRun` and `GetRun` during execution |

### Observability Requirements

| Requirement | Description |
|------------|-------------|
| **OTLP Tracing** | Workers SHOULD support OpenTelemetry for distributed tracing |
| **Structured Logging** | Workers SHOULD use structured JSON logging with diagnostic context |
| **Context Propagation** | Workers SHOULD propagate `ObservabilityContext` in bidirectional calls |

## Worker Configuration

Workers discover the orchestrator and queue configuration through environment variables. When launched as a subprocess, the orchestrator sets these automatically:

| Variable | Description |
|----------|-------------|
| `STEPFLOW_TRANSPORT` | Queue backend: `grpc` or `nats` |
| `STEPFLOW_QUEUE_NAME` | Queue name to pull tasks from |
| `STEPFLOW_TASKS_URL` | Orchestrator gRPC address (for gRPC transport) |
| `STEPFLOW_BLOB_URL` | BlobService gRPC address |

For remote workers (e.g., in Kubernetes), configure these in the deployment manifest.

## Protocol Methods

### Component Listing (MUST)

When the worker receives a `list_components` task, it MUST enumerate all registered components and report them via `CompleteTask` with a `ListComponentsResult`.

### Component Execution (MUST)

When the worker receives an `execute` task, it MUST:
1. Send a `TaskHeartbeat` immediately (transitions task to EXECUTING)
2. Execute the component with the provided input
3. Send periodic heartbeats during execution (resets crash-detection timer)
4. Report the result via `CompleteTask` (success or failure)

### Bidirectional Callbacks (MAY)

During execution, workers MAY call back to the orchestrator:

| RPC | Description |
|-----|-------------|
| `SubmitRun` | Submit a sub-workflow for execution |
| `GetRun` | Query run status and results |

Workers use the `orchestrator_service_url` from `TaskContext` for callbacks. See [Task Lifecycle — Bidirectional Callbacks](../protocol/task-lifecycle.md#bidirectional-callbacks) for details.

### Blob Storage

Workers access blob storage via the `BlobService` gRPC API. See [Blob Storage](../protocol/blobs.md) for details.

## Error Handling

Workers MUST report failures via `CompleteTask` with an appropriate `TaskErrorCode`:

| Code | When to Use |
|------|------------|
| `COMPONENT_FAILED` | Component executed but returned a business-logic failure |
| `INVALID_INPUT` | Input validation failure |
| `COMPONENT_NOT_FOUND` | Requested component doesn't exist on this worker |
| `RESOURCE_UNAVAILABLE` | External resource unavailable |
| `WORKER_ERROR` | Unexpected worker/SDK error |

See [Error Handling](../protocol/errors.md) for the complete reference and retry behavior.

## Observability

Each task carries an `ObservabilityContext` with OpenTelemetry trace/span IDs. Workers should extract the parent context, create child spans for component execution, and propagate context in bidirectional calls.

Workers SHOULD support these environment variables:

| Variable | Description | Default |
|----------|-------------|---------|
| `STEPFLOW_OTLP_ENDPOINT` | OTLP collector endpoint | none |
| `STEPFLOW_SERVICE_NAME` | Service name for traces/logs | `stepflow-worker` |
| `STEPFLOW_TRACE_ENABLED` | Enable tracing | `true` if endpoint set |
| `STEPFLOW_LOG_LEVEL` | Log level | `INFO` |
| `STEPFLOW_LOG_DESTINATION` | Where to log (stderr, file, otlp) | `otlp` if endpoint set, else `stderr` |

## Implementation Checklist

### Required (MUST)

- [ ] Connect to queue backend (gRPC `PullTasks` or NATS consumer)
- [ ] Handle `list_components` task assignments
- [ ] Handle `execute` task assignments
- [ ] Send `TaskHeartbeat` before execution starts
- [ ] Send periodic heartbeats during execution
- [ ] Report results via `CompleteTask`
- [ ] Report failures with `TaskError` and `TaskErrorCode`

### Recommended (SHOULD)

- [ ] OpenTelemetry tracing with parent context extraction
- [ ] Structured JSON logging with diagnostic context
- [ ] OTLP export for traces and logs
- [ ] Graceful shutdown handling
- [ ] Progress reporting in heartbeats

## Reference Implementations

- **Python SDK**: See [sdks/python/stepflow-py](https://github.com/stepflow-ai/stepflow/tree/main/sdks/python/stepflow-py) for a complete implementation
- **Proto definitions**: See [proto/stepflow/v1/](https://github.com/stepflow-ai/stepflow/tree/main/proto/stepflow/v1) for the gRPC service definitions

## See Also

- [Protocol Overview](../protocol/index.md) — Architecture and queue backends
- [Task Lifecycle](../protocol/task-lifecycle.md) — Detailed task flow
- [Error Handling](../protocol/errors.md) — Error codes reference
- [Custom Components](./custom-components.md) — Python SDK guide
