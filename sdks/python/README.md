# StepFlow Python SDK

Python SDK for building StepFlow components and workflows.

## Installation

```bash
# Install from source
uv add stepflow-py
```

## Usage

### Creating a Component Server

```python
from stepflow_py import StepflowStdioServer, StepflowContext
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

The `StepflowContext` provides bidirectional communication with the StepFlow runtime:

```python
# Store JSON data as a blob
blob_id = await context.put_blob({"key": "value"})

# Retrieve blob data
data = await context.get_blob(blob_id)

# Logging
context.log("Debug message")
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

- `src/stepflow_py/` - Main SDK code
  - `generated_protocol.py` - Auto-generated protocol types from JSON schema
  - `protocol.py` - Hybrid protocol layer with envelope patterns for efficient deserialization
  - `server.py` - Component server implementation
  - `context.py` - Runtime context API
  - `exceptions.py` - SDK-specific exceptions
- `tests/` - Test suite

### Architecture

The SDK uses a hybrid approach for protocol handling:

1. **Generated Types** (`generated_protocol.py`) - Auto-generated from the StepFlow JSON schema using `datamodel-code-generator`
2. **Protocol Layer** (`protocol.py`) - Combines generated types with manual envelope patterns for two-stage deserialization using `msgspec.Raw`
3. **Server Implementation** - Uses the protocol layer for efficient JSON-RPC message handling

This design provides:
- **Type Safety** - All protocol messages use properly typed structs
- **Schema Consistency** - Generated types match the Rust protocol exactly
- **Performance** - Two-stage deserialization with `msgspec.Raw` for optimal speed
- **Maintainability** - Protocol changes can be regenerated automatically

## License

Licensed under the Apache License, Version 2.0.