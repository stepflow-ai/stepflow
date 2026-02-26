---
sidebar_position: 4
---

# Error Handling

The Stepflow Protocol uses JSON-RPC 2.0 error handling with standardized error codes for consistent error reporting across all implementations. This specification defines standard error codes, error response formats, and best practices for error handling.

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

- **`code`** (required): Integer error code from standardized ranges
- **`message`** (required): Human-readable error description
- **`data`** (optional): Additional structured error information for debugging

## Error Code Ranges

All error codes across the platform are organized into four ranges:

| Range | Category | Description |
|-------|----------|-------------|
| -32700 to -32600 | JSON-RPC Standard | Standard JSON-RPC 2.0 error codes |
| -32099 to -32000 | Stepflow Protocol | Errors between orchestrator and component servers |
| 1 to 4999 | Orchestrator / Component | Errors from expression evaluation, input validation, or component logic |
| 5000+ | Transport | Infrastructure failures — the component never ran or didn't complete |

These ranges are used consistently in JSON-RPC error responses between orchestrator and component servers, and in `FlowResult` error codes surfaced in run results.

The `ErrorCode` enum in the protocol schema (`schemas/protocol.json`) defines all well-known codes. The Python SDK auto-generates a matching `ErrorCode(IntEnum)` class from this schema. Error code fields accept any integer — the enum provides convenience constants, not an exhaustive constraint.

## Standard Error Codes

### JSON-RPC Standard Errors

| Code | Name | Description | When to Use |
|------|------|-------------|-------------|
| -32700 | Parse Error | Invalid JSON was received | Malformed JSON in request |
| -32600 | Invalid Request | The JSON sent is not a valid Request object | Missing required fields, invalid structure |
| -32601 | Method Not Found | The method does not exist / is not available | Unknown method name |
| -32602 | Invalid Params | Invalid method parameter(s) | Schema validation failure, wrong parameter types |
| -32603 | Internal Error | Internal JSON-RPC error | Server implementation error |

### Stepflow Protocol Errors

These codes are used in JSON-RPC error responses between the orchestrator and component servers.

| Code | Name | Description | When to Use |
|------|------|-------------|-------------|
| -32000 | Server Error | Generic server error | Unspecified server-side error |
| -32001 | Component Not Found | Requested component does not exist | Unknown component name |
| -32002 | Server Not Initialized | Server has not been initialized | Missing initialization |
| -32003 | Invalid Input Schema | Input does not match component schema | Schema validation failure |
| -32004 | Component Execution Failed | Component failed during execution | Business logic error in component |
| -32005 | Resource Unavailable | Required resource is not available | External dependency failure |
| -32006 | Timeout | Operation timed out | Long-running operation timeout |
| -32007 | Permission Denied | Insufficient permissions | Authorization failure |
| -32008 | Blob Not Found | Requested blob does not exist | Invalid blob ID |
| -32009 | Expression Evaluation Failed | Flow expression could not be evaluated | Invalid reference or path |
| -32010 | Session Expired | HTTP session has expired | Session cleanup or timeout |
| -32011 | Invalid Value | Invalid value for a field | Value in a field is not valid |
| -32012 | Not Found | Referenced value is not found | Referenced entity does not exist |

### Orchestrator / Component Errors

These codes appear in `FlowResult` errors surfaced in run results. They are set by the orchestrator during expression evaluation or input resolution.

| Code | Name | Description |
|------|------|-------------|
| 1 | Undefined Field | A referenced field does not exist in the step output, input, or variable |
| 400 | Bad Request | Invalid input or structure |
| 404 | Entity Not Found | A referenced entity was not found (e.g., unknown step name) |
| 500 | Internal Error | Default code for component-reported failures or internal errors |

### Transport Errors

| Code | Name | Description |
|------|------|-------------|
| 5000+ | Transport Error | Infrastructure failure — subprocess crash, network timeout, connection refused |

## How Errors Flow

When a step executes, errors are classified based on how they originated:

### Component server errors

When a component server returns a JSON-RPC error response (e.g., the component raised an exception), the orchestrator converts it to a `FlowResult::Failed` with the **original error code** preserved. For example, if a Python component raises an exception, the SDK returns a `-32004` (Component Execution Failed) error, and the run result will show `code: -32004`.

### Transport errors

When the component server is unreachable — subprocess crash, network timeout, connection refused — the orchestrator sets the error code to `5000` (Transport Error). This indicates the component never ran or didn't complete.

### Orchestrator errors

When the orchestrator itself encounters an error during expression evaluation or input resolution (e.g., a `$step` reference to an unknown step, or a path that doesn't exist in the output), it creates an error with an orchestrator code (1–4999).

## Retry Behavior

The orchestrator uses error codes to make retry decisions:

- **Transport errors (code ≥ 5000)**: Retried up to `retry.transportMaxRetries` (default: 3). The plugin's `prepare_for_retry()` is called before each retry to allow resource recovery (e.g., restarting a crashed subprocess).
- **Component errors (code < 5000)**: Retried only if the step has `onError: { action: retry }`, up to `maxRetries` (default: 3).
- **Separate budgets**: Transport retries and component retries have independent counters. Exhausting one does not affect the other.

Transport errors and component errors share a single monotonically increasing `attempt` counter visible to the component. See [Component Execution](./methods/components.md#the-attempt-field) for details.

### Error handling actions

Steps can configure `onError` to control behavior on failure:

- **`fail`** (default): Stop workflow execution with the error
- **`useDefault`**: Use a `defaultValue` and mark the step as successful
- **`retry`**: Retry the step up to `maxRetries` times (for component errors only)

```yaml
steps:
  - id: flaky_step
    component: /external/api
    onError:
      action: retry
      maxRetries: 5
```

## Using ErrorCode in the Python SDK

The Python SDK provides an auto-generated `ErrorCode` enum with all well-known codes:

```python
from stepflow_py import ErrorCode

# Use in custom exceptions
from stepflow_py.worker.exceptions import StepflowError

raise StepflowError("Something went wrong", code=ErrorCode.RESOURCE_UNAVAILABLE)

# Check error classification
from stepflow_py.worker.exceptions import is_transport_error

is_transport_error(5000)   # True
is_transport_error(-32004) # False
```

Component servers can use any integer error code — the `ErrorCode` enum provides well-known constants but does not restrict the values.
