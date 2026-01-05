# stepflow-core

Core types and interfaces for the Stepflow Python ecosystem.

## Installation

```bash
pip install stepflow-core
```

## Overview

This package provides common types used by:
- `stepflow-client`: HTTP client for remote Stepflow servers
- `stepflow-runtime`: Embedded Stepflow runtime with bundled server
- `stepflow-server`: Component server SDK for building custom components

## Usage

```python
from stepflow_core import FlowResult, StepflowExecutor, RestartPolicy

# Use StepflowExecutor as a type hint for interchangeable backends
async def run_workflow(executor: StepflowExecutor) -> FlowResult:
    result = await executor.run("workflow.yaml", {"x": 1})
    if result.is_success:
        print(f"Output: {result.output}")
    return result
```

## Types

- `FlowResult` - Result of a workflow execution (success/failed/skipped)
- `ValidationResult` - Result of workflow validation with diagnostics
- `LogEntry` - Log entry from the stepflow server
- `RestartPolicy` - Policy for restarting the server subprocess
- `ComponentInfo` - Information about an available component

## Protocol

- `StepflowExecutor` - Protocol implemented by both `StepflowClient` and `StepflowRuntime`

## License

Apache-2.0
