---
sidebar_position: 1
---

# Methods Reference

The Stepflow Protocol defines methods and notifications for communication between the runtime and component servers. This reference provides a complete overview of all available methods organized by category.

## Method Categories

The protocol methods are organized into the following categories:

1. **[Initialization](./initialization.md)** - Protocol connection and capability negotiation
2. **[Components](./components.md)** - Component discovery, introspection, and execution
3. **[Runs](./runs.md)** - Workflow run submission and status retrieval
4. **[Blob Storage](./blobs.md)** - Content-addressable data storage via HTTP API

## Complete Method Reference

| Method | Direction | Type | Description |
|--------|-----------|------|-------------|
| **Initialization** | | | |
| [`initialize`](./initialization.md#initialize-method) | Runtime → Component | Request | Negotiate protocol version and establish capabilities |
| [`initialized`](./initialization.md#initialized-notification) | Runtime → Component | Notification | Confirm initialization is complete |
| **Components** | | | |
| [`components/list`](./components.md#componentslist-method) | Runtime → Component | Request | Discover all available components |
| [`components/info`](./components.md#componentsinfo-method) | Runtime → Component | Request | Get detailed component information and schema |
| [`components/execute`](./components.md#componentsexecute-method) | Runtime → Component | Request | Execute a component with input data |
| **Runs** | | | |
| [`runs/submit`](./runs.md#runssubmit-method) | Component → Runtime | Request | Submit a workflow run for execution |
| [`runs/get`](./runs.md#runsget-method) | Component → Runtime | Request | Retrieve run status and results |

:::note Blob Storage
Blob storage is accessed via HTTP API rather than the bidirectional protocol. See [Blob Storage](./blobs.md) for details.
:::
