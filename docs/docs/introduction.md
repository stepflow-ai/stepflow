---
sidebar_position: 1
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# StepFlow Introduction

StepFlow allows you to create and execute AI workflows combining components from different tools and services, both locally and in the cloud.
With StepFlow, components may execute locally or remotely, allowing simple development while providing isolation and resource management for production scale.

StepFlow defines a protocol for component servers, allowing a combination of custom and off-the-shelf components to be combined within a single workflow.
By routing specific component servers to different StepFlow runtimes, you can create workflows that run across multiple machines, containers, or cloud services.
Its modular architecture ensures secure, isolated execution of components—whether running locally or deployed to production.

StepFlow further solves for production problems like durability and fault-tolerance by journalling the results of each component execution, allowing workflows to be resumed from the last successful step in the event of a failure without adding complexity to component servers.

## Architecture

StepFlow consists of a runtime that manages the execution of workflows and servers that provide components and tools using the StepFlow protocol or Model Context Protocol.

<Tabs>
  <TabItem value="local" label="Local" default>
    During development, the StepFlow runtime can manage the component servers and MCP servers in subprocesses, communicating over stdio.

    ```mermaid
    flowchart LR
        Workflows["Workflows"]@{ shape: docs }
        Workflows <-->|"Workflow YAML"| Host
        subgraph "Local"
            Host["Stepflow Runtime"]
            S1["Component Server A"]
            S2["MCP Tool Server B"]
            S3["Component Server C"]
            Host <-->|"StepFlow Protocol"| S1
            Host <-->|"MCP Protocol"| S2
            Host <-->|"StepFlow Protocol"| S3
            S1 <--> D1[("Local<br>Data Source A")]
            S2 <--> D2[("Local<br>Data Source B")]
        end
        subgraph "Internet"
            S3 <-->|"Web APIs"| D3[("Remote<br>Service C")]
        end
    ```
  </TabItem>
  <TabItem value="production" label="Production">
    In production, the StepFlow runtime can communicate with remote servers in separate containers or k8s nodes.
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
            Host <-->|"StepFlow Protocol"| S1
            Host <-->|"MCP Protocol"| S2
            Host <-->|"StepFlow Protocol"| S3
        end
        subgraph "Internet"
            S3 <-->|"Web APIs"| D3[("Remote<br>Service C")]
        end
    ```
  </TabItem>
</Tabs>

## Next Steps

* [Get Started](./getting_started.md) by installing StepFlow and running your first flow.
* Read more about writing your own [Workflows](./workflows/index.md).
* Learn how to create your own components using the [StepFlow Protocol](./protocol/index.md).