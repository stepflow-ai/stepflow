# stepflow-py

The unified Python SDK for [Stepflow](https://stepflow.org) - build and run AI workflows.

## Installation

```bash
pip install stepflow-py
```

### Optional Extras

| Extra | Install | Description |
|-------|---------|-------------|
| `runtime` | `pip install stepflow-py[runtime]` | Embedded Stepflow server (runs in-process, includes bundled binary) |
| `http` | `pip install stepflow-py[http]` | HTTP transport for component servers (FastAPI, uvicorn) |
| `langchain` | `pip install stepflow-py[langchain]` | LangChain runnable integration |
| `all` | `pip install stepflow-py[all]` | All of the above |

## Quick Start

### Run a Workflow

```python
from stepflow_py import StepflowClient

async with StepflowClient("http://localhost:7837") as client:
    result = await client.run("workflow.yaml", {"name": "World"})

    if result.is_success:
        print(result.output)
    else:
        print(f"Error: {result.error.message}")
```

### Build a Component

```python
from stepflow_py import StepflowServer, StepflowContext
import msgspec

class GreetInput(msgspec.Struct):
    name: str

class GreetOutput(msgspec.Struct):
    greeting: str

server = StepflowServer()

@server.component
def greet(input: GreetInput) -> GreetOutput:
    return GreetOutput(greeting=f"Hello, {input.name}!")

server.run()
```

### Embedded Runtime

Run workflows without a separate server process:

```python
from stepflow_py import StepflowRuntime  # requires stepflow-py[runtime]

async with StepflowRuntime.start("stepflow-config.yml") as runtime:
    result = await runtime.run("workflow.yaml", {"name": "World"})
    print(result.output)
```

## What's Included

This package bundles the Stepflow Python ecosystem:

| Import | Description |
|--------|-------------|
| `StepflowClient` | HTTP client for remote Stepflow servers |
| `StepflowRuntime` | Embedded runtime (requires `[runtime]` extra) |
| `StepflowServer` | SDK for building Python components |
| `StepflowContext` | Runtime context for components |
| `FlowResult` | Workflow execution result |
| `StepflowExecutor` | Protocol implemented by Client and Runtime |

## Interchangeable Backends

Both `StepflowClient` and `StepflowRuntime` implement `StepflowExecutor`:

```python
from stepflow_py import StepflowExecutor, FlowResult

async def run_workflow(executor: StepflowExecutor) -> FlowResult:
    return await executor.run("workflow.yaml", {"x": 1})

# Works with either:
await run_workflow(StepflowClient("http://localhost:7837"))
await run_workflow(StepflowRuntime.start())
```

## Individual Packages

For fine-grained dependency control, install individual packages:

- `stepflow-client` - HTTP client only
- `stepflow-runtime` - Embedded runtime only
- `stepflow-worker` - Component SDK only
- `stepflow-core` - Shared types only

## License

Apache-2.0
