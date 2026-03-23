---
sidebar_position: 1
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# Stepflow Introduction

Stepflow is a **workflow orchestrator** for AI applications. You define workflows declaratively in YAML, and Stepflow coordinates execution across workers — handling data flow, parallelism, fault tolerance, and scaling.

The orchestrator and workers are **separate processes** connected by an [open protobuf protocol](./protocol/index.md). This separation is the foundation of Stepflow's architecture: workers can be written in any language, scaled independently, and deployed on hardware matched to their workload — all without changing the workflow definition.

## Key Features

- **Combine anything in one workflow.** A single workflow can call an LLM, run a Python function, invoke an MCP tool, and execute a component from a different framework — each in its own worker process with no shared runtime or dependency conflicts.

- **Open protocol, any language.** Workers communicate over [pull-based task queues and gRPC](./protocol/index.md). Build workers with the [Python SDK](./workers/custom-components.md), implement the protocol directly in any language, or use [MCP servers](./components/mcp-tools.md) as components with zero wrapping code.

- **Dev to production, no workflow changes.** Locally, Stepflow runs as a [single binary](./getting-started.md) with embedded storage and subprocess workers. In production, the same workflows run on a [distributed cluster](./deployment/index.md) with dedicated worker pools, persistent storage, and message brokers like [NATS](./protocol/index.md#nats-queues) — you change the infrastructure, not the workflow.

- **Production-grade by default.** Stepflow [journals every step result](./deployment/persistence-recovery.md), so workflows resume from the last successful step after a crash. Workers run in isolated processes with independent scaling and [resource routing](./deployment/index.md) to appropriate hardware.

- **Batch execution.** Process thousands of inputs in parallel with [configurable concurrency](./flows/batch-execution.md), progress tracking, and fault isolation — locally or on remote servers.

- **Dynamic, composable flows.** Workflows can [spawn sub-workflows](./components/builtins/eval.md) at runtime. The declarative format is simple enough for LLMs to author flows dynamically using a whitelisted set of components — enabling safe agentic patterns.

## Architecture

<Tabs>
  <TabItem value="local" label="Local Development" default>
    During development, Stepflow manages workers as subprocesses. Everything runs on a single machine.

    ```mermaid
    flowchart LR
        Workflows@{ shape: docs, label:"Workflows" }

        subgraph Stepflow
            Host@{shape: process, label: "Orchestrator"}

            S1@{shape: processes, label: "Worker A"}
            S2@{shape: processes, label: "MCP Server B"}
            S3@{shape: processes, label: "Worker C"}
            D1@{shape: cylinder, label: "Data Source A"}
            D2@{shape: cylinder, label: "Data Source B"}
        end

        subgraph Internet
            D3@{shape: cylinder, label: "Remote API"}
        end

        Workflows <-->|"YAML"| Host
        S1 <--> D1
        S2 <--> D2
        Host --> |"Task Queue"| S1
        S1   --> |"gRPC"| Host
        Host <-->|"MCP"| S2
        Host --> |"Task Queue"| S3
        S3   --> |"gRPC"| Host
        S3 <-->|"HTTP"| D3
    ```
  </TabItem>
  <TabItem value="production" label="Production">
    In production, workers run in separate containers or nodes. The orchestrator routes tasks to worker pools via named queues, enabling independent scaling and resource isolation.

    ```mermaid
    flowchart LR
        Workflows@{ shape: docs, label:"Workflows" }

        subgraph Orchestrator Node
            Host["Orchestrator"]
        end

        subgraph Worker Pool A
            S1["Worker A"]
            S2["MCP Server B"]
            D1[("Data Source A")]
            D2[("Data Source B")]
            S1 <--> D1
            S2 <--> D2
        end

        subgraph Worker Pool B
            S3["Worker C"]
        end

        subgraph Internet
            D3[("Remote API")]
        end

        Workflows <-->|"YAML"| Host
        Host --> |"Task Queue"| S1
        S1   --> |"gRPC"| Host
        Host <-->|"MCP"| S2
        Host --> |"Task Queue"| S3
        S3   --> |"gRPC"| Host
        S3 <-->|"HTTP"| D3
    ```
  </TabItem>
</Tabs>

The **orchestrator** executes workflows, manages data flow between steps, persists state, and routes tasks to worker pools. **Workers** pull tasks, execute components, and return results — controlling their own concurrency by choosing when to pull the next task.

## Dev to Production

Stepflow scales from a single binary to a distributed cluster — no workflow changes required.

| | Development | Production |
|---|---|---|
| **Orchestrator** | Single local binary | Cluster with persistent storage |
| **Workers** | Subprocesses | Separate containers/nodes |
| **Task routing** | In-process task queues | Named queues (gRPC or NATS) |
| **Scaling** | Single machine | Independent worker pool scaling |
| **Workflows** | Same YAML | Same YAML |

See [Production Deployment](./deployment/index.md) for architecture patterns and scaling strategies.

## Next Steps

* [Get Started](./getting-started.md) — install Stepflow and run your first workflow
* [Flows](./flows/index.md) — learn the workflow definition language
* [Components](./components/index.md) — explore built-in and custom components
* [Deployment](./deployment/index.md) — production architecture and scaling
* [FAQ](./faq.md) — comparisons with other workflow and orchestration technologies