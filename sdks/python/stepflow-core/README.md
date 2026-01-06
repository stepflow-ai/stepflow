# stepflow-core

Core types and interfaces for the Stepflow Python ecosystem.

## Installation

```bash
pip install stepflow-core
```

## Overview

This package defines the `StepflowExecutor` protocol and shared types used by:
- `stepflow-client`: HTTP client for remote Stepflow servers
- `stepflow-runtime`: Embedded Stepflow server with bundled binary

Both implementations are interchangeable, allowing code to work with either backend.

## StepflowExecutor Protocol

The `StepflowExecutor` protocol defines the high-level workflow execution interface:

```python
from stepflow_core import StepflowExecutor, FlowResult

async def run_workflow(executor: StepflowExecutor) -> FlowResult:
    # Works with either StepflowClient or StepflowRuntime
    return await executor.run("workflow.yaml", {"x": 1})
```

### run() - Execute and wait

Execute a workflow and wait for the result:

```python
result = await executor.run(
    flow="workflow.yaml",       # Path to workflow file or dict
    input={"x": 1, "y": 2},     # Input data
    overrides={"step1": {...}}  # Optional step overrides
)

if result.is_success:
    print(result.output)
elif result.is_failed:
    print(f"Error {result.error.code}: {result.error.message}")
elif result.is_skipped:
    print(f"Skipped: {result.skip_reason}")
```

### submit() - Execute without waiting

Submit a workflow for execution and return immediately:

```python
run_id = await executor.submit("workflow.yaml", {"x": 1})

# Poll for completion later
result = await executor.get_result(run_id)
```

### get_result() - Get execution result

Get the result of a previously submitted workflow:

```python
result = await executor.get_result(run_id)
```

### validate() - Validate workflow

Validate a workflow without executing it:

```python
validation = await executor.validate("workflow.yaml")

if not validation.valid:
    for diag in validation.errors:
        print(f"Error: {diag.message} at {diag.location}")
    for diag in validation.warnings:
        print(f"Warning: {diag.message}")
```

### list_components() - Discover components

List all available components:

```python
components = await executor.list_components()
for comp in components:
    print(f"{comp.path}: {comp.description}")
```

## Types

### FlowResult

Result of a workflow execution:

```python
from stepflow_core import FlowResult, FlowResultStatus

# Check status
if result.status == FlowResultStatus.SUCCESS:
    print(result.output)
elif result.status == FlowResultStatus.FAILED:
    print(f"Error {result.error.code}: {result.error.message}")
elif result.status == FlowResultStatus.SKIPPED:
    print(f"Skipped: {result.skip_reason}")

# Convenience properties
result.is_success   # True if SUCCESS
result.is_failed    # True if FAILED
result.is_skipped   # True if SKIPPED

# Factory methods
FlowResult.success({"answer": 42})
FlowResult.failed(400, "Validation error", {"field": "input"})
FlowResult.skipped("Condition not met")
```

### ValidationResult

Result of workflow validation:

```python
from stepflow_core import ValidationResult, Diagnostic

result.valid        # True if workflow is valid
result.diagnostics  # List of all diagnostics
result.errors       # Filtered list of error-level diagnostics
result.warnings     # Filtered list of warning-level diagnostics
```

### ComponentInfo

Information about an available component:

```python
from stepflow_core import ComponentInfo

comp.path           # e.g., "/builtin/openai"
comp.description    # Human-readable description
comp.input_schema   # JSON schema for input
comp.output_schema  # JSON schema for output
```

### FlowError

Error details from a failed execution:

```python
from stepflow_core import FlowError

error.code      # Numeric error code
error.message   # Human-readable message
error.details   # Optional additional details (dict)
```

## Implementations

| Package | Use case |
|---------|----------|
| `stepflow-client` | Connect to a remote Stepflow server |
| `stepflow-runtime` | Run an embedded server locally |

```python
from stepflow_client import StepflowClient
from stepflow_runtime import StepflowRuntime

# Remote server
async with StepflowClient("http://localhost:7837") as client:
    result = await client.run("workflow.yaml", {"x": 1})

# Embedded server
async with StepflowRuntime.start("config.yml") as runtime:
    result = await runtime.run("workflow.yaml", {"x": 1})
```

## License

Apache-2.0
