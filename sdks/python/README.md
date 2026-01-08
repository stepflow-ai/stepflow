# Stepflow Python SDK

Python SDK for building Stepflow components and workflows.

## Installation

```bash
# Install from source
uv add stepflow-worker
```

## Configuration

The SDK is configured via environment variables:

| Variable | Description | Default |
|----------|-------------|---------|
| `STEPFLOW_SERVICE_NAME` | Service name for observability | `stepflow-workerthon` |
| `STEPFLOW_LOG_LEVEL` | Log level (DEBUG, INFO, WARNING, ERROR) | `INFO` |
| `STEPFLOW_LOG_DESTINATION` | Log destination (stderr, file, otlp, comma-separated) | `otlp` if OTLP endpoint set, else `stderr` |
| `STEPFLOW_LOG_FILE` | File path for file logging | - |
| `STEPFLOW_OTLP_ENDPOINT` | OTLP endpoint (e.g., http://localhost:4317) | - |
| `STEPFLOW_TRACE_ENABLED` | Enable distributed tracing | `true` |

**Example with OTLP (logs and traces go to OTLP by default):**
```bash
STEPFLOW_SERVICE_NAME=my-python-components \
STEPFLOW_OTLP_ENDPOINT=http://localhost:4317 \
uv run stepflow_worker
```

**Example with OTLP + stderr logging:**
```bash
STEPFLOW_SERVICE_NAME=my-components \
STEPFLOW_LOG_DESTINATION=stderr,otlp \
STEPFLOW_OTLP_ENDPOINT=http://localhost:4317 \
uv run stepflow_worker
```

**Example with only stderr logging (no OTLP):**
```bash
STEPFLOW_SERVICE_NAME=my-components \
STEPFLOW_LOG_LEVEL=DEBUG \
uv run stepflow_worker
```

## Usage

### Creating a Component Server

```python
from stepflow_worker import StepflowStdioServer, StepflowContext
import msgspec

# Define input/output types
class MyInput(msgspec.Struct):
    message: str
    count: int

class MyOutput(msgspec.Struct):
    result: str

# Create server
server = StepflowStdioServer()

# Register a component
@server.component
def my_component(input: MyInput) -> MyOutput:
    return MyOutput(result=f"Processed: {input.message} x{input.count}")

# Component with context (for blob operations)
@server.component
async def component_with_context(input: MyInput, context: StepflowContext) -> MyOutput:
    # Store data as a blob
    blob_id = await context.put_blob({"processed": input.message})
    return MyOutput(result=f"Stored blob: {blob_id}")

# Run the server
if __name__ == "__main__":
    server.run()
```

### Using the Context API

The `StepflowContext` provides bidirectional communication with the Stepflow runtime:

```python
# Store JSON data as a blob
blob_id = await context.put_blob({"key": "value"})

# Retrieve blob data
data = await context.get_blob(blob_id)

# Logging (uses Python's standard logging with automatic diagnostic context)
import logging
logger = logging.getLogger(__name__)
logger.info("Processing data")  # Automatically includes flow_id, run_id, step_id, etc.
```

## Development

### Running Tests

```bash
uv run pytest
```

### Type Checking

```bash
uv run mypy src/
```

### Protocol Generation

This SDK uses auto-generated protocol types from the JSON schema. To regenerate the protocol types when the schema changes:

```bash
uv run python generate.py
```

The generation script automatically handles the generation and applies necessary fixes for msgspec compatibility.

### Project Structure

- `src/stepflow_worker/` - Main SDK code
  - `generated_protocol.py` - Auto-generated protocol types from JSON schema
  - `protocol.py` - Hybrid protocol layer with envelope patterns for efficient deserialization
  - `server.py` - Component server implementation
  - `context.py` - Runtime context API
  - `exceptions.py` - SDK-specific exceptions
- `tests/` - Test suite

### Architecture

The SDK uses a hybrid approach for protocol handling:

1. **Generated Types** (`generated_protocol.py`) - Auto-generated from the Stepflow JSON schema using `datamodel-code-generator`
2. **Protocol Layer** (`protocol.py`) - Combines generated types with manual envelope patterns for two-stage deserialization using `msgspec.Raw`
3. **Server Implementation** - Uses the protocol layer for efficient JSON-RPC message handling

This design provides:
- **Type Safety** - All protocol messages use properly typed structs
- **Schema Consistency** - Generated types match the Rust protocol exactly
- **Performance** - Two-stage deserialization with `msgspec.Raw` for optimal speed
- **Maintainability** - Protocol changes can be regenerated automatically

## License

Licensed under the Apache License, Version 2.0.