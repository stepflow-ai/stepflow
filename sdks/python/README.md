# Stepflow Python SDK

Python packages for building and running Stepflow workflows.

## How the Packages Fit Together

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Your Application                            │
└─────────────────────────────────────────────────────────────────────┘
                                   │
                    ┌──────────────┴──────────────┐
                    ▼                             ▼
        ┌───────────────────┐         ┌───────────────────┐
        │  stepflow-client  │         │ stepflow-runtime  │
        │  (remote server)  │         │ (embedded server) │
        └───────────────────┘         └───────────────────┘
                    │                             │
                    └──────────────┬──────────────┘
                                   ▼
                        ┌───────────────────┐
                        │   stepflow-core   │
                        │ (shared types &   │
                        │  StepflowExecutor │
                        │    protocol)      │
                        └───────────────────┘

        ┌───────────────────┐
        │  stepflow-server  │  ◄── Build custom components
        │ (component SDK)   │
        └───────────────────┘
```

| Package | What it's for |
|---------|---------------|
| **stepflow-server** | Build custom Python components that workflows can call |
| **stepflow-client** | Connect to a remote Stepflow server |
| **stepflow-runtime** | Run an embedded Stepflow server in your Python process |
| **stepflow-core** | Shared types (`FlowResult`, `StepflowExecutor` protocol) |
| **stepflow-api** | Generated HTTP client (used internally) |

## Quick Start: End-to-End Example

This example shows the complete flow: building a component, writing a workflow, and executing it.

### 1. Build a Custom Component

```bash
pip install stepflow-server
```

Create `my_components.py`:

```python
from stepflow_server import StepflowStdioServer
import msgspec

class GreetInput(msgspec.Struct):
    name: str

class GreetOutput(msgspec.Struct):
    greeting: str

server = StepflowStdioServer()

@server.component
def greet(input: GreetInput) -> GreetOutput:
    return GreetOutput(greeting=f"Hello, {input.name}!")

if __name__ == "__main__":
    server.run()
```

> **Note:** You don't need to run the component server manually. Stepflow launches it automatically based on your configuration (see step 2).

### 2. Configure Stepflow to Use Your Component

Create `stepflow-config.yml`:

```yaml
plugins:
  builtin:
    type: builtin

  python:
    type: stepflow
    transport: stdio
    command: python
    args: ["my_components.py"]

routes:
  "/python/{*component}":
    - plugin: python
  "/{*component}":
    - plugin: builtin
```

### 3. Write a Workflow

Create `hello-workflow.yaml`:

```yaml
schema: "flow/v1"
name: hello-world

input:
  type: object
  properties:
    name:
      type: string
  required: [name]

steps:
  greet:
    component: /python/greet
    input:
      name: { $input: "name" }

output:
  message: { $step: "greet", path: "greeting" }
```

### 4. Execute the Workflow

**Option A: Embedded runtime (no separate server)**

```bash
pip install stepflow-runtime
```

```python
import asyncio
from stepflow_runtime import StepflowRuntime

async def main():
    async with StepflowRuntime.start("stepflow-config.yml") as runtime:
        result = await runtime.run("hello-workflow.yaml", {"name": "World"})

        if result.is_success:
            print(result.output)  # {"message": "Hello, World!"}
        else:
            print(f"Error: {result.error.message}")

asyncio.run(main())
```

**Option B: Remote server**

```bash
pip install stepflow-client
```

```bash
# Start the server (in another terminal)
stepflow-server --config stepflow-config.yml
```

```python
import asyncio
from stepflow_client import StepflowClient

async def main():
    async with StepflowClient("http://localhost:7837") as client:
        result = await client.run("hello-workflow.yaml", {"name": "World"})

        if result.is_success:
            print(result.output)  # {"message": "Hello, World!"}
        else:
            print(f"Error: {result.error.message}")

asyncio.run(main())
```

## Choosing Between Client and Runtime

Both `StepflowClient` and `StepflowRuntime` implement the `StepflowExecutor` protocol, so they're interchangeable:

```python
from stepflow_core import StepflowExecutor, FlowResult

async def run_workflow(executor: StepflowExecutor, input: dict) -> FlowResult:
    # Works with either StepflowClient or StepflowRuntime
    return await executor.run("workflow.yaml", input)
```

**Use `StepflowRuntime` when:**
- Running locally for development
- Embedding Stepflow in a Python application
- You don't want to manage a separate server process

**Use `StepflowClient` when:**
- Connecting to a shared/production server
- Running in a distributed environment
- The server is already running

## StepflowExecutor API

Both `StepflowClient` and `StepflowRuntime` provide these methods:

```python
# Execute and wait for result
result = await executor.run(flow, input, overrides=None)

# Submit without waiting (returns run_id)
run_id = await executor.submit(flow, input, overrides=None)

# Get result of a submitted run
result = await executor.get_result(run_id)

# Validate a workflow
validation = await executor.validate(flow)

# List available components
components = await executor.list_components()
```

## Building Custom Components

See the [stepflow-server README](stepflow-server/README.md) for detailed documentation on:
- Defining component input/output schemas with msgspec
- Async components with `StepflowContext` for blob storage
- HTTP transport for production deployments

## Development

```bash
cd sdks/python
uv sync

uv run poe test       # Run tests
uv run poe fmt        # Format code
uv run poe lint       # Lint code
uv run poe typecheck  # Type check
```

## License

Apache-2.0
