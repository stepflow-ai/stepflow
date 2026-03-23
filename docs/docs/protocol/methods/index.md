---
sidebar_position: 1
---

# Methods Reference

The Stepflow Protocol defines gRPC services for communication between the orchestrator and workers. This reference provides a complete overview of all available RPCs organized by service.

## gRPC Services

The protocol methods are organized into the following services:

1. **[Initialization](./initialization.md)** - Worker connection via `PullTasks`
2. **[Components](./components.md)** - Component discovery, introspection, and execution
3. **[Runs](./runs.md)** - Workflow run submission and status retrieval
4. **[Blob Storage](./blobs.md)** - Content-addressable data storage via HTTP API

## Complete RPC Reference

| RPC | Service | Direction | Description |
|-----|---------|-----------|-------------|
| **TasksService** | | | |
| [`PullTasks`](./initialization.md) | TasksService | Worker → Orchestrator | Pull task assignments via streaming RPC |
| `GetOrchestratorForRun` | TasksService | Worker → Orchestrator | Discover which orchestrator owns a run |
| **OrchestratorService** | | | |
| [`CompleteTask`](./components.md#task-completion) | OrchestratorService | Worker → Orchestrator | Report task results (success, failure, or component listing) |
| [`TaskHeartbeat`](./components.md#heartbeats) | OrchestratorService | Worker → Orchestrator | Signal liveness and report progress |
| [`SubmitRun`](./runs.md#runssubmit-method) | OrchestratorService | Worker → Orchestrator | Submit a workflow run for execution |
| [`GetRun`](./runs.md#runsget-method) | OrchestratorService | Worker → Orchestrator | Retrieve run status and results |
| **ComponentsService** | | | |
| `ListRegisteredComponents` | ComponentsService | Client → Orchestrator | Public API for component discovery |

:::note Blob Storage
Blob storage is accessed via HTTP API rather than gRPC. See [Blob Storage](./blobs.md) for details.
:::
