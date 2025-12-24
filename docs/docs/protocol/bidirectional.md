---
sidebar_position: 5
---

# Bidirectional Communication

The Stepflow Protocol supports bidirectional communication, enabling component servers to make requests back to the runtime during component execution. This capability enables powerful patterns like blob storage, run submission, and runtime introspection while maintaining the JSON-RPC request-response model.

## Overview

While the primary communication flow is Runtime → Component Server, the protocol enables Component Server → Runtime requests for:

- **Blob Storage**: Store and retrieve persistent data using content-addressable storage
- **Run Submission**: Submit and monitor sub-workflow executions
- **Resource Access**: Request additional resources or capabilities (future)

## Communication Model

### Unidirectional vs Bidirectional

**Traditional Model (Unidirectional):**
```mermaid
sequenceDiagram
    participant R as Runtime
    participant S as Component Server

    R->>+S: component_execute request
    S->>S: Process input data
    S-->>-R: component_execute response
```

**Stepflow Model (Bidirectional):**
```mermaid
sequenceDiagram
    participant R as Runtime
    participant S as Component Server

    R->>+S: component_execute request

    Note over S: Component can make requests during execution
    S->>+R: blobs/put request
    R-->>-S: blob_id response

    S->>+R: runs/submit request
    R-->>-S: run status response

    S-->>-R: component_execute response
```

## Available Methods

Component servers can call these methods during execution:

### Blob Storage Methods
- **`blobs/put`**: Store JSON data, receive content-addressable blob ID
- **`blobs/get`**: Retrieve stored data by blob ID

See [Blob Storage Methods](./methods/blobs.md) for detailed specifications.

### Run Methods
- **`runs/submit`**: Submit a workflow run for execution
- **`runs/get`**: Retrieve run status and results

See [Run Methods](./methods/runs.md) for detailed specifications.
