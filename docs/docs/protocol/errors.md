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

## Standard Error Codes

The protocol defines error codes in standardized ranges following JSON-RPC conventions:

### JSON-RPC Standard Errors (-32768 to -32000)

| Code | Name | Description | When to Use |
|------|------|-------------|-------------|
| -32700 | Parse Error | Invalid JSON was received | Malformed JSON in request |
| -32600 | Invalid Request | The JSON sent is not a valid Request object | Missing required fields, invalid structure |
| -32601 | Method Not Found | The method does not exist / is not available | Unknown method name |
| -32602 | Invalid Params | Invalid method parameter(s) | Schema validation failure, wrong parameter types |
| -32603 | Internal Error | Internal JSON-RPC error | Server implementation error |

### Stepflow Protocol Errors (-32099 to -32000)

| Code | Name | Description | When to Use |
|------|------|-------------|-------------|
| -32000 | Server Error | Generic server error | Unspecified server-side error |
| -32001 | Component Not Found | Requested component does not exist | Unknown component name |
| -32002 | Server Not Initialized | Server has not been initialized | Missing initialization |
| -32003 | Invalid Input Schema | Input does not match component schema | Schema validation failure |
| -32004 | Component Execution Failed | Component failed during execution | Business logic error |
| -32005 | Resource Unavailable | Required resource is not available | External dependency failure |
| -32006 | Timeout | Operation timed out | Long-running operation timeout |
| -32007 | Permission Denied | Insufficient permissions | Authorization failure |
| -32008 | Blob Not Found | Requested blob does not exist | Invalid blob ID |
| -32009 | Expression Evaluation Failed | Flow expression could not be evaluated | Invalid reference or path |
| -32010 | Session Expired | HTTP session has expired | Session cleanup or timeout |
| -32011 | Invalid Step ID | Provided step ID does not exist | Unknown step ID in a request |