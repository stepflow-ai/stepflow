---
sidebar_position: 1
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# Stepflow Introduction

Stepflow is a **workflow orchestrator** that enables you to create and execute AI workflows by combining components from different tools and services, both locally and in the cloud.

As an orchestrator, Stepflow manages workflow execution, data flow, and state persistence, while **component servers provide the actual business logic**. This separation allows for flexible, scalable architectures where components can execute locally during development or be distributed across multiple machines in production.

Stepflow defines a protocol for component servers, allowing a combination of custom and off-the-shelf components to be orchestrated within a single workflow.
By routing specific component servers to different Stepflow runtimes, you can create workflows that run across multiple machines, containers, or cloud services.
Its modular architecture ensures secure, isolated execution of components—whether running locally or deployed to production.

Stepflow further solves for production problems like durability and fault-tolerance by journalling the results of each component execution, allowing workflows to be resumed from the last successful step in the event of a failure without adding complexity to component servers.

## Architecture

Stepflow consists of a **workflow orchestrator runtime** that manages the execution of workflows and **component servers** that provide the actual business logic using the Stepflow protocol or Model Context Protocol.

The orchestrator handles:
- Workflow execution and step coordination
- Data flow between components
- State persistence and fault tolerance
- Resource management and scaling

Component servers provide:
- Business logic implementation
- Domain-specific functionality
- Integration with external services
- Custom processing capabilities

<Tabs>
  <TabItem value="local" label="Local" default>
    During development, the Stepflow runtime can manage the component servers and MCP servers in subprocesses, communicating over stdio.

    ```mermaid
    flowchart LR
        Workflows["Workflows"]@{ shape: docs }
        Workflows <-->|"Workflow YAML"| Host
        subgraph "Local"
            Host["Stepflow Runtime"]
            S1["Component Server A"]
            S2["MCP Tool Server B"]
            S3["Component Server C"]
            Host <-->|"Stepflow Protocol"| S1
            Host <-->|"MCP Protocol"| S2
            Host <-->|"Stepflow Protocol"| S3
            S1 <--> D1[("Local<br>Data Source A")]
            S2 <--> D2[("Local<br>Data Source B")]
        end
        subgraph "Internet"
            S3 <-->|"Web APIs"| D3[("Remote<br>Service C")]
        end
    ```
  </TabItem>
  <TabItem value="production" label="Production">
    In production, the Stepflow runtime can communicate with remote servers in separate containers or k8s nodes.
    This allows sharing a server across multiple runtimes and isolating specific components on dedicated servers for better security and resource management.

    ```mermaid
    flowchart LR
        Workflows["Workflows"]@{ shape: docs }
        Workflows <-->|"Workflow YAML"| Host
        subgraph "Runtime Node"
            Host["Stepflow Runtime"]
        end
        subgraph "Components A+B node"
            S1["Component Server A"]
            S2["MCP Tool Server B"]
            S1 <--> D1[("Local<br>Data Source A")]
            S2 <--> D2[("Local<br>Data Source B")]
        end
        subgraph "Components C node"
            S3["Component Server C"]
            Host <-->|"Stepflow Protocol"| S1
            Host <-->|"MCP Protocol"| S2
            Host <-->|"Stepflow Protocol"| S3
        end
        subgraph "Internet"
            S3 <-->|"Web APIs"| D3[("Remote<br>Service C")]
        end
    ```
  </TabItem>
</Tabs>

## Next Steps

* [Get Started](./getting-started.md) by installing Stepflow and running your first flow.
* Read more about writing your own [Workflows](./flows/index.md).
* Learn about available components and creating your own in [Components](./components/index.md).
* Learn about [production deployment](./deployment/index.md).