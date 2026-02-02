---
sidebar_position: 1
---

# Overview

The Stepflow Protocol is a JSON-RPC 2.0 based communication protocol designed for executing local and remote workflow components.
It enables bidirectional communication between the Stepflow runtime and workers over HTTP, allowing workers to access runtime services like blob storage while ensuring durable execution.

## Architecture

The protocol defines communication between two primary entities:

- **Stepflow Runtime**: Orchestrates workflow execution, persistence, and routes components to plugins
- **Worker**: Hosts one or more workflow components and executes them on behalf of the runtime

```mermaid
graph TB
    subgraph "Stepflow Runtime"
        WE[Workflow Engine]
        PS[Plugin System]
        BS[Blob Store]
    end

    subgraph "Worker A"
        CA1[Component A1]
        CA2[Component A2]
        CTX[Worker Context]
    end

    subgraph "Worker B"
        CB1[Component B1]
        CB2[Component B2]
        CTX2[Worker Context]
    end

    WE --> PS
    PS <--> CA1
    PS <--> CB1
    PS <--> BS
    CTX <--> BS
    CTX2 <--> BS

    style WE fill:#e1f5fe
    style BS fill:#f3e5f5
    style CA1 fill:#e8f5e8
    style CB1 fill:#e8f5e8
```

## Core Concepts

### JSON-RPC Foundation
The protocol builds on JSON-RPC 2.0, providing:
- **Request/Response patterns** for method calls
- **Notification patterns** for one-way messages
- **Standardized error handling** with custom error codes
- **Message correlation** through request/response IDs

### Bidirectional Communication
Unlike traditional client-server models, the protocol supports bidirectional communication:
- **Runtime → Server**: Component execution, discovery, information requests
- **Server → Runtime**: Blob storage operations, workflow introspection, resource access

### Content-Addressable Storage
The protocol includes a built-in blob storage system:
- **Content-based IDs**: SHA-256 hashes ensure data integrity
- **Automatic deduplication**: Identical data shares the same blob ID
- **Cross-component sharing**: Blobs can be accessed by any component in a workflow
- **Type-aware storage**: Support for different blob types (JSON, binary, etc.)

### HTTP Transport
Workers communicate via HTTP with Server-Sent Events (SSE) for bidirectional communication:
- **Streamable HTTP**: JSON-RPC over HTTP with SSE streaming
- **Bidirectional calls**: Workers can call back to the runtime during execution
- **Flexible deployment**: Workers can run locally (spawned by runtime) or remotely

See [Transport](./transport.md) for details.

## Method Categories

The protocol organizes methods into logical categories:

### Initialization Methods
- `initialize` - Establish protocol version and capabilities
- `initialized` - Confirm initialization completion

### Component Methods
- `components/list` - Discover available components
- `components/info` - Get component metadata and schemas
- `components/execute` - Execute a component with input data

See [Component Methods](./methods/components.md) for details.

### Blob Storage Methods
- `blobs/put` - Store data and receive content-addressable ID
- `blobs/get` - Retrieve data by blob ID

See [Blob Methods](./methods/blobs.md) for details.

### Run Methods
- `runs/submit` - Submit a workflow run for execution
- `runs/get` - Retrieve run status and results

See [Run Methods](./methods/runs.md) for details.

## Communication Patterns

### Synchronous Execution
Simple component operations follow a request-response pattern:

```mermaid
sequenceDiagram
    participant Runtime as Stepflow Runtime
    participant Worker as Worker

    Runtime->>+Worker: components/execute request
    Worker->>Worker: Process input data
    Worker-->>-Runtime: execution response
```

### Bidirectional Operations
Workers can make requests back to the runtime during execution via SSE streaming:

```mermaid
sequenceDiagram
    participant Runtime as Stepflow Runtime
    participant Worker as Worker

    Runtime->>+Worker: components/execute request
    Note over Worker: Start SSE stream
    Worker-->>Runtime: SSE: blobs/put request
    Runtime->>Worker: blobs/put response
    Worker-->>-Runtime: SSE: execution result
```

See the [Bidirectional Communication](./bidirectional.md) document for more details.

## Error Handling

The protocol defines standard error codes for consistent error handling:

- **-32xxx**: JSON-RPC standard errors (parse, invalid request, method not found)
- **-32000 to -32099**: Stepflow-specific errors (component not found, execution failed, etc.)

All errors include structured data for programmatic handling and user-friendly messages for debugging.

See the [Error Handling](./errors.md) document for a complete list of error codes and their meanings.

## Schema Integration

All protocol messages are defined with JSON Schema:
- **Request/response validation**: Ensures message conformity
- **Development tooling**: Enables auto-completion and validation in IDEs
- **Documentation generation**: Schemas serve as authoritative message specifications
- **Multi-language support**: Schemas generate types for Python, TypeScript, and other SDKs

See the [protocol schema](../reference/protocol-schema.mdx) for detailed message definitions.

## Next Steps

- **[Message Format](./message-format.md)**: JSON-RPC message structure and correlation
- **[Transport](./transport.md)**: HTTP transport specification
- **[Methods](./methods/)**: Detailed method specifications with examples
- **[Error Handling](./errors.md)**: Complete error code reference
- **[Implementing Workers](../components/component-server/implementing-workers.md)**: Guide to building workers