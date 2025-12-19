# stepflow-server

Python SDK for creating Stepflow component servers.

## Installation

```bash
pip install stepflow-server
```

For HTTP server support:
```bash
pip install stepflow-server[http]
```

For LangChain integration:
```bash
pip install stepflow-server[langchain]
```

## Overview

This package provides the Python SDK for creating custom Stepflow component servers. Component servers expose user-defined Python functions as workflow components that can be orchestrated by the Stepflow runtime.

## Quick Start

### Creating Components

```python
from stepflow_server import StepflowServer
import msgspec

class AddInput(msgspec.Struct):
    a: int
    b: int

class AddOutput(msgspec.Struct):
    result: int

server = StepflowServer()

@server.component
def add(input: AddInput) -> AddOutput:
    return AddOutput(result=input.a + input.b)

# Run as stdio server (default)
server.start_stdio()
```

### Components with Context

Components can access runtime services through the `StepflowContext`:

```python
from stepflow_server import StepflowServer, StepflowContext

server = StepflowServer()

@server.component
async def process_data(input: MyInput, context: StepflowContext) -> MyOutput:
    # Store data as a blob
    blob_id = await context.put_blob({"processed": input.data})

    # Get metadata about the current execution
    metadata = await context.get_metadata()

    return MyOutput(blob_id=blob_id, run_id=context.run_id)
```

### HTTP Server Mode

For production deployments, you can run the server in HTTP mode:

```bash
# Install HTTP dependencies
pip install stepflow-server[http]

# Run in HTTP mode
stepflow_server --http --port 8080
```

Or programmatically:

```python
import asyncio

async def main():
    await server.start_http(host="0.0.0.0", port=8080)

asyncio.run(main())
```

## Building Workflows

Create workflows programmatically using the `FlowBuilder`:

```python
from stepflow_server import FlowBuilder, Value

builder = FlowBuilder(name="my-workflow")

# Add steps
step1 = builder.add_step(
    id="step1",
    component="/my_component",
    input_data={
        "message": Value.input().message,
        "config": Value.literal({"temperature": 0.7}),
    }
)

# Reference previous step output
step2 = builder.add_step(
    id="step2",
    component="/process",
    input_data={
        "data": Value.step("step1", "$.result"),
    }
)

# Set output
builder.set_output({"result": Value.step("step2")})

flow = builder.build()
```

## LangChain Integration

Register LangChain runnables as components:

```python
from stepflow_server import StepflowServer
from langchain_core.runnables import RunnableLambda

server = StepflowServer()

@server.langchain_component(name="/my_chain")
def create_my_chain():
    return RunnableLambda(lambda x: f"Processed: {x}")
```

## API Reference

### StepflowServer

The main server class for registering and running components.

- `component(func, *, name=None, description=None)` - Decorator to register a component
- `langchain_component(func, *, name=None, description=None, execution_mode="invoke")` - Decorator for LangChain runnables
- `start_stdio(stdin=None, stdout=None)` - Start STDIO server
- `start_http(host="localhost", port=8080)` - Start HTTP server

### StepflowContext

Runtime context available to components.

- `put_blob(data, blob_type)` - Store data as a blob
- `get_blob(blob_id)` - Retrieve blob data
- `evaluate_flow(flow, input)` - Execute a sub-flow
- `get_metadata(step_id=None)` - Get execution metadata
- `submit_batch(flow, inputs)` - Submit batch execution
- Properties: `run_id`, `flow_id`, `step_id`, `attempt`

### FlowBuilder

Build workflows programmatically.

- `add_step(id, component, input_data, ...)` - Add a workflow step
- `set_output(output_data)` - Set flow output
- `build()` - Build the Flow object

### Value

Create workflow value references.

- `Value.input()` - Reference workflow input
- `Value.step(step_id, path)` - Reference step output
- `Value.literal(value)` - Escape literal values

## Migration from stepflow-py

This package was renamed from `stepflow-py`. To migrate:

1. Replace `pip install stepflow-py` with `pip install stepflow-server`
2. Update imports from `stepflow_py` to `stepflow_server`
3. Update entry point from `stepflow_py` to `stepflow_server`

## License

Apache-2.0
