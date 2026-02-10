---
sidebar_position: 6
---

# Configuration

Stepflow configuration controls which plugins and components are available to workflows, state storage backends, and other runtime settings. Configuration enables you to use the same workflow definitions across different environments (development, staging, production) by changing only the underlying infrastructure and component implementations.

Stepflow's flexible configuration supports a truly distributed architecture, allowing component servers to run across multiple machines or containers for scalable batch execution and high-throughput workflows. See [Batch Execution](./flows/batch-execution.md) for details.

Key concepts:
- **Plugins** provide components (built-in, external processes, HTTP services)
- **Routes** map component paths to specific plugins
- **State Stores** allow persisting workflow state for durable execution

## Configuration File Structure

Stepflow configuration files use YAML format with camelCase field names:

```yaml
plugins:
  builtin:
    type: builtin
  python:
    type: stepflow
    command: uv
    args: ["--project", "../sdks/python", "run", "stepflow_py"]
    env:
      LOG_LEVEL: "${LOG_LEVEL:-info}"

routes:
  "/python/{*component}":
    - plugin: python
  "/{*component}":
    - plugin: builtin

storageConfig:
  type: sqlite
  databaseUrl: "sqlite:workflow_state.db"
  autoMigrate: true
  maxConnections: 10
```

### Environment Variable Substitution

Plugin configurations support environment variable substitution:

- **Basic substitution**: `${VAR}` - expands to the value of environment variable `VAR`
- **Default values**: `${VAR:-default}` - uses `default` if `VAR` is not set
- **Nested substitution**: `${HOME}/projects/${USER}` - combines multiple variables

Environment variables are resolved when the plugin is launched using the current process environment.
If a variable is not found and no default is provided, plugin initialization will fail.

:::tip Secret Management
Store sensitive data in environment variables rather than directly in configuration files. For flow-level configuration that varies by environment, use [Variables](./flows/variables.md) instead.
:::

### Configuration File Location

Stepflow locates configuration files using the following search priority:

1. **Explicit path**: Use `--config` CLI option if provided
2. **Workflow directory**: Look in the same directory as the workflow file
3. **Current directory**: Look in the current working directory
4. **Default**: Use built-in configuration with builtin components only

**Supported filenames** (checked in order):
- `stepflow-config.yml`
- `stepflow-config.yaml`
- `stepflow_config.yml`
- `stepflow_config.yaml`

If multiple configuration files exist in the same directory, Stepflow will report an error.

## Plugin Configuration

### Built-in Plugins

Built-in components provided by Stepflow itself:

```yaml
plugins:
  builtin:
    type: builtin
```

**Available Components:**
- `openai` - OpenAI API integration
- `create_messages` - Chat message creation
- `eval` - Nested workflow execution
- `put_blob` - Store data as blobs
- `get_blob` - Retrieve blob data
- `load_file` - Load and parse files

### Stepflow Plugins

External component servers that implement the Stepflow protocol.

#### Subprocess Mode

For local development and subprocess-based component servers:

```yaml
plugins:
  python:
    type: stepflow
    command: uv
    args: ["--project", "${PROJECT_DIR:-../sdks/python}", "run", "stepflow_py"]
    env:
      PYTHONPATH: "${HOME}/custom/path"
      DEBUG: "true"
      USER_CONFIG: "${USER:-anonymous}"
```

**Parameters:**
- **`command`**: Executable to run
- **`args`**: Command-line arguments (supports environment variable substitution)
- **`env`** (optional): Environment variables to set (supports substitution)

#### Remote Mode

For distributed architectures and remote component servers:

```yaml
plugins:
  remote_server:
    type: stepflow
    url: "http://localhost:8080"
```

**Parameters:**
- **`url`**: Base URL of the HTTP component server
- **`timeout`** (optional): Request timeout in seconds [default: 30]

**Features:**
- Optional MCP-style session negotiation for connection isolation
- Automatic fallback for servers that don't support session negotiation
- Scalable architecture suitable for production deployments

### MCP Plugins

Model Context Protocol servers can be used as component plugins:

```yaml
plugins:
  filesystem:
    type: mcp
    command: npx
    args: ["-y", "@modelcontextprotocol/server-filesystem", "${HOME}/workspace"]
    env:
      MCP_LOG_LEVEL: "${LOG_LEVEL:-info}"
      MCP_CONFIG_DIR: "${HOME}/.config/mcp"
```

**Parameters:**
- **`command`**: Executable to run the MCP server
- **`args`**: Command-line arguments (supports environment variable substitution)
- **`env`** (optional): Environment variables (supports substitution)

## Routing Configuration

Routing rules map component paths to specific plugins. **Route rules are required** for components to be accessible.

```yaml
routes:
  "/python/{*component}":
    - plugin: python
  "/filesystem/{*component}":
    - plugin: filesystem
  "/ai/chat":
    - conditions:
        - path: "$.model"
          value: "gpt-4"
      plugin: openai_plugin
    - plugin: fallback_plugin
  "/{*component}":
    - plugin: builtin
```

### Path Patterns

- **Component placeholders**: `{component}` matches a single path segment
- **Wildcard placeholders**: `{*component}` matches multiple path segments
- **Literal paths**: `/ai/chat` matches exactly

### Route Rules

Each route can have multiple rules evaluated in order:

- **`plugin`**: Plugin name to route to (must exist in plugins section)
- **`conditions`** (optional): Input-based conditions for conditional routing
- **`component`** (optional): Component name to pass to plugin [default: extracted component name]

### Input Conditions

Route based on workflow input data:

- **`path`**: JSON path expression (e.g., `$.model`, `$.config.temperature`)
- **`value`**: Expected value for exact match

All conditions must match for the rule to apply.

### Route Evaluation

- Rules are evaluated in order, first match wins
- Use `/{*component}` as a catch-all pattern for fallback routing
- Component paths like `/python/my_component` are matched against route patterns

## State Store Configuration

Stepflow supports multiple backends for storing workflow execution state and blob data.

### In-Memory State Store (Default)

```yaml
storageConfig:
  type: inMemory
```

- Fast access, no persistence
- Suitable for testing and development
- All data lost when process exits

### SQLite State Store

```yaml
storageConfig:
  type: sqlite
  databaseUrl: "sqlite:/tmp/prod_workflow_state.db?mode=rwc"
  autoMigrate: true
  maxConnections: 10
```

**Parameters:**
- **`databaseUrl`**: SQLite connection string
  - File path: `"sqlite:path/to/database.db"`
  - File path with create mode: `"sqlite:path/to/database.db?mode=rwc"` (creates file if it doesn't exist)
  - In-memory: `"sqlite::memory:"`
- **`autoMigrate`** (optional): Automatically create/update database schema [default: true]
- **`maxConnections`** (optional): Connection pool size [default: 10]

## Example: Development and Production {#example-dev-prod}

The same workflow can run in different environments by changing only the configuration. Here's an example from the [production model serving demo](https://github.com/stepflow/stepflow/tree/main/examples/production-model-serving):

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

<Tabs>
<TabItem value="development" label="Development">

```yaml
# Development Configuration - Local subprocess-based servers
plugins:
  builtin:
    type: builtin

  python_sdk:
    type: stepflow
    command: uv
    args: ["--project", "../../sdks/python", "run", "stepflow_py"]
    env:
      LOG_LEVEL: "${LOG_LEVEL:-INFO}"

  text_models:
    type: stepflow
    command: uv
    args: ["--project", "../../sdks/python", "run", "python", "text_models_server.py"]
    env:
      LOG_LEVEL: "${LOG_LEVEL:-INFO}"
      CUDA_VISIBLE_DEVICES: ""  # Disable GPU in development
      TRANSFORMERS_OFFLINE: "${TRANSFORMERS_OFFLINE:-0}"

  vision_models:
    type: stepflow
    command: uv
    args: ["--project", "../../sdks/python", "run", "python", "vision_models_server.py"]
    env:
      LOG_LEVEL: "${LOG_LEVEL:-INFO}"
      CUDA_VISIBLE_DEVICES: "${CUDA_VISIBLE_DEVICES:-}"

routes:
  "/models/text/{*component}":
    - plugin: text_models
  "/models/vision/{*component}":
    - plugin: vision_models
  "/python/{*component}":
    - plugin: python_sdk
  "/{*component}":
    - plugin: builtin

storageConfig:
  type: inMemory
```

</TabItem>
<TabItem value="production" label="Production">

```yaml
# Production Configuration - HTTP-based distributed servers
plugins:
  builtin:
    type: builtin

  text_models_cluster:
    type: stepflow
    url: "http://text-models:8080"

  vision_models_cluster:
    type: stepflow
    url: "http://vision-models:8081"

routes:
  "/models/text/{*component}":
    - plugin: text_models_cluster
  "/models/vision/{*component}":
    - plugin: vision_models_cluster
  "/python/{*component}":
    - plugin: text_models_cluster  # Python components handled by text models server
  "/{*component}":
    - plugin: builtin

storageConfig:
  type: sqlite
  databaseUrl: "sqlite:/tmp/prod_workflow_state.db?mode=rwc"
  autoMigrate: true
  maxConnections: 20
```

</TabItem>
</Tabs>

Specifically, note that both configurations provide the same paths, but use different plugin implementations and state stores.
This specifically allows local development to use the same workflow definitions as production deployments without modification.

Locally, component servers are started to execute the workflow.
In production, the Stepflow runtime connects to remote component servers over HTTP, allowing the component servers to be separately deployed and scaled.

## Runtime Environment Variables

Stepflow behavior can be configured through environment variables. These are separate from the [environment variable substitution](#environment-variable-substitution) used in configuration files.

### Orchestrator Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `STEPFLOW_COMPONENT_STDERR` | `quiet` | Controls how component server stderr is handled. `quiet` buffers stderr and only shows it on crash. `verbose` logs all stderr immediately (useful for debugging). |

#### Stderr Handling

When using stdio transport, component servers write logs and errors to stderr. By default, Stepflow uses **quiet mode** to reduce log noise:

- **Quiet mode** (default): Buffers stderr output and only displays it if the component server crashes. During initialization, all stderr is preserved (unbounded). After initialization, only the last 100 lines are kept.
- **Verbose mode**: Logs all stderr immediately with the prefix `Component stderr:`. Useful for debugging component issues.

```bash
# Enable verbose stderr for debugging
STEPFLOW_COMPONENT_STDERR=verbose stepflow run --flow workflow.yaml
```

### Python SDK Variables

The Python SDK supports additional environment variables for observability:

| Variable | Default | Description |
|----------|---------|-------------|
| `STEPFLOW_OTLP_ENDPOINT` | (none) | OpenTelemetry collector endpoint for traces and logs |
| `STEPFLOW_LOG_DESTINATION` | `stderr` (or `otlp` if endpoint set) | Where to send logs: `stderr`, `otlp`, `file`, or comma-separated combination |
| `STEPFLOW_LOG_LEVEL` | `INFO` | Logging level: `DEBUG`, `INFO`, `WARNING`, `ERROR` |
| `STEPFLOW_LOG_FILE` | (none) | File path when using `file` log destination |
| `STEPFLOW_TRACE_ENABLED` | `true` | Enable/disable OpenTelemetry tracing |

:::tip Reducing Log Noise
When `STEPFLOW_OTLP_ENDPOINT` is configured, the Python SDK automatically defaults `STEPFLOW_LOG_DESTINATION` to `otlp` instead of `stderr`. This sends logs to your observability backend rather than the orchestrator's stderr, reducing noise in production deployments.
:::