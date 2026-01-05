# stepflow-core

Core types and interfaces for the Stepflow Python ecosystem.

## Installation

```bash
pip install stepflow-core
```

## Overview

This package provides common types used by:
- `stepflow-runtime`: Embedded Stepflow runtime that implements `StepflowExecutor`
- `stepflow-client`: Low-level HTTP client for direct Stepflow server API access
- `stepflow-server`: Component server SDK for building custom components

## Usage

Use `StepflowExecutor` as a type hint for code that works with workflow execution:

```python
from stepflow_core import FlowResult, StepflowExecutor

async def run_workflow(executor: StepflowExecutor) -> FlowResult:
    result = await executor.run("workflow.yaml", {"x": 1})
    if result.is_success:
        print(f"Output: {result.output}")
    return result

# Example with StepflowRuntime (implements StepflowExecutor)
from stepflow_runtime import StepflowRuntime

async with StepflowRuntime.start() as runtime:
    result = await run_workflow(runtime)
```

## Types

### Execution Results

- `FlowResult` - Result of a workflow execution (success/failed/skipped)
- `FlowResultStatus` - Enum: `SUCCESS`, `FAILED`, `SKIPPED`
- `FlowError` - Error details from a failed execution

```python
from stepflow_core import FlowResult, FlowResultStatus, FlowError

# Create results using factory methods
success = FlowResult.success({"answer": 42})
failed = FlowResult.failed(400, "Validation error", {"field": "input"})
skipped = FlowResult.skipped("Condition not met")

# Check result status
if result.is_success:
    print(result.output)
elif result.is_failed:
    print(f"Error {result.error.code}: {result.error.message}")
```

### Validation

- `ValidationResult` - Result of workflow validation with diagnostics
- `Diagnostic` - Single diagnostic message (error, warning, info)

```python
from stepflow_core import ValidationResult, Diagnostic

result = ValidationResult(
    valid=False,
    diagnostics=[
        Diagnostic(level="error", message="Missing field", location="$.input.name"),
        Diagnostic(level="warning", message="Deprecated step type"),
    ]
)

# Filter diagnostics
errors = result.errors
warnings = result.warnings
```

### Runtime Configuration

- `RestartPolicy` - Policy for restarting server subprocess (used by `StepflowRuntime`)
- `LogEntry` - Log entry from the stepflow server

```python
from stepflow_core import RestartPolicy

# Available policies
RestartPolicy.NEVER       # Never restart
RestartPolicy.ON_FAILURE  # Restart on non-zero exit
RestartPolicy.ALWAYS      # Always restart on exit
```

### Component Discovery

- `ComponentInfo` - Information about an available component

```python
from stepflow_core import ComponentInfo

info = ComponentInfo(
    path="/builtin/openai",
    description="OpenAI chat completion",
    input_schema={"type": "object", "properties": {...}},
    output_schema={"type": "object", "properties": {...}},
)
```

## Protocol

### StepflowExecutor

`StepflowExecutor` is a Protocol that defines the high-level workflow execution interface:

```python
from stepflow_core import StepflowExecutor

class StepflowExecutor(Protocol):
    @property
    def url(self) -> str: ...

    async def run(self, flow, input, overrides=None) -> FlowResult: ...
    async def submit(self, flow, input, overrides=None) -> str: ...
    async def get_result(self, run_id: str) -> FlowResult: ...
    async def validate(self, flow) -> ValidationResult: ...
    async def list_components(self) -> list[ComponentInfo]: ...
```

**Implementations:**
- `StepflowClient` (from `stepflow-client`) - HTTP client for remote servers
- `StepflowRuntime` (from `stepflow-runtime`) - Embedded server with subprocess management

Both implementations are interchangeable, allowing code to work with either backend.

## License

Apache-2.0
