---
sidebar_position: 3
---

# MCP Tools

[Model Context Protocol (MCP)](https://modelcontextprotocol.io/) provides a standardized way for AI systems to connect to external tools and data sources.

Stepflow's MCP integration allows you to use any MCP-compatible server as a source of components in your workflows, giving you access to file systems, databases, APIs, and other services through a unified interface.

## Quick Start

### Installation

No additional installation is required for MCP support - it's built into Stepflow. However, you'll need the MCP servers you want to use. Many are available as npm packages.

### Basic Configuration

Configure MCP servers as plugins in your `stepflow-config.yml`:

```yaml
plugins:
  # MCP filesystem server
  filesystem:
    type: mcp
    command: npx
    args: ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]

routes:
  "/filesystem/{*component}":
    - plugin: filesystem
```

### Simple Workflow Example

Use MCP tools in your workflows with the `/plugin_name/tool_name` format:

```yaml
schema: https://stepflow.org/schemas/v1/flow.json
input_schema:
  type: object
  properties:
    filename:
      type: string
    content:
      type: string

steps:
- id: write_file
  component: /filesystem/write_file
  input:
    path:
      { $input: "filename" }
    content:
      { $input: "content" }

- id: read_file
  component: /filesystem/read_file
  input:
    path:
      { $input: "filename" }

output:
  file_content:
    { $step: read_file }
```

## Next Steps

- Browse [available MCP servers](https://github.com/modelcontextprotocol/servers) in the official repository
- Learn about [Custom Components](./component-server/custom-components.md) for building your own tools