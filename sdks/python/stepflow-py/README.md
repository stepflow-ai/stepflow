# stepflow-py

The official Python SDK for Stepflow - an orchestration engine for AI workflows.

## Installation

```bash
pip install stepflow-py
```

## Features

- **API Client**: Type-safe client for the Stepflow REST API
- **Worker Module**: Build component servers that extend Stepflow functionality
- **Flow Builder**: Programmatically create and manipulate workflows

## Quick Start

### Using the API Client

```python
from stepflow_py.api import ApiClient, Configuration, FlowApi

config = Configuration(host="http://localhost:8080")
client = ApiClient(configuration=config)
flow_api = FlowApi(client)

# Submit a workflow
response = flow_api.submit_flow(flow_definition)
```

### Building a Worker (Component Server)

```python
from stepflow_py.worker import StepflowServer, StepflowContext
import msgspec

class MyInput(msgspec.Struct):
    message: str

class MyOutput(msgspec.Struct):
    result: str

server = StepflowServer()

@server.component
async def my_component(input: MyInput, context: StepflowContext) -> MyOutput:
    return MyOutput(result=f"Processed: {input.message}")

if __name__ == "__main__":
    import asyncio
    asyncio.run(server.run())
```

## Documentation

For full documentation, visit [stepflow.org](https://stepflow.org).

## License

Apache License 2.0
