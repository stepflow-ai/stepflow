---
sidebar_position: 4
---

# Error Handling

The Stepflow Protocol uses JSON-RPC 2.0 error handling with standardized error codes organized into sub-ranges by origin and retryability. This specification defines the error code ranges, their retry behavior, and best practices for error handling.

## Error Response Format

All errors follow the JSON-RPC 2.0 error response specification:

```json
{
  "jsonrpc": "2.0",
  "id": "<request_id>",
  "error": {
    "code": <error_code>,
    "message": "<human_readable_message>",
    "data": <optional_additional_data>
  }
}
```

### Error Response Fields

- **`code`** (required): Integer error code from the ranges below
- **`message`** (required): Human-readable error description
- **`data`** (optional): Additional structured error information for debugging

## Error Code Ranges

All error codes use JSON-RPC negative ranges. Retryability is determined by which range a code falls in.

| Range | Category | Retry Behavior |
|-------|----------|----------------|
| -32700 to -32600 | JSON-RPC Standard | Never |
| -32000 to -32010 | Component Server — predefined | Never |
| -32011 to -32099 | Component Server — user-defined | Never |
| -32100 to -32109 | Component Execution — predefined | With `onError: { action: retry }` |
| -32110 to -32199 | Component Execution — user-defined | With `onError: { action: retry }` |
| -32200 to -32299 | Orchestrator | Never |
| -32300 to -32399 | Transport | Always |

The `ErrorCode` enum in the protocol schema (`schemas/protocol.json`) defines all well-known codes. The Python SDK auto-generates a matching `ErrorCode(IntEnum)` class from this schema. Error code fields accept any integer — the enum provides convenience constants, not an exhaustive constraint.

## Standard Error Codes

### JSON-RPC Standard Errors

| Code | Name | Description |
|------|------|-------------|
| -32700 | Parse Error | Invalid JSON was received |
| -32600 | Invalid Request | The JSON sent is not a valid Request object |
| -32601 | Method Not Found | The method does not exist / is not available |
| -32602 | Invalid Params | Invalid method parameter(s) |
| -32603 | Internal Error | Internal JSON-RPC error |

### Component Server Errors (-32000 to -32099)

Structural errors in the component server process itself — not in user code. These indicate SDK infrastructure issues (wrong component name, server not ready, schema mismatch). **Never retried**, even with `onError: { action: retry }`.

**Predefined (-32000 to -32010):**

| Code | Name | Description |
|------|------|-------------|
| -32000 | Server Error | Generic server error |
| -32001 | Component Not Found | Requested component does not exist on the server |
| -32002 | Server Not Initialized | Server has not been initialized |
| -32003 | Invalid Input Schema | Input does not match the component's declared schema |
| -32004 | Invalid Value | Invalid value for a protocol field |
| -32005 | Not Found | Referenced entity not found (flow, blob, etc.) |
| -32006 | Protocol Version Mismatch | Protocol version incompatibility between orchestrator and component server |
| -32007 | Dependency Error | Required dependency unavailable (e.g., Python import failure) |
| -32008 | Configuration Error | Invalid or incompatible configuration |

**User-defined (-32011 to -32099):** Available for SDK implementers to define non-retryable server errors.

### Component Execution Errors (-32100 to -32199)

Errors from user component code — the component ran but its logic failed. These are the only errors eligible for retry via `onError: { action: retry }`.

**Predefined (-32100 to -32109):**

| Code | Name | Description |
|------|------|-------------|
| -32100 | Component Execution Failed | Unhandled exception in component code |
| -32101 | Component Value Error | Invalid value in component logic |
| -32102 | Component Resource Unavailable | Required resource not available during execution |
| -32103 | Component Bad Request | Invalid input structure in component logic |

**User-defined (-32110 to -32199):** Available for component developers to define custom retryable error codes.

### Orchestrator Errors (-32200 to -32299)

Errors from the orchestrator during expression evaluation or input resolution. The flow definition or step outputs have structural problems. **Never retried** — the flow definition won't change between attempts.

| Code | Name | Description |
|------|------|-------------|
| -32200 | Undefined Field | A referenced field does not exist in the step output, input, or variable |
| -32201 | Entity Not Found | A referenced entity was not found (e.g., unknown step name) |
| -32202 | Internal Error | Unexpected internal error |

### Transport Errors (-32300 to -32399)

Infrastructure failures — the component never ran or didn't complete. The JSON-RPC communication channel itself failed. **Always retried** up to `retry.transportMaxRetries` (default: 3).

| Code | Name | Description |
|------|------|-------------|
| -32300 | Transport Error | Generic transport/infrastructure failure |
| -32301 | Transport Spawn Error | Subprocess failed to start or crashed on startup |
| -32302 | Transport Connection Error | HTTP request/response or SSE stream failure |
| -32303 | Transport Protocol Error | Garbled or unparseable JSON-RPC message |

## How Errors Flow

When a step executes, errors are classified based on how they originated:

### Component server errors

When a component server returns a JSON-RPC error response (e.g., the component raised an exception), the orchestrator converts it to a `FlowResult::Failed` with the **original error code** preserved. For example, if a Python component raises an exception, the SDK returns a `-32100` (Component Execution Failed) error, and the run result will show `code: -32100`.

### Transport errors

When the component server is unreachable — subprocess crash, network timeout, connection refused — the orchestrator sets the error code to `-32300` (Transport Error). This indicates the component never ran or didn't complete.

### Orchestrator errors

When the orchestrator itself encounters an error during expression evaluation or input resolution (e.g., a `$step` reference to an unknown step, or a path that doesn't exist in the output), it creates an error with an orchestrator code (-32200 to -32299).

## Retry Behavior

The orchestrator uses error code ranges to make retry decisions:

- **Transport errors (-32300 to -32399)**: Always retried up to `retry.transportMaxRetries` (default: 3). The plugin's `prepare_for_retry()` is called before each retry to allow resource recovery (e.g., restarting a crashed subprocess).
- **Component execution errors (-32100 to -32199)**: Retried only if the step has `onError: { action: retry }`, up to `maxRetries` (default: 3).
- **All other errors**: Never retried. Component server errors, orchestrator errors, and JSON-RPC standard errors indicate structural problems that won't resolve on retry.
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

## User-Defined Error Codes

Component developers can define custom error codes in two ranges:

- **Non-retryable (-32011 to -32099):** For server-level errors that shouldn't be retried.
- **Retryable (-32110 to -32199):** For component logic errors that may succeed on retry.

In the Python SDK, use `StepflowFailed` to raise errors with custom codes:

```python
from stepflow_py.worker.exceptions import StepflowFailed

# Custom retryable error (in component execution range)
raise StepflowFailed(error_code=-32150, message="Rate limited, try again later")

# Custom non-retryable error (in component server range)
raise StepflowFailed(error_code=-32050, message="Invalid API key")
```

## Using ErrorCode in the Python SDK

The Python SDK provides an auto-generated `ErrorCode` enum with all well-known codes:

```python
from stepflow_py import ErrorCode

# Use in custom exceptions
from stepflow_py.worker.exceptions import StepflowError

raise StepflowError("Something went wrong", code=ErrorCode.COMPONENT_RESOURCE_UNAVAILABLE)

# Check error classification
from stepflow_py.worker.exceptions import is_transport_error, is_component_execution_error

is_transport_error(-32300)            # True
is_transport_error(-32100)            # False
is_component_execution_error(-32100)  # True
is_component_execution_error(-32000)  # False
```

Component servers can use any integer error code — the `ErrorCode` enum provides well-known constants but does not restrict the values.
