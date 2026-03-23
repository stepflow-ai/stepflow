---
sidebar_position: 0
---

# Workers

Workers are standalone processes that host workflow components for Stepflow. They allow you to implement custom logic, integrate with external systems, and extend the capabilities of Stepflow beyond the built-in components.

## What is a Worker?

A worker is a process that:

- **Hosts components**: Provides one or more executable components for workflows
- **Pulls tasks**: Pulls tasks from named queues hosted by the orchestrator
- **Returns results**: Executes components and reports results back to the orchestrator via gRPC
- **Calls back**: Can make gRPC requests back to the orchestrator (e.g., submit sub-workflows, store blobs)

Because workers communicate over an [open protocol](../protocol/index.md) (task queues for dispatch, gRPC for results and callbacks), they can be written in any language, deployed independently, and scaled horizontally.

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

For other languages, you can implement the [Stepflow Protocol](./implementing-workers.md) directly. The protocol is based on gRPC with Protocol Buffers.

## Next Steps

* [Create custom components](./custom-components.md) using the Python SDK
* [Implement workers](./implementing-workers.md) in any language using the Stepflow Protocol
* [Use user-defined functions (UDFs)](./udfs.md) to execute code dynamically within workflows
* [Protocol reference](../protocol/index.md) for complete protocol documentation