---
sidebar_position: 1
---

# StepFlow Introduction

StepFlow is an open protocol and runtime for building, executing, and scaling GenAI workflows across local and cloud environments.
Its modular architecture ensures secure, isolated execution of components—whether running locally or deployed to production.
With durability, fault-tolerance, and an open specification, StepFlow empowers anyone to create, share, and 

## Local Architecture

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

- **StepFlow Runtime**: Where the workflow is executed.
- **Component Servers**: Lightweight programs that each expose specific workflow components through the standardized StepFlow Protocol.
- **MCP Servers**: MCP servers can be used as component servers, with each tool treated as a component.
- **Local Data Sources**: Files, databases and services that Component Servers can securely access.
- **Remote Services**: External systems available over the internet (e.g., through APIs) that Copmonent Servers servers can connect to.

## Production Architecture

This same architecture allows separating the component servers into separate containers or k8s nodes for production deployment.
The same component server could be used by multiple runtimes, each with a different set of components available.
This provides efficient resource and the ability to isolate different security concerns.

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

## Next Steps

* See [Getting Started](./getting_started.md) for installation and running your first workflows.
* Read more about writing your own [Workflows](./workflows/index.md).
* Learn how to create your own components using the [StepFlow Protocol](./protocol/index.md).