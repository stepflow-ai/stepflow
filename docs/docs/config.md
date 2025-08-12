---
sidebar_position: 7
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
    transport: stdio
    command: uv
    args: ["--project", "../sdks/python", "run", "stepflow_py"]
  remote_python:
    type: stepflow
    transport: http
    url: "http://localhost:8080"
  custom_server:
    type: stepflow
    transport: stdio
    command: "./my-component-server"
    args: ["--config", "server.json"]

routes:
  "/python/{*component}":
    - plugin: python
  "/custom/{*component}":
    - plugin: custom_server
  "/{*component}":
    - plugin: builtin

stateStore:
  type: sqlite
  databaseUrl: "sqlite:workflow_state.db"
  autoMigrate: true
  maxConnections: 10
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

#### STDIO Plugins

External component servers that communicate via JSON-RPC over stdin/stdout:

```yaml
plugins:
  python:
    type: stepflow
    transport: stdio
    command: uv
    args: ["--project", "${PROJECT_DIR:-../sdks/python}", "run", "stepflow_py"]
    working_directory: "."  # optional, defaults to current directory
    env:                    # optional environment variables with substitution support
      PYTHONPATH: "${HOME}/custom/path"
      DEBUG: "true"
      USER_CONFIG: "${USER:-anonymous}"
      WORKSPACE: "${HOME}/projects/${USER}"
```

**Parameters:**
- **`type`**: Must be `stepflow`
- **`transport`**: Set to `stdio` (default if not specified)
- **`command`**: Executable to run
- **`args`**: Command-line arguments
- **`working_directory`** (optional): Working directory for the command
- **`env`** (optional): Environment variables to set

**Environment Variable Substitution:**
Environment variables in both `args` and `env` sections support shell-like substitution:
- **Basic substitution**: `${VAR}` - expands to the value of environment variable `VAR`
- **Default values**: `${VAR:-default}` - uses `default` if `VAR` is not set
- **Nested substitution**: `${HOME}/projects/${USER}` - combines multiple variables
- **Substitution timing**: Variables are resolved when the plugin is launched using the current process environment
- **Error handling**: If a variable is not found and no default is provided, plugin initialization will fail
- **Applies to**: Both command arguments (`args`) and environment variables (`env`)

#### HTTP Plugins

HTTP-based component servers for distributed architectures:

```yaml
plugins:
  remote_server:
    type: stepflow
    transport: http
    url: "http://localhost:8080"
    timeout: 30  # optional, request timeout in seconds
```

**Parameters:**
- **`type`**: Must be `stepflow`
- **`transport`**: Set to `http`
- **`url`**: Base URL of the HTTP component server
- **`timeout`** (optional): Request timeout in seconds [default: 30]

**Features:**
- Distributed component servers running as independent HTTP services
- Optional MCP-style session negotiation for connection isolation
- Automatic fallback for servers that don't support session negotiation
- Scalable architecture suitable for production deployments

## Routing Configuration

**Routing rules are required** for components to be accessible. StepFlow uses routing rules to map component paths to specific plugins.

### Basic Routing

```yaml
routes:
  "/python/{*component}":
    - plugin: python
  "/remote/{*component}":
    - plugin: remote_python
  "/filesystem/{*component}":
    - plugin: filesystem
  "/{*component}":
    - plugin: builtin
```

- **Path patterns**: Component path patterns with placeholders like `{component}` or `{*component}`
- **`plugin`**: Plugin name to route to (must match a key in the plugins section)
- Rules are evaluated in order, first match wins
- Use `/{*component}` as a catch-all pattern for fallback routing

### Advanced Routing with Input Conditions

```yaml
routes:
  "/ai/chat":
    - conditions:
        - path: "$.model"
          value: "gpt-4"
      plugin: openai_plugin
    - plugin: fallback_plugin
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

The Python SDK provides a convenient way to create components with both STDIO and HTTP transports:

**STDIO Mode (Default):**
```yaml
plugins:
  python:
    type: stepflow
    transport: stdio
    command: uv
    args: ["--project", "path/to/python/project", "run", "stepflow_py"]
```

**HTTP Mode:**
```yaml
plugins:
  python_http:
    type: stepflow
    transport: http
    url: "http://localhost:8080"
```

Start the Python SDK server in HTTP mode:
```bash
uv run --project path/to/python/project --extra http python -m stepflow_py --http --port 8080
```

Both modes enable components like:
- `/python/udf` - Execute user-defined Python functions
- Custom components defined in your Python project

### MCP Integration

Model Context Protocol (MCP) servers can be used as component plugins:

```yaml
plugins:
  filesystem:
    type: mcp
    command: npx
    args: ["-y", "@modelcontextprotocol/server-filesystem", "${HOME}/workspace"]
    env:
      MCP_LOG_LEVEL: "${LOG_LEVEL:-info}"
      MCP_CONFIG_DIR: "${HOME}/.config/mcp"
      MCP_WORKSPACE: "${WORKSPACE_DIR:-${HOME}/workspace}"
```

**MCP Plugin Features:**
- **Environment Variable Substitution**: Same substitution syntax as StepFlow plugins
- **Command Arguments**: Environment variables can be used in both `args` and `env` fields
- **Tool Access**: MCP tools are accessed with format: `/server/tool_name`
- **Process Management**: MCP servers run as separate processes with isolated environments
- **Args Substitution**: Command line arguments support full environment variable substitution

## State Store Configuration

StepFlow supports multiple backends for storing workflow state and blobs.

### In-Memory State Store (Default)

No configuration needed - this is the default:

```yaml
# Optional explicit configuration
stateStore:
  type: inMemory
```

**Characteristics:**
- Fast access
- No persistence
- Suitable for testing and development
- All data lost when process exits

### SQLite State Store

File-based SQLite database for persistent storage:

```yaml
stateStore:
  type: sqlite
  databaseUrl: "sqlite:workflow_state.db"
  autoMigrate: true
  maxConnections: 10
```

**Parameters:**
- **`databaseUrl`**: SQLite connection string
  - File path: `"sqlite:path/to/database.db"`
  - In-memory: `"sqlite::memory:"`
- **`autoMigrate`** (optional): Automatically create/update database schema [default: true]
- **`maxConnections`** (optional): Connection pool size [default: 10]

**Characteristics:**
- Persistent storage
- File-based, no server required
- Suitable for single-node deployments
- Automatic schema migration

### PostgreSQL State Store (Future)

Enterprise-grade persistent storage (planned feature):

```yaml
stateStore:
  type: postgresql
  databaseUrl: "postgresql://user:password@localhost/stepflow"
  maxConnections: 20
  autoMigrate: true
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
    transport: stdio
    command: uv
    args: ["--project", ".", "run", "stepflow_py"]
    env:
      DEBUG: "true"
      PYTHONPATH: "${HOME}/dev/python-libs"
      USER_CONFIG: "${USER:-dev}"

routes:
  "/python/{*component}":
    - plugin: python
  "/{*component}":
    - plugin: builtin

stateStore:
  type: inMemory
```

### Production Configuration

```yaml
# stepflow-config.yml for production
plugins:
  builtin:
    type: builtin
  python:
    type: stepflow
    transport: stdio
    command: python
    args: ["-m", "stepflow_py"]
    working_directory: "/app/components"
    env:
      PYTHONPATH: "${APP_ROOT}/lib"
      LOG_LEVEL: "${LOG_LEVEL:-info}"
  remote_ai:
    type: stepflow
    transport: http
    url: "http://ai-service:8080"
    timeout: 60
  data_processing:
    type: stepflow
    transport: http
    url: "http://data-processor:8081"
  mcp_filesystem:
    type: mcp
    command: npx
    args: ["-y", "@modelcontextprotocol/server-filesystem", "${DATA_DIR:-/app/data}"]
    env:
      MCP_LOG_LEVEL: "${LOG_LEVEL:-info}"
      MCP_CONFIG_DIR: "${CONFIG_DIR:-/app/config}"

routes:
  "/python/{*component}":
    - plugin: python
  "/ai/{*component}":
    - plugin: remote_ai
  "/data/{*component}":
    - plugin: data_processing
  "/filesystem/{*component}":
    - plugin: mcp_filesystem
  "/{*component}":
    - plugin: builtin

stateStore:
  type: sqlite
  databaseUrl: "sqlite:/var/lib/stepflow/state.db"
  autoMigrate: true
  maxConnections: 20
```

### Multi-Service Configuration

```yaml
# stepflow-config.yml for multi-service setup
plugins:
  builtin:
    type: builtin
  analytics:
    type: stepflow
    transport: http
    url: "http://analytics-service:8080"
    timeout: 120
  ml_models:
    type: stepflow
    transport: stdio
    command: python
    args: ["-m", "ml_components"]
    env:
      MODEL_PATH: "${MODEL_PATH:-/models}"
      CUDA_VISIBLE_DEVICES: "${CUDA_DEVICES:-0}"
      HF_HOME: "${HOME}/.cache/huggingface"
  external_apis:
    type: stepflow
    transport: http
    url: "http://api-gateway:8082"

routes:
  "/analytics/{*component}":
    - plugin: analytics
  "/ml/{*component}":
    - plugin: ml_models
  "/external/{*component}":
    - plugin: external_apis
  "/{*component}":
    - plugin: builtin

stateStore:
  type: sqlite
  databaseUrl: "sqlite:/shared/storage/workflows.db"
  autoMigrate: true
  maxConnections: 50
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

### Environment Variable Substitution in Configuration

Plugin configurations support environment variable substitution in the `env` section:

```yaml
plugins:
  python:
    type: stepflow
    transport: stdio
    command: python
    env:
      # Basic substitution
      PYTHONPATH: "${HOME}/custom/path"
      # Default values
      LOG_LEVEL: "${LOG_LEVEL:-info}"
      # Nested substitution
      CONFIG_FILE: "${HOME}/.config/${USER}/settings.json"
```

**Substitution Syntax:**
- `${VAR}` - Substitutes the value of environment variable `VAR`
- `${VAR:-default}` - Uses `default` if `VAR` is not set or empty
- Variables are resolved from the current process environment when the plugin is launched
- Substitution failures will prevent plugin initialization

## Plugin Development

### Creating Component Servers

Component servers can be created using either STDIO or HTTP transports:

**STDIO Transport Requirements:**
1. **Implement JSON-RPC Protocol**: Handle `initialize`, `component_info`, and `component_execute` methods
2. **Define Component Schemas**: Provide input/output schemas via `component_info`
3. **Handle Bidirectional Communication**: Support `blob_store` and `blob_get` calls back to runtime
4. **Process Management**: Handle subprocess lifecycle and stdio communication

**HTTP Transport Requirements:**
1. **Implement HTTP Server**: Serve JSON-RPC over HTTP POST requests
2. **Optional Session Support**: Implement MCP-style session negotiation with SSE
3. **Handle Bidirectional Communication**: Support `blob_store` and `blob_get` calls back to runtime
4. **Service Management**: Handle HTTP service lifecycle and request routing

### Python SDK Example

**STDIO Mode:**
```python
from stepflow_py import StepflowStdioServer

server = StepflowStdioServer()

@server.component
def my_component(input: MyInput) -> MyOutput:
    # Component logic here
    return MyOutput(result="processed")

server.run()
```

**HTTP Mode:**
```python
from stepflow_py import StepflowServer
import uvicorn

server = StepflowServer()

@server.component
def my_component(input: MyInput) -> MyOutput:
    # Component logic here
    return MyOutput(result="processed")

# Create FastAPI app and run HTTP server
app = server.create_http_app()
uvicorn.run(app, host="0.0.0.0", port=8080)
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
- **Transport selection**: Use HTTP transport for distributed deployments and better scalability
- **Session management**: Configure MCP-style session negotiation for HTTP transport isolation
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
**For STDIO Transport:**
- Check command path and arguments
- Verify dependencies are installed
- Check working directory and environment variables
- Review component server logs

**For HTTP Transport:**
- Verify server is running and accessible at the configured URL
- Check network connectivity and firewall settings
- Verify server supports the StepFlow protocol
- Check server logs for initialization errors

#### State Store Connection Failed
```
Error: Failed to connect to state store
```
- Verify database URL format
- Check file permissions for SQLite files
- Ensure database server is running (for PostgreSQL)
- Check connection limits and timeouts

#### HTTP Transport Connection Issues
```
Error: HTTP request failed or timed out
```
- Verify the component server is running on the specified URL
- Check network connectivity between StepFlow and the server
- Increase timeout values if requests are timing out
- Verify the server supports JSON-RPC over HTTP POST
- Check if MCP-style session negotiation is working correctly
- Review server logs for HTTP-specific errors

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