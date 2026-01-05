# Stepflow Python SDK

Python SDK for building Stepflow components and workflows.

## Packages

This workspace contains multiple packages:

| Package | Description |
|---------|-------------|
| **stepflow-core** | Core types and interfaces (`FlowResult`, `StepflowExecutor` protocol) |
| **stepflow-api** | Generated API client models from OpenAPI spec |
| **stepflow-client** | HTTP client for remote Stepflow servers |
| **stepflow-runtime** | Embedded runtime with bundled server binary |
| **stepflow-server** | Component server SDK for building custom components |

## Installation

```bash
# For building custom components
pip install stepflow-server

# For remote execution
pip install stepflow-client

# For embedded local execution
pip install stepflow-runtime
```

## Quick Start

### Building Components

```python
from stepflow_server import StepflowStdioServer, StepflowContext
import msgspec

class MyInput(msgspec.Struct):
    message: str
    count: int

class MyOutput(msgspec.Struct):
    result: str

server = StepflowStdioServer()

@server.component
def my_component(input: MyInput) -> MyOutput:
    return MyOutput(result=f"Processed: {input.message} x{input.count}")

@server.component
async def component_with_context(input: MyInput, context: StepflowContext) -> MyOutput:
    blob_id = await context.put_blob({"processed": input.message})
    return MyOutput(result=f"Stored blob: {blob_id}")

if __name__ == "__main__":
    server.run()
```

### Remote Execution

```python
from stepflow_client import StepflowClient

async def main():
    client = StepflowClient("http://localhost:7837")
    result = await client.run("workflow.yaml", {"input": "value"})
    print(result)
```

### Embedded Runtime

```python
from stepflow_runtime import StepflowRuntime

async def main():
    async with StepflowRuntime.start("stepflow-config.yml") as runtime:
        result = await runtime.run("workflow.yaml", {"input": "value"})
        print(result)
```

## Configuration

The SDK is configured via environment variables:

| Variable | Description | Default |
|----------|-------------|---------|
| `STEPFLOW_SERVICE_NAME` | Service name for observability | `stepflow-python` |
| `STEPFLOW_LOG_LEVEL` | Log level (DEBUG, INFO, WARNING, ERROR) | `INFO` |
| `STEPFLOW_LOG_DESTINATION` | Log destination (stderr, file, otlp) | `otlp` if endpoint set |
| `STEPFLOW_OTLP_ENDPOINT` | OTLP endpoint for traces/logs | - |
| `STEPFLOW_TRACE_ENABLED` | Enable distributed tracing | `true` |

## Development

### Setup

```bash
cd sdks/python
uv sync
```

### Available Tasks

```bash
uv run poe test       # Run all tests
uv run poe fmt        # Format code
uv run poe lint       # Lint and fix code
uv run poe check      # Check formatting and linting (no fixes)
uv run poe typecheck  # Run type checking
uv run poe codegen    # Regenerate protocol types
uv run poe api-gen    # Regenerate API client from OpenAPI
uv run poe api-check  # Check if API client is up-to-date
uv run poe api-update # Update OpenAPI spec from server and regenerate
```

## Code Generation

This workspace uses code generation for two purposes. Both depend on JSON schema files in `../../schemas/` which are generated from the Rust codebase.

### Schema Sources

The JSON schemas are **not hand-written** - they're generated from Rust types:

| Schema | Generated From | How to Update |
|--------|----------------|---------------|
| `flow.json`, `protocol.json` | Rust types in `stepflow-protocol` | `STEPFLOW_OVERWRITE_SCHEMA=1 cargo test -p stepflow-protocol` |
| `openapi.json` | Running stepflow-server | `uv run poe api-gen --update-spec` |

See [stepflow-rs/CLAUDE.md](../../stepflow-rs/CLAUDE.md#schema-generation) for details on Rust schema generation.

### Protocol Types (`stepflow-server`)

Protocol types (JSON-RPC messages, flow definitions) are generated from JSON schemas:

```bash
uv run poe codegen
```

Source: `../../schemas/flow.json`, `../../schemas/protocol.json` → Target: `stepflow-server/src/stepflow_server/generated_*.py`

### API Client (`stepflow-api`)

API client models are generated from the OpenAPI specification:

```bash
# Regenerate from stored spec
uv run poe api-gen

# Update spec from server AND regenerate (builds and runs Rust server temporarily)
uv run poe api-update

# Check if models are up-to-date (CI)
uv run poe api-check
```

Source: `../../schemas/openapi.json` → Target: `stepflow-api/src/stepflow_api/models/`

See [stepflow-api README](stepflow-api/README.md) for details on the generation process and custom overrides.

## License

Licensed under the Apache License, Version 2.0.
