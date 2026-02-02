---
sidebar_position: 1
---

# Workers

Workers (also called component servers) are standalone processes that host workflow components for Stepflow. They allow you to implement custom logic, integrate with external systems, and extend the capabilities of Stepflow beyond the built-in components.

## What is a Worker?

A worker is a process that:

- **Hosts components**: Provides one or more executable components for workflows
- **Executes requests**: Receives execution requests from the Stepflow runtime and returns results
- **Communicates bidirectionally**: Can make calls back to the runtime (e.g., for blob storage)

Workers communicate with the Stepflow runtime using JSON-RPC 2.0 over HTTP. This architecture provides several key benefits:

- **Process Isolation**: Each worker runs in its own process, providing security and fault isolation
- **Language Flexibility**: Workers can be written in any language that implements the Stepflow Protocol
- **Independent Scaling**: Workers can be deployed and scaled independently
- **Distributed Execution**: Workers can run on different machines, enabling horizontal scaling

## Getting Started

There are two main approaches to creating workers:

### 1. Using the Python SDK (Recommended)

The [Python SDK](./custom-components.md) provides a high-level API that handles all protocol details:

```python
from stepflow_py import StepflowServer

server = StepflowServer()

@server.component
def my_component(input: MyInput) -> MyOutput:
    return MyOutput(result=process(input.data))

if __name__ == "__main__":
    server.run()
```

### 2. Implementing the Protocol Directly

For other languages, you can implement the [Stepflow Protocol](./implementing-workers.md) directly. The protocol is based on JSON-RPC 2.0 with Streamable HTTP transport.

## Next Steps

* [Create custom components](./custom-components.md) using the Python SDK
* [Implement workers](./implementing-workers.md) in any language using the Stepflow Protocol
* [Use user-defined functions (UDFs)](./udfs.md) to execute code dynamically within workflows
* [Protocol reference](../../protocol/index.md) for complete protocol documentation