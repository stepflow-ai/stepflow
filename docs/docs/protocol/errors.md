---
sidebar_position: 4
---

# Error Handling

Stepflow uses a `TaskErrorCode` enum to categorize errors. Each variant has built-in retryability semantics that the orchestrator uses for automatic retry decisions.

## TaskErrorCode

The `TaskErrorCode` enum is defined in the gRPC protocol (`common.proto`) and used throughout the system — in `FlowError`, `TaskError`, `ItemResult`, and `StepStatus`.

| Code | Description | Retry Behavior |
|------|-------------|----------------|
| `TIMEOUT` | Task exceeded execution deadline or heartbeat timeout | Always retried |
| `UNREACHABLE` | Worker could not be reached (subprocess crash, network timeout) | Always retried |
| `COMPONENT_FAILED` | Component executed but returned a business-logic failure | With `onError: retry` |
| `RESOURCE_UNAVAILABLE` | Resource required by the component was not available | With `onError: retry` |
| `INVALID_INPUT` | Component rejected its input (schema validation, missing fields) | Never |
| `CANCELLED` | Task explicitly cancelled by orchestrator | Never |
| `COMPONENT_NOT_FOUND` | Requested component does not exist on the worker | Never |
| `EXPRESSION_FAILURE` | Orchestrator failed to resolve a value expression (`$step`, `$input`) | Never |
| `ORCHESTRATOR_ERROR` | Catch-all for unexpected orchestrator errors | Never |
| `WORKER_ERROR` | Catch-all for unexpected worker/SDK errors | Never |

## Error Format

Errors in flow results use the `FlowError` structure:

```json
{
  "outcome": "failed",
  "error": {
    "code": "COMPONENT_FAILED",
    "message": "API call returned 503",
    "data": { "stack": [] }
  }
}
```

### Fields

- **`code`** (required): A `TaskErrorCode` string value
- **`message`** (required): Human-readable error description
- **`data`** (optional): Structured error data (stack traces, error context)

## How Errors Flow

### Component errors

When a worker reports a task failure via gRPC `CompleteTask`, it sends a `TaskError` with the appropriate `TaskErrorCode`. The orchestrator uses this directly in the `FlowResult`.

### Transport errors

When the worker is unreachable — subprocess crash, network timeout, connection refused — the orchestrator sets the error code to `UNREACHABLE`. This indicates the component never executed.

### Orchestrator errors

When the orchestrator fails to resolve value expressions (e.g., `$step` reference to unknown step, path to nonexistent field), it creates an `EXPRESSION_FAILURE` error. Other unexpected orchestrator issues produce `ORCHESTRATOR_ERROR`.

## Retry Behavior

The orchestrator uses `TaskErrorCode` variants to make retry decisions:

- **Always retried**: `UNREACHABLE` and `TIMEOUT` errors are retried up to `retry.transportMaxRetries` (default: 3). The plugin's `prepare_for_retry()` is called before each retry.
- **Retried with `onError: retry`**: `COMPONENT_FAILED` and `RESOURCE_UNAVAILABLE` errors are retried only if the step has `onError: { action: retry }`, up to `maxRetries` (default: 3).
- **Never retried**: All other error codes indicate structural problems that won't resolve on retry.
- **Separate budgets**: Transport retries and component retries have independent counters. Exhausting one does not affect the other.

Transport errors and component errors share a single monotonically increasing `attempt` counter visible to the component. See [Component Execution](./methods/components.md#the-attempt-field) for details.

### Error handling actions

Steps can configure `onError` to control behavior on failure:

- **`fail`** (default): Stop workflow execution with the error
- **`useDefault`**: Use a `defaultValue` and mark the step as successful
- **`retry`**: Retry the step up to `maxRetries` times (for component execution errors only)

```yaml
steps:
  - id: flaky_step
    component: /external/api
    onError:
      action: retry
      maxRetries: 5
```

## Python SDK Error Handling

### Exception hierarchy

The Python SDK maps exceptions to `TaskErrorCode` variants for gRPC reporting:

| Exception | TaskErrorCode |
|-----------|--------------|
| `StepflowExecutionError` | `COMPONENT_FAILED` |
| `StepflowRuntimeError` | `RESOURCE_UNAVAILABLE` |
| `StepflowValidationError` | `INVALID_INPUT` |
| `StepflowComponentError` | `COMPONENT_NOT_FOUND` |
| `StepflowError` (base) | `WORKER_ERROR` |

### Raising errors in components

```python
from stepflow_py.worker.exceptions import StepflowExecutionError, StepflowRuntimeError

# Business logic failure (retriable with onError: retry)
raise StepflowExecutionError("API call failed")

# Resource unavailable (retriable with onError: retry)
raise StepflowRuntimeError("Database connection failed")
```

## Orchestrator Service Errors

When a worker calls `OrchestratorService` RPCs (`CompleteTask`, `TaskHeartbeat`, `SubmitRun`, `GetRun`), the orchestrator returns gRPC error codes to signal ownership and availability issues:

| gRPC Code | Meaning | Worker Action |
|-----------|---------|---------------|
| `NOT_FOUND` | The run/task is not on this orchestrator (run migrated or completed) | Call `GetOrchestratorForRun` to discover the current orchestrator, retry on the new URL |
| `UNAVAILABLE` | The orchestrator is unreachable or the run is being recovered | Retry with exponential backoff |

These are distinct from `TaskErrorCode` (which categorizes *component execution* failures). Orchestrator service errors indicate infrastructure-level routing problems, not component logic errors.

### Orchestrator Discovery

When a worker receives `NOT_FOUND` or `UNAVAILABLE` from any `OrchestratorService` RPC, it can call `TasksService.GetOrchestratorForRun` on any orchestrator to discover which orchestrator currently owns the run. This enables automatic recovery when orchestrators restart or runs migrate between orchestrators.

The worker's `OrchestratorTracker` handles this automatically for all RPCs — heartbeat, task completion, subflow submission, and run queries all share the same tracker per task and benefit from discovery performed by any one of them.

