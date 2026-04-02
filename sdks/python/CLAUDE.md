# Python SDK Development Guide

This guide covers Python SDK-specific development for the Stepflow project.

See the root `/CLAUDE.md` for project overview, configuration, and workflow syntax.

## Package Structure

The Python SDK is organized into two packages:

- **stepflow-py**: Main SDK with gRPC client and worker modules
  - `stepflow_py.client`: gRPC client for Stepflow orchestrator
  - `stepflow_py.worker`: Component server for Python components
  - `stepflow_py.proto`: Generated protobuf/gRPC stubs
- **stepflow-orchestrator**: Platform-specific wheels bundling the stepflow-server binary

## Development Commands

### Testing

```bash
# Test Python SDK with current Python version
cd sdks/python
uv run poe test

# Test across all supported versions (3.10, 3.11, 3.12, 3.13)
./scripts/test-python-versions.sh
```

### Running the SDK

```bash
# Run in gRPC mode (default)
uv run --project sdks/python/stepflow-py stepflow_py

# Run in NATS mode
uv run --project sdks/python/stepflow-py stepflow_py --nats
```

### Type Generation

```bash
# Regenerate Python config/flow types from updated schemas
cd sdks/python
uv run python generate.py

# Check if Python types are up to date without regenerating
uv run python generate.py --check
```

**Note**: Config and flow types (msgspec-based) are generated from JSON schemas. Protocol types are defined by `.proto` files and generated via `generate-python-proto.sh`.

**Important**: Run type generation after Rust schema updates (see `stepflow-rs/CLAUDE.md` for schema generation commands).

### Proto / gRPC Stub Generation

Proto files live in `stepflow-rs/proto/stepflow/v1/`. Generated Python stubs live in `stepflow-py/src/stepflow_py/proto/`.

```bash
# Regenerate Python gRPC stubs after modifying any .proto file
./scripts/generate-python-proto.sh
```

**Important**: Always regenerate after modifying `.proto` files. The script cleans the output directory, runs `grpc_tools.protoc`, fixes imports for the flat package layout, and writes the `__init__.py` with public re-exports.

## Python Version Compatibility

The Python SDK supports Python 3.10, 3.11, 3.12, and 3.13. All versions are tested in CI.

### Testing a Specific Python Version

```bash
cd sdks/python

# Install specific Python version
uv python install 3.10  # or 3.11, 3.12, 3.13

# Pin the version for testing
uv python pin 3.10

# Install dependencies and run tests
uv sync --project stepflow-py
uv run poe test
uv run poe typecheck
```

## Transport Modes

The Python SDK supports two transport methods for communication with the Stepflow runtime.

### gRPC Transport (Default)

Pull-based gRPC transport where workers pull tasks from the orchestrator's TasksService.

**Usage**:
```bash
# Run in gRPC mode (default)
uv run --project sdks/python/stepflow-py stepflow_py

# With custom tasks URL
uv run --project sdks/python/stepflow-py stepflow_py --tasks-url localhost:7837
```

**Configuration** (in stepflow-config.yml):
```yaml
plugins:
  python:
    type: grpc
    command: uv
    args: ["--project", "../sdks/python/stepflow-py", "run", "stepflow_py"]

routes:
  "/python":
    - plugin: python
```

### NATS Transport

NATS JetStream-based transport for distributed deployments.

**Usage**:
```bash
# Run in NATS mode
uv run --project sdks/python/stepflow-py stepflow_py --nats

# With custom NATS URL
uv run --project sdks/python/stepflow-py stepflow_py --nats --nats-url nats://localhost:4222
```

## Component Development

### Routing: prefix + subpath

Component names are **subpaths relative to the worker's route prefix** in the orchestrator config. The full workflow path is the prefix + subpath.

```
Orchestrator config         Component subpath              Full workflow path
───────────────────         ─────────────────              ──────────────────
routes:                     @server.component              component: /python/my_func
  "/python":                def my_func(...): ...
    - plugin: python
```

Do **not** include the prefix in component names — the orchestrator adds it. A component registered as `my_func` (subpath `/my_func`) on a worker at prefix `/python` is referenced in workflows as `/python/my_func`.

### Basic Component Registration

```python
from stepflow_py import StepflowServer
import msgspec

class MyInput(msgspec.Struct):
    message: str
    count: int

class MyOutput(msgspec.Struct):
    result: str

server = StepflowServer()

@server.component
def my_component(input: MyInput) -> MyOutput:
    result = input.message * input.count
    return MyOutput(result=result)

```

### Using StepflowContext for Bidirectional Communication

Components can receive a `StepflowContext` parameter to access runtime operations:

```python
from stepflow_py import StepflowServer, StepflowContext
import msgspec

class MyInput(msgspec.Struct):
    data: dict

class MyOutput(msgspec.Struct):
    blob_id: str

server = StepflowServer()

@server.component
async def my_component(input: MyInput, context: StepflowContext) -> MyOutput:
    # Store data as a blob
    blob_id = await context.put_blob(input.data)

    # Retrieve blob data
    retrieved_data = await context.get_blob(blob_id)

    return MyOutput(blob_id=blob_id)

```

**Available Context Methods**:
- `put_blob(data: dict) -> str`: Store JSON data as blob, returns content-based ID
- `get_blob(blob_id: str) -> dict`: Retrieve JSON data by blob ID

## Using the FlowBuilder

The `stepflow_py.worker` module provides a `FlowBuilder` for programmatically creating flows:

```python
from stepflow_py.worker import FlowBuilder, Value

# Create a flow programmatically
builder = FlowBuilder(name="my_flow")

# Add steps
builder.add_step(
    id="greet",
    component="/builtin/eval",
    input_data={"code": Value.literal("return f'Hello, {input.name}!'")}
)

# Set output
builder.set_output(Value.step("greet"))

# Build the flow
flow = builder.build()
```

## Architecture Notes

1. **Component Registry**: `StepflowServer` manages component registration and lookup
2. **Context Hierarchy**: `StepflowContext` is the base class; `GrpcContext` provides gRPC transport
3. **Task Handler**: Shared task execution logic used by both gRPC and NATS workers
4. **msgspec Flow Types**: Flow types use msgspec Structs generated from `schemas/flow.json`
5. **Unified Package**: gRPC client and worker functionality in single `stepflow-py` package

## Testing Best Practices

- Use type checking with `poe typecheck` to catch type errors early
- Run tests across all supported Python versions before submitting changes
- Mock `StepflowContext` for unit testing components without runtime dependencies
