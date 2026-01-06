# Stepflow Python SDK

Python SDK for building and running Stepflow workflows.

## Installation

```bash
pip install stepflow-py
```

For embedded runtime (runs Stepflow server in-process):
```bash
pip install stepflow-py[runtime]
```

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
from stepflow_py import StepflowServer
import msgspec

class GreetInput(msgspec.Struct):
    name: str

class GreetOutput(msgspec.Struct):
    greeting: str

server = StepflowServer()

@server.component
def greet(input: GreetInput) -> GreetOutput:
    return GreetOutput(greeting=f"Hello, {input.name}!")

server.run()  # Stepflow launches this automatically via config
```

### Embedded Runtime

Run workflows without a separate server:

```python
from stepflow_py import StepflowRuntime  # requires stepflow-py[runtime]

async with StepflowRuntime.start("stepflow-config.yml") as runtime:
    result = await runtime.run("workflow.yaml", {"name": "World"})
    print(result.output)
```

## End-to-End Example

### 1. Create a Component

```bash
pip install stepflow-py
```

Create `my_components.py`:

```python
from stepflow_py import StepflowServer
import msgspec

class GreetInput(msgspec.Struct):
    name: str

class GreetOutput(msgspec.Struct):
    greeting: str

server = StepflowServer()

@server.component
def greet(input: GreetInput) -> GreetOutput:
    return GreetOutput(greeting=f"Hello, {input.name}!")

if __name__ == "__main__":
    server.run()
```

### 2. Configure Stepflow

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

Create `hello.yaml`:

```yaml
schema: "flow/v1"
name: hello-world

input:
  type: object
  properties:
    name: { type: string }
  required: [name]

steps:
  greet:
    component: /python/greet
    input:
      name: { $input: "name" }

output:
  message: { $step: "greet", path: "greeting" }
```

### 4. Run It

```bash
pip install stepflow-py[runtime]
```

```python
import asyncio
from stepflow_py import StepflowRuntime

async def main():
    async with StepflowRuntime.start("stepflow-config.yml") as runtime:
        result = await runtime.run("hello.yaml", {"name": "World"})
        print(result.output)  # {"message": "Hello, World!"}

asyncio.run(main())
```

## Package Structure

```
┌─────────────────────────────────────────────────────────────────────┐
│                          stepflow-py                                │
│                    (unified SDK - install this)                     │
└─────────────────────────────────────────────────────────────────────┘
                                   │
          ┌────────────────────────┼────────────────────────┐
          ▼                        ▼                        ▼
┌───────────────────┐  ┌───────────────────┐  ┌───────────────────┐
│  stepflow-client  │  │ stepflow-runtime  │  │  stepflow-server  │
│  (remote server)  │  │(embedded runtime) │  │  (component SDK)  │
└───────────────────┘  └───────────────────┘  └───────────────────┘
          │                        │                        │
          ▼                        └────────────┬───────────┘
┌───────────────────┐                           ▼
│   stepflow-api    │               ┌───────────────────┐
│ (generated HTTP)  │               │   stepflow-core   │
└───────────────────┘               │  (shared types)   │
                                    └───────────────────┘
```

| Package | Description |
|---------|-------------|
| **stepflow-py** | Unified SDK (install this) |
| **stepflow-client** | HTTP client for remote servers |
| **stepflow-runtime** | Embedded runtime with bundled server |
| **stepflow-server** | SDK for building Python components |
| **stepflow-core** | Shared types (`FlowResult`, `StepflowExecutor`) |
| **stepflow-api** | Generated HTTP client (internal) |

## API Reference

### StepflowExecutor Protocol

Both `StepflowClient` and `StepflowRuntime` implement this protocol:

```python
from stepflow_py import StepflowExecutor

async def run_workflow(executor: StepflowExecutor):
    # Execute and wait
    result = await executor.run(flow, input, overrides=None)

    # Submit without waiting
    run_id = await executor.submit(flow, input, overrides=None)

    # Get result later
    result = await executor.get_result(run_id)

    # Validate workflow
    validation = await executor.validate(flow)

    # List components
    components = await executor.list_components()
```

### Component Development

See the [stepflow-server README](stepflow-server/README.md) for:
- Input/output schemas with msgspec
- Async components with `StepflowContext`
- HTTP transport for production

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
