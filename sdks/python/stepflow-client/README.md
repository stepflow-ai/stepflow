# stepflow-client

HTTP client for Stepflow servers, implementing the `StepflowExecutor` protocol.

## Installation

```bash
pip install stepflow-client
```

## Overview

This package provides an async HTTP client that implements the `StepflowExecutor` protocol, enabling interoperability with `StepflowRuntime`. Use either backend interchangeably:

```python
from stepflow_core import StepflowExecutor

async def run_workflow(executor: StepflowExecutor):
    result = await executor.run("workflow.yaml", {"x": 1})
    return result
```

## Quick Start

```python
from stepflow_client import StepflowClient

async with StepflowClient("http://localhost:7837") as client:
    # Run a workflow and get the result
    result = await client.run("workflow.yaml", {"message": "hello"})

    if result.is_success:
        print(f"Output: {result.output}")
    else:
        print(f"Error: {result.error.message}")
```

## StepflowExecutor Protocol

`StepflowClient` implements the `StepflowExecutor` protocol, providing these high-level methods:

### run() - Execute and wait

```python
result = await client.run(
    flow="workflow.yaml",       # Path to workflow file or dict
    input={"x": 1, "y": 2},     # Input data
    overrides={"step1": {...}}  # Optional step overrides
)

if result.is_success:
    print(result.output)
elif result.is_failed:
    print(f"Error {result.error.code}: {result.error.message}")
```

### submit() - Execute without waiting

```python
# Submit and get a run ID immediately
run_id = await client.submit("workflow.yaml", {"x": 1})

# Check result later
result = await client.get_result(run_id)
```

### validate() - Validate workflow

```python
validation = await client.validate("workflow.yaml")
if not validation.valid:
    for diag in validation.errors:
        print(f"Error: {diag.message} at {diag.location}")
```

### list_components() - Discover components

```python
components = await client.list_components()
for comp in components:
    print(f"{comp.path}: {comp.description}")
```

## Low-Level API

For fine-grained control, use the lower-level methods that map directly to API endpoints:

### Flow Management

```python
# Store a flow from file or dict
response = await client.store_flow("workflow.yaml")
response = await client.store_flow({"schema": "...", "steps": [...]})
flow_id = response.flow_id

# Get a stored flow
flow = await client.get_flow(flow_id)

# Delete a stored flow
await client.delete_flow(flow_id)
```

### Run Execution

```python
# Create and execute a run
run = await client.create_run(
    flow_id=flow_id,
    input={"x": 1, "y": 2},
    variables={"api_key": "..."},  # Optional workflow variables
    debug=False,                    # Optional debug mode
)

# Get run details
details = await client.get_run(str(run.run_id))
print(f"Status: {details.status}")  # running, completed, failed, cancelled

# Get execution results (for batch runs)
items = await client.get_run_items(str(run.run_id))
for item in items.items:
    print(f"Item {item.index}: {item.result}")

# Get step-by-step execution details
steps = await client.get_run_steps(str(run.run_id))

# Cancel a running execution
await client.cancel_run(str(run.run_id))

# List runs with filtering
runs = await client.list_runs(status=ExecutionStatus.COMPLETED, limit=10)
```

### Batch Execution

```python
# Execute multiple inputs in one request
run = await client.create_run(
    flow_id=flow_id,
    input=[{"x": 1}, {"x": 2}, {"x": 3}],
    max_concurrency=2,  # Limit parallel execution
)

# Get all results
items = await client.get_run_items(str(run.run_id))
```

### Health Check

```python
health = await client.health_check()
print(f"Server status: {health.status}")  # "healthy"
```

## Generated API Access

For advanced use cases, access the generated API client directly:

```python
from stepflow_api.api.run import get_run
from stepflow_api.models import CreateRunRequest

# Use the low-level client
response = await get_run.asyncio_detailed(
    client=client.api,
    run_id="..."
)
```

## Error Handling

```python
from stepflow_client import StepflowClient, StepflowClientError

try:
    await client.get_run("nonexistent-id")
except StepflowClientError as e:
    print(f"Error: {e}")
    print(f"HTTP status: {e.status_code}")
    print(f"Details: {e.details}")
```

## Response Types

All methods return typed response objects:

```python
from stepflow_client import (
    # StepflowExecutor protocol types
    FlowResult,           # run(), get_result() response
    ValidationResult,     # validate() response
    ComponentInfo,        # list_components() item type

    # API response types
    StoreFlowResponse,    # store_flow() response
    CreateRunResponse,    # create_run() response
    RunDetails,           # get_run() response
    ListItemsResponse,    # get_run_items() response
    ExecutionStatus,      # Enum: running, completed, failed, cancelled
)
```

## License

Apache-2.0
