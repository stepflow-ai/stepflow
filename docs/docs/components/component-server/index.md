---
sidebar_position: 1
---

# Component Server

Component servers are the backbone of StepFlow, providing a way to create and manage components that can be used in workflows. They allow you to implement custom logic, integrate with external systems, and extend the capabilities of StepFlow beyond the built-in components.

## How Component Servers Work

Component servers are standalone processes that communicate with the StepFlow runtime using a JSON-RPC protocol. This architecture provides several key benefits:

- **Process Isolation**: Each component server runs in its own process, providing security and fault isolation
- **Language Flexibility**: Component servers can be written in any language that supports JSON-RPC
- **Independent Development**: Components can be developed, tested, and deployed independently
- **Scalability**: Multiple instances of component servers can run simultaneously

## Next Steps

* [Create custom components](./custom-components.md) using the Python SDK
* [Use user-defined functions (UDFs)](./udfs.md) to execute code dynamically within workflows
* [Use the Stepflow Protocol to create components in other languages](../protocol/index.md)