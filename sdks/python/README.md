# Stepflow Python SDK

Python SDK for building Stepflow components and workflows.

## Packages

This workspace contains multiple packages:

| Package | Description |
|---------|-------------|
| **stepflow** | Shared types and interfaces (`FlowResult`, `StepflowExecutor` protocol) |
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

### Running Tests

```bash
uv run poe test
```

### Type Checking

```bash
uv run poe typecheck
```

### Linting

```bash
uv run poe lint
```

### Protocol Generation

Regenerate protocol types when the schema changes:

```bash
uv run python generate.py
```

## License

Licensed under the Apache License, Version 2.0.
