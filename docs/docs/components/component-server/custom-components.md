---
sidebar_position: 2
---

# Custom Components

Custom components allow you to extend StepFlow with your own business logic, integrations, and data processing capabilities. This guide shows you how to create custom components using the Python SDK.

## Quick Start

### Installation

Install the Python SDK:

```bash
# Using uv (recommended)
uv add stepflow-py

# Using pip
pip install stepflow-py
```

For HTTP transport support:

```bash
# Using uv
uv add stepflow-py[http]

# Using pip
pip install stepflow-py[http]
```

### Basic Component Server

Create a simple component server:

```python
# my_server.py
from stepflow_py import StepflowServer
import msgspec

# Define input and output types
class GreetingInput(msgspec.Struct):
    name: str
    language: str = "en"

class GreetingOutput(msgspec.Struct):
    message: str
    language: str

# Create server instance
server = StepflowServer()

# Register a component
@server.component
def greet(input: GreetingInput) -> GreetingOutput:
    greetings = {
        "en": f"Hello, {input.name}!",
        "es": f"¡Hola, {input.name}!",
        "fr": f"Bonjour, {input.name}!",
    }

    message = greetings.get(input.language, greetings["en"])
    return GreetingOutput(message=message, language=input.language)

if __name__ == "__main__":
    server.run()
```

### Configuration

Configure the component server in `stepflow-config.yml`:

```yaml
plugins:
  my_components:
    type: stepflow
    transport: stdio
    command: python
    args: ["my_server.py"]

routes:
  "/my/{*component}":
    - plugin: my_components
```

See the [Configuration Guide](../configuration/index.md) for more details.

### Using in Workflows

Reference your components in workflow YAML files:

```yaml
schema: https://stepflow.org/schemas/v1/flow.json
input_schema:
  type: object
  properties:
    user_name:
      type: string

steps:
- id: greeting_step
  component: /my/greet
  input:
    name:
      $from: { workflow: input }
      path: user_name
    language: "es"

output:
  greeting:
    $from: { step: greeting_step }
    path: message
```

See [Steps](../steps/index.md) for more details on using components in steps within a flow.

## Component Development

### Type Definitions

Use `msgspec.Struct` for type-safe input and output definitions:

```python
import msgspec
from typing import Optional, List, Dict, Any

class ProcessingInput(msgspec.Struct):
    # Required fields
    data: List[Dict[str, Any]]
    operation: str

    # Optional fields with defaults
    batch_size: int = 100
    timeout: Optional[int] = None

    # Complex nested types
    config: Optional[Dict[str, Any]] = None

class ProcessingOutput(msgspec.Struct):
    processed_count: int
    results: List[Dict[str, Any]]
    errors: List[str] = msgspec.field(default_factory=list)
    metadata: Dict[str, Any] = msgspec.field(default_factory=dict)
```

### Synchronous Components

Simple components that don't require I/O operations:

```python
@server.component
def calculate_statistics(input: DataInput) -> StatsOutput:
    """Calculate basic statistics for a dataset."""
    data = input.numbers

    return StatsOutput(
        count=len(data),
        sum=sum(data),
        mean=sum(data) / len(data) if data else 0,
        min=min(data) if data else None,
        max=max(data) if data else None
    )
```

### Asynchronous Components

Components that perform I/O operations, API calls, or other async work:

```python
import asyncio
import httpx

@server.component
async def fetch_user_data(input: UserInput) -> UserOutput:
    """Fetch user data from external API."""
    async with httpx.AsyncClient() as client:
        response = await client.get(f"https://api.example.com/users/{input.user_id}")
        response.raise_for_status()

        user_data = response.json()
        return UserOutput(
            user_id=user_data["id"],
            name=user_data["name"],
            email=user_data["email"],
            last_seen=user_data.get("last_seen")
        )
```

### Error Handling

Handle different types of errors appropriately:

```python
@server.component
async def process_with_validation(input: ProcessInput) -> ProcessOutput:
    """Process data with comprehensive error handling."""
    try:
        # Business logic validation
        if not input.data:
            return ProcessOutput(
                success=False,
                error="No data provided",
                error_type="validation_error"
            )

        # Process the data
        result = await process_data(input.data)

        return ProcessOutput(
            success=True,
            result=result,
            processed_count=len(input.data)
        )

    except ValueError as e:
        # Business logic error - workflow can handle this
        return ProcessOutput(
            success=False,
            error=f"Invalid data format: {e}",
            error_type="validation_error"
        )

    except Exception as e:
        # System error - this will halt workflow execution
        raise RuntimeError(f"Failed to process data: {e}")
```

## Advanced Features with Context {#context}

### Using StepflowContext

Components can access the StepFlow runtime through the `StepflowContext` parameter.
This can be used to store and retrieve data using blob storage or execute sub-workflows.

### Blob Storage Operations

Use blob storage for persisting data between workflow steps:

```python
@server.component
async def data_aggregator(
    input: AggregatorInput,
    context: StepflowContext
) -> AggregatorOutput:
    """Aggregate data from multiple blob sources."""

    aggregated_data = []

    # Retrieve data from multiple blobs
    for blob_id in input.source_blob_ids:
        try:
            # highlight-start
            blob_data = await context.get_blob(blob_id)
            aggregated_data.extend(blob_data.get("items", []))
            # highlight-end
        except Exception as e:
            context.log(f"Failed to retrieve blob {blob_id}: {e}")

    # Process aggregated data
    processed = process_aggregated_data(aggregated_data)

    # Store result
    # highlight-start
    result_blob_id = await context.put_blob({
        "aggregated_count": len(aggregated_data),
        "processed_data": processed,
        "source_blobs": input.source_blob_ids
    })
    # highlight-end

    return AggregatorOutput(
        result_blob_id=result_blob_id,
        total_items=len(aggregated_data)
    )
```

### Sub-workflow Execution

Execute other workflows from within components.
This is particularly useful for orchestrating complex tasks involving multiple steps in parallel.
For example, this creates a `workflow_orchestrator` component which runs sub-workflows.
In this example, each workflow is awaited immediately, but these could use `asyncio.gather` for parallel execution.

```python
@server.component
async def workflow_orchestrator(
    input: OrchestratorInput,
    context: StepflowContext
) -> OrchestratorOutput:
    """Execute sub-workflows based on input conditions."""

    results = []

    for task in input.tasks:
        if task.type == "data_processing":
            # Execute data processing workflow
            # highlight-start
            result = await context.evaluate_flow_by_id(
                flow_id=input.data_processing_flow_id,
                input={"data": task.data, "config": task.config}
            )
            # highlight-end
            results.append({
                "task_id": task.id,
                "type": "data_processing",
                "result": result
            })

        elif task.type == "ml_inference":
            # Execute ML inference workflow
            # highlight-start
            result = await context.evaluate_flow_by_id(
                flow_id=input.ml_flow_id,
                input={"model": task.model, "features": task.features}
            )
            # highlight-end
            results.append({
                "task_id": task.id,
                "type": "ml_inference",
                "result": result
            })

    return OrchestratorOutput(
        completed_tasks=len(results),
        results=results
    )
```