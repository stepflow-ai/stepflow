---
sidebar_position: 1
---

# Components Overview

Components are the executable units in Stepflow workflows — each step invokes a component to do its work. Stepflow provides built-in components for common tasks, and you can create custom components using the [Python SDK](../workers/custom-components.md), the [gRPC protocol](../protocol/index.md) directly, or [MCP servers](./mcp-tools.md).

## Types of Components

### 1. Built-in Components

Stepflow provides a variety of built-in components that handle common operations:

- **Data Storage**: `/put_blob`, `/get_blob` for storing and retrieving data managed by the Stepflow runtime
- **AI Integration**: `/openai`, `/create_messages` demonstrating interaction with AI APIs
- **File Operations**: `/load_file` demonstrating interaction with the local file system
- **Workflow Control**: `/eval` for executing nested workflows

[Learn more about built-in components →](./builtins/index.md)

### 2. Worker Components

[Workers](../workers/index.md) host custom components built with Stepflow SDKs:

- **Python SDK**: Build components in Python with full async support
- **Any Language**: Implement the Stepflow Protocol directly

[Learn more about workers →](../workers/index.md)

:::info User-Defined Functions (UDFs)
Many of the SDKs also support user-defined functions (UDFs).
These are typically implemented as a component provided by the SDK that takes code as an input and executes it.
[Learn more about user-defined functions →](../workers/udfs.md).
:::

### 3. MCP Tool Components

Use tools from Model Context Protocol (MCP) servers as components:

- Access file systems, databases, and APIs through MCP
- Leverage the growing ecosystem of MCP tools
- No additional wrapping needed—MCP tools work directly as components

[Learn more about MCP tools →](./mcp-tools.md).

## Next Steps

- [Explore built-in components](./builtins/index.md) for common operations
- [Learn how to create custom components](../workers/custom-components.md) using Stepflow SDKs
- [Create steps](../flows/steps.md) to use components in a flow
- Read the [FAQ](../faq.md) for comparisons with other workflow technologies