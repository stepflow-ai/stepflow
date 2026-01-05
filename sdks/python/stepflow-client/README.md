# stepflow-client

HTTP client for Stepflow servers.

## Installation

```bash
pip install stepflow-client
```

## Overview

This package provides an async HTTP client for communicating with remote Stepflow servers. It wraps the generated `stepflow-api` client with quality-of-life improvements like automatic file loading and simplified method signatures.

## Quick Start

```python
from stepflow_client import StepflowClient

async with StepflowClient("http://localhost:7837") as client:
    # Store a workflow
    store_response = await client.store_flow("workflow.yaml")

    # Execute it
    run_response = await client.create_run(
        flow_id=store_response.flow_id,
        input={"message": "hello"}
    )

    # Check the result
    details = await client.get_run(str(run_response.run_id))
    print(f"Status: {details.status}")
```

## API Reference

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

The `input` parameter accepts a list for batch execution:

```python
# Execute multiple inputs in one request
run = await client.create_run(
    flow_id=flow_id,
    input=[{"x": 1}, {"x": 2}, {"x": 3}],
    max_concurrency=2,  # Limit parallel execution
)

# Get all results
items = await client.get_run_items(str(run.run_id))
for item in items.items:
    if item.result.is_success:
        print(f"Input {item.index}: {item.result.output}")
```

### Component Discovery

```python
# List available components
components = await client.list_components()
for comp in components.components:
    print(f"{comp.path}: {comp.description}")

# Include input/output schemas
components = await client.list_components(include_schemas=True)
```

### Health Check

```python
health = await client.health_check()
print(f"Server status: {health.status}")  # "healthy"
```

## Low-Level API Access

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

All methods return typed response objects from `stepflow-api`:

```python
from stepflow_client import (
    StoreFlowResponse,    # store_flow() response
    CreateRunResponse,    # create_run() response
    RunDetails,           # get_run() response
    ListItemsResponse,    # get_run_items() response
    ExecutionStatus,      # Enum: running, completed, failed, cancelled
)
```

## License

Apache-2.0
