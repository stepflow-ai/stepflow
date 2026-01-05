# stepflow-client

HTTP client for Stepflow servers.

## Installation

```bash
pip install stepflow-client
```

## Overview

This package provides an async HTTP client for communicating with remote Stepflow servers. It implements the `StepflowExecutor` protocol, allowing it to be used interchangeably with `StepflowRuntime`.

## Usage

### Basic Usage

```python
from stepflow_client import StepflowClient

async with StepflowClient("http://localhost:7837") as client:
    # Run a workflow and wait for result
    result = await client.run("workflow.yaml", {"x": 1, "y": 2})

    if result.is_success:
        print(f"Output: {result.output}")
    elif result.is_failed:
        print(f"Error: {result.error.message}")
```

### Submit and Poll

```python
# Submit workflow (returns immediately)
run_id = await client.submit("workflow.yaml", {"x": 1})

# Get result later
result = await client.get_result(run_id)
```

### Batch Execution

```python
# Submit multiple runs
inputs = [{"x": 1}, {"x": 2}, {"x": 3}]
batch_id = await client.submit_batch("workflow.yaml", inputs, max_concurrent=2)

# Get batch results
details, results = await client.get_batch(batch_id)
for result in results:
    print(result.output)
```

### Validation

```python
validation = await client.validate("workflow.yaml")
if not validation.valid:
    for error in validation.errors:
        print(f"Error: {error.message}")
```

### List Components

```python
components = await client.list_components()
for comp in components:
    print(f"{comp.path}: {comp.description}")
```

## Interchangeable with StepflowRuntime

```python
from stepflow_core import StepflowExecutor

def get_executor(local: bool = True) -> StepflowExecutor:
    if local:
        from stepflow_runtime import StepflowRuntime
        return StepflowRuntime.start("config.yml")
    from stepflow_client import StepflowClient
    return StepflowClient("http://production:7837")

executor = get_executor(local=False)
result = await executor.run("workflow.yaml", input_data)
```

## License

Apache-2.0
