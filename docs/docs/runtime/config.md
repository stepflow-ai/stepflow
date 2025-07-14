---
sidebar_position: 3
---

# Configuration

StepFlow configuration controls which plugins and components are available to workflows, state storage backends, and other runtime settings. Configuration is defined in YAML files and follows a hierarchical resolution system.

## Configuration File Structure

The main configuration file (typically `stepflow-config.yml`) defines plugins, routing rules, and state storage:

```yaml
plugins:
  builtin:
    type: builtin
  python:
    type: stepflow
    command: uv
    args: ["--project", "../sdks/python", "run", "stepflow_sdk"]
  custom_server:
    type: stepflow
    command: "./my-component-server"
    args: ["--config", "server.json"]

routing:
  - match: "/python/*"
    target: python
  - match: "/custom/*"
    target: custom_server
  - match: "*"
    target: builtin

state_store:
  type: sqlite
  database_url: "sqlite:workflow_state.db"
  auto_migrate: true
  max_connections: 10
```

## Configuration Resolution

StepFlow uses a hierarchical approach to find configuration files:

1. **Explicit path**: Use `--config` CLI option if provided
2. **Workflow directory**: Look for config files in the same directory as the workflow file
3. **Current directory**: Look for config files in the current working directory
4. **Default**: Use built-in configuration with builtin components only

### Config File Names

StepFlow looks for these filenames in order:
- `stepflow-config.yml`
- `stepflow-config.yaml`
- `stepflow_config.yml`
- `stepflow_config.yaml`

## Plugin Configuration

### Plugin Types

#### Builtin Plugins

Built-in components provided by StepFlow itself:

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

#### Stdio Plugins

External component servers that communicate via JSON-RPC over stdin/stdout:

```yaml
plugins:
  python:
    type: stepflow
    command: uv
    args: ["--project", "../sdks/python", "run", "stepflow_sdk"]
    working_directory: "."  # optional, defaults to current directory
    env:                    # optional environment variables
      PYTHONPATH: "/custom/path"
      DEBUG: "true"
```

**Parameters:**
- **`type`**: Must be `stdio`
- **`command`**: Executable to run
- **`args`**: Command-line arguments
- **`working_directory`** (optional): Working directory for the command
- **`env`** (optional): Environment variables to set

#### HTTP Plugins (Future)

HTTP-based component servers (planned feature):

```yaml
plugins:
  remote_server:
    type: http
    base_url: "http://localhost:8080"
    timeout: 30
```

## Routing Configuration

**Routing rules are required** for components to be accessible. StepFlow uses routing rules to map component paths to specific plugins.

### Basic Routing

```yaml
routing:
  - match: "/python/*"
    target: python
  - match: "/filesystem/*"
    target: filesystem
  - match: "*"
    target: builtin
```

- **`match`**: Glob pattern to match component paths (supports `*` and `**` wildcards)
- **`target`**: Plugin name to route to (must match a key in the plugins section)
- Rules are evaluated in order, first match wins
- Use `*` as a catch-all pattern for fallback routing

### Advanced Routing with Input Conditions

```yaml
routing:
  - match: "/ai/chat"
    input:
      - path: "$.model"
        value: "gpt-4"
    target: openai_plugin
  - match: "/ai/chat"
    target: fallback_plugin
```

**Input conditions:**
- **`path`**: JSON path expression to extract value from input
- **`value`**: Expected value for exact match
- All conditions must match for the rule to apply

### Component URLs

Components are referenced by path format: `<component_name>` for builtin components or `/<plugin_name>/<component_name>` for plugin components

```yaml
steps:
  - id: my_step
    component: /python/my_component
    input: { }
```

### Python SDK Plugin

The Python SDK provides a convenient way to create components:

```yaml
plugins:
  python:
    type: stepflow
    command: uv
    args: ["--project", "path/to/python/project", "run", "stepflow_sdk"]
```

This plugin enables components like:
- `/python/udf` - Execute user-defined Python functions
- Custom components defined in your Python project

### MCP Integration

Model Context Protocol (MCP) servers can be used as component plugins:

```yaml
plugins:
  filesystem:
    type: stepflow
    command: npx
    args: ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/allowed/directory"]
```

MCP tools are accessed with format: `/server/tool_name`

## State Store Configuration

StepFlow supports multiple backends for storing workflow state and blobs.

### In-Memory State Store (Default)

No configuration needed - this is the default:

```yaml
# Optional explicit configuration
state_store:
  type: in_memory
```

**Characteristics:**
- Fast access
- No persistence
- Suitable for testing and development
- All data lost when process exits

### SQLite State Store

File-based SQLite database for persistent storage:

```yaml
state_store:
  type: sqlite
  database_url: "sqlite:workflow_state.db"
  auto_migrate: true
  max_connections: 10
```

**Parameters:**
- **`database_url`**: SQLite connection string
  - File path: `"sqlite:path/to/database.db"`
  - In-memory: `"sqlite::memory:"`
- **`auto_migrate`** (optional): Automatically create/update database schema [default: true]
- **`max_connections`** (optional): Connection pool size [default: 10]

**Characteristics:**
- Persistent storage
- File-based, no server required
- Suitable for single-node deployments
- Automatic schema migration

### PostgreSQL State Store (Future)

Enterprise-grade persistent storage (planned feature):

```yaml
state_store:
  type: postgresql
  database_url: "postgresql://user:password@localhost/stepflow"
  max_connections: 20
  auto_migrate: true
```

## Complete Configuration Examples

### Development Configuration

```yaml
# stepflow-config.yml for development
plugins:
  builtin:
    type: builtin
  python:
    type: stepflow
    command: uv
    args: ["--project", ".", "run", "stepflow_sdk"]
    env:
      DEBUG: "true"

routing:
  - match: "/python/*"
    target: python
  - match: "*"
    target: builtin

state_store:
  type: in_memory
```

### Production Configuration

```yaml
# stepflow-config.yml for production
plugins:
  builtin:
    type: builtin
  python:
    type: stepflow
    command: python
    args: ["-m", "stepflow_sdk"]
    working_directory: "/app/components"
  data_processing:
    type: stepflow
    command: "./data-processor"
    args: ["--config", "/etc/data-processor.json"]
  mcp_filesystem:
    type: stepflow
    command: npx
    args: ["-y", "@modelcontextprotocol/server-filesystem", "/app/data"]

routing:
  - match: "/python/*"
    target: python
  - match: "/data/*"
    target: data_processing
  - match: "/filesystem/*"
    target: mcp_filesystem
  - match: "*"
    target: builtin

state_store:
  type: sqlite
  database_url: "sqlite:/var/lib/stepflow/state.db"
  auto_migrate: true
  max_connections: 20
```

### Multi-Service Configuration

```yaml
# stepflow-config.yml for multi-service setup
plugins:
  builtin:
    type: builtin
  analytics:
    type: stepflow
    command: "./analytics-service"
    args: ["--port", "0"]  # Use stdio instead of HTTP
  ml_models:
    type: stepflow
    command: python
    args: ["-m", "ml_components"]
    env:
      MODEL_PATH: "/models"
      CUDA_VISIBLE_DEVICES: "0"
  external_apis:
    type: stepflow
    command: node
    args: ["api-gateway.js"]

routing:
  - match: "/analytics/*"
    target: analytics
  - match: "/ml/*"
    target: ml_models
  - match: "/external/*"
    target: external_apis
  - match: "*"
    target: builtin

state_store:
  type: sqlite
  database_url: "sqlite:/shared/storage/workflows.db"
  auto_migrate: true
  max_connections: 50
```

## Environment Variables

StepFlow respects these environment variables:

### Global Settings

- **`STEPFLOW_CONFIG`**: Path to configuration file (overrides automatic detection)
- **`STEPFLOW_LOG_LEVEL`**: Default log level (trace, debug, info, warn, error)
- **`STEPFLOW_STATE_STORE_URL`**: Override state store database URL

### Component-Specific

- **`OPENAI_API_KEY`**: OpenAI API key for `openai` component
- **`PYTHONPATH`**: Python path for Python components
- **Custom variables**: Passed through to component servers via plugin configuration

## Plugin Development

### Creating Stdio Components

To create a stdio component server:

1. **Implement JSON-RPC Protocol**: Handle `initialize`, `component_info`, and `component_execute` methods
2. **Define Component Schemas**: Provide input/output schemas via `component_info`
3. **Handle Bidirectional Communication**: Support `blob_store` and `blob_get` calls back to runtime

### Python SDK Example

```python
from stepflow_sdk import StepflowStdioServer

server = StepflowStdioServer()

@server.component
def my_component(input: MyInput) -> MyOutput:
    # Component logic here
    return MyOutput(result="processed")

server.run()
```

### TypeScript SDK Example

```typescript
import { StepflowStdioServer } from '@stepflow/sdk';

const server = new StepflowStdioServer();

server.component('my_component', {
  inputSchema: { /* JSON Schema */ },
  outputSchema: { /* JSON Schema */ },
  execute: async (input) => {
    // Component logic here
    return { result: "processed" };
  }
});

server.run();
```

## Best Practices

### Configuration Management

- **Environment-specific configs**: Use different configs for dev/staging/production
- **Secret management**: Store sensitive data in environment variables, not config files
- **Version control**: Check in config files (without secrets) for reproducibility
- **Documentation**: Document custom plugins and their requirements

### Plugin Organization

- **Single responsibility**: Create focused plugins for specific domains
- **Stable interfaces**: Use semantic versioning for plugin APIs
- **Error handling**: Implement robust error handling in custom components
- **Testing**: Include test cases for all custom components

### State Store Selection

- **Development**: Use in-memory for fast iteration
- **Testing**: Use SQLite for persistence during testing
- **Production**: Use SQLite for single-node, PostgreSQL for multi-node
- **Backup**: Regular backups of SQLite files in production

### Performance Optimization

- **Connection pooling**: Configure appropriate max_connections for state store
- **Plugin lifecycle**: Reuse plugin processes across workflow executions
- **Resource limits**: Set appropriate resource limits for component servers
- **Monitoring**: Monitor plugin performance and resource usage

## Troubleshooting

### Common Issues

#### Plugin Not Found
```
Error: Plugin 'python' not found
```
- Check plugin name spelling in workflow and config
- Verify config file is being loaded correctly
- Use `stepflow list-components` to see available components

#### Component Server Failed to Start
```
Error: Failed to initialize plugin 'python'
```
- Check command path and arguments
- Verify dependencies are installed
- Check working directory and environment variables
- Review component server logs

#### State Store Connection Failed
```
Error: Failed to connect to state store
```
- Verify database URL format
- Check file permissions for SQLite files
- Ensure database server is running (for PostgreSQL)
- Check connection limits and timeouts

### Debugging Configuration

Use these commands to debug configuration issues:

```bash
# List all available components
stepflow list-components --config=my-config.yml

# Show detailed component information
stepflow list-components --format=json --schemas=true

# Test specific workflow with config
stepflow run --flow=test.yaml --input=test.json --config=debug-config.yml
```

### Logging

Enable debug logging to troubleshoot configuration issues:

```bash
stepflow run --log-level=debug --flow=workflow.yaml --input=input.json
```

This will show:
- Config file resolution
- Plugin initialization
- Component discovery
- State store connection details