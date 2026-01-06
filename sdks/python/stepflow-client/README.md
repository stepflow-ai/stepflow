# stepflow-client

HTTP client for remote Stepflow servers.

## Installation

```bash
pip install stepflow-client
```

## Overview

This package provides an async HTTP client for connecting to remote Stepflow servers. It implements the `StepflowExecutor` protocol, making it interchangeable with `StepflowRuntime`.

For the protocol API documentation (`run`, `submit`, `validate`, etc.), see the [stepflow-core README](../stepflow-core/README.md).

## Quick Start

```python
from stepflow_client import StepflowClient

async with StepflowClient("http://localhost:7837") as client:
    result = await client.run("workflow.yaml", {"name": "World"})

    if result.is_success:
        print(result.output)
    else:
        print(f"Error: {result.error.message}")
```

## Configuration

```python
client = StepflowClient(
    base_url="http://localhost:7837",  # Server URL
    timeout=30.0,                       # Request timeout in seconds
)
```

## Error Handling

The client raises `StepflowClientError` for HTTP and API errors:

```python
from stepflow_client import StepflowClient, StepflowClientError

try:
    result = await client.run("workflow.yaml", {"x": 1})
except StepflowClientError as e:
    print(f"Error: {e}")
    print(f"HTTP status: {e.status_code}")
    print(f"Details: {e.details}")
```

## Low-Level API

For fine-grained control, the client exposes lower-level methods that map directly to the Stepflow HTTP API:

### Flow Management

```python
# Store a flow from file or dict
response = await client.store_flow("workflow.yaml")
flow_id = response.flow_id

# Get a stored flow
flow = await client.get_flow(flow_id)

# Delete a stored flow
await client.delete_flow(flow_id)
```

### Run Management

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

# Get execution results
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

For advanced use cases, access the generated OpenAPI client directly:

```python
from stepflow_api.api.run import get_run

response = await get_run.asyncio_detailed(
    client=client.api,
    run_id="..."
)
```

## Response Types

```python
from stepflow_client import (
    # Low-level API response types
    StoreFlowResponse,    # store_flow() response
    CreateRunResponse,    # create_run() response
    RunDetails,           # get_run() response
    ListItemsResponse,    # get_run_items() response
    ExecutionStatus,      # Enum: RUNNING, COMPLETED, FAILED, CANCELLED
)
```

## License

Apache-2.0
