# Stepflow Rust Engine

The core Rust-based execution engine and runtime for Stepflow workflows.

## Quick Start

### Building

```bash
# Build the project
cargo build

# Build with optimizations
cargo build --release
```

### Testing

```bash
# Run fast unit tests (no external dependencies)
cargo test

# Run tests for a specific crate
cargo test -p stepflow-execution

# Run a specific test
cargo test -p stepflow-execution -- execute_flows

# Run with snapshot tests and automatic snapshot updates
cargo insta test --unreferenced=delete --review
```

For integration tests that require external dependencies (Python environment), use the scripts in the project root:

```bash
# Run integration tests (requires Python environment)
cd ..
./scripts/test-integration.sh

# Run complete test suite (unit + Python SDK + integration)
./scripts/test-all.sh
```

### Running Workflows

```bash
# Run a workflow with input file
cargo run -- run --flow=../examples/basic/workflow.yaml --input=../examples/basic/input1.json --config=../examples/basic/stepflow-config.yml

# Run with inline JSON input
cargo run -- run --flow=../examples/basic/workflow.yaml --input-json='{"m": 3, "n": 4}' --config=../examples/basic/stepflow-config.yml

# Run with inline YAML input
cargo run -- run --flow=../examples/basic/workflow.yaml --input-yaml='m: 2\nn: 7' --config=../examples/basic/stepflow-config.yml

# Run reading from stdin (JSON format)
echo '{"m": 1, "n": 2}' | cargo run -- run --flow=../examples/basic/workflow.yaml --format=json --config=../examples/basic/stepflow-config.yml

# Run with variables from file
cargo run -- run --flow=workflow.yaml --input=input.json --variables=variables.json

# Run with inline JSON variables
cargo run -- run --flow=workflow.yaml --input=input.json --variables-json='{"api_key": "test-key", "temperature": 0.8}'

# Run with inline YAML variables
cargo run -- run --flow=workflow.yaml --input=input.json --variables-yaml='api_key: test-key\ntemperature: 0.8'

# Run with environment variable fallback for missing variables
cargo run -- run --flow=workflow.yaml --input=input.json --env-variables

# Run with a custom config file
cargo run -- run --flow=<flow.yaml> --input=<input.json> --config=<stepflow-config.yml>
```

### Running as a Service

```bash
# Start workflow service
cargo run -- serve --port=7837 --config=<stepflow-config.yml>

# Submit a workflow to running service
cargo run -- submit --url=http://localhost:7837/api/v1 --flow=<flow.yaml> --input=<input.json>
```

### Code Quality

```bash
# Run linting
cargo clippy

# Auto-fix linting issues where possible
cargo clippy --fix

# Format code
cargo fmt

# Check compilation without building
cargo check
```

## Configuration

Stepflow supports flexible plugin configuration with environment variable substitution:

```yaml
# stepflow-config.yml
plugins:
  python:
    type: stepflow
    transport: stdio
    command: python
    args: ["--project", "${PROJECT_DIR:-../sdk}"]
    env:
      PYTHONPATH: "${HOME}/custom/path"
      USER_CONFIG: "${USER:-anonymous}"

  filesystem:
    type: mcp
    command: npx
    args: ["-y", "@modelcontextprotocol/server-filesystem", "${HOME}/workspace"]
    env:
      MCP_LOG_LEVEL: "${LOG_LEVEL:-info}"
```

**Environment Variable Features:**
- Shell-like substitution with `${VAR}` syntax
- Default values using `${VAR:-default}` syntax
- Nested substitution: `${HOME}/projects/${USER}`
- Works with both Stepflow and MCP plugins
- Applies to both command arguments (`args`) and environment variables (`env`)

## Variables and Secrets

Stepflow supports workflow variables that can be referenced in steps, enabling parameterized workflows and secure handling of sensitive data.

### Workflow Variables

Variables are declared at the flow level using JSON Schema format and referenced in steps using the `$from` syntax:

```yaml
# workflow.yaml
variables:
  type: object
  properties:
    api_key:
      type: string
      is_secret: true
      description: "OpenAI API key"
    temperature:
      type: number
      default: 0.7
      minimum: 0
      maximum: 2
    model:
      type: string
      default: "gpt-4"
  required: ["api_key"]

steps:
  - id: chat
    component: "/builtin/openai"
    input:
      api_key: { $from: { variable: api_key } }
      temperature: { $from: { variable: temperature } }
      model: { $from: { variable: model } }
      messages: { $from: { workflow: input }, path: "messages" }
```

### Providing Variable Values

Variables can be provided through multiple sources:

**From File:**
```bash
# variables.json
{
  "api_key": "sk-...",
  "temperature": 0.8
}

cargo run -- run --flow=workflow.yaml --variables=variables.json
```

**Inline JSON/YAML:**
```bash
# JSON format
cargo run -- run --flow=workflow.yaml --variables-json='{"api_key": "sk-...", "temperature": 0.8}'

# YAML format
cargo run -- run --flow=workflow.yaml --variables-yaml='api_key: sk-...\ntemperature: 0.8'
```

**Environment Variables:**
```bash
# Set environment variables
export STEPFLOW_VAR_API_KEY="sk-..."
export STEPFLOW_VAR_TEMPERATURE="0.8"

# Enable environment fallback
cargo run -- run --flow=workflow.yaml --env-variables
```

### Variable Features

- **Type Safety**: Variables are validated against JSON Schema definitions
- **Default Values**: Specify default values in the schema (single source of truth)
- **Required Variables**: Mark variables as required in the schema
- **Secret Handling**: Mark variables as `is_secret: true` for secure handling
- **Environment Fallback**: Use `STEPFLOW_VAR_<NAME>` pattern for missing variables
- **JSON Path Support**: Access nested variable properties with `{ $from: { variable: config }, path: "$.api.timeout" }`

### Security Considerations

Variables marked with `is_secret: true` are handled securely:
- Redacted in logs and traces as `[REDACTED]`
- Not displayed in error messages or debug output  
- Proper handling throughout the execution pipeline

**Example with secrets:**
```yaml
variables:
  type: object
  properties:
    database_url:
      type: string
      is_secret: true
      description: "Database connection string"
    debug_mode:
      type: boolean
      default: false
```

## Project Structure

This is a Rust workspace containing multiple crates:

- **`stepflow-core`** - Core types and workflow definitions
- **`stepflow-execution`** - Workflow execution engine
- **`stepflow-plugin`** - Plugin system and interfaces
- **`stepflow-protocol`** - JSON-RPC communication protocol
- **`stepflow-builtins`** - Built-in component implementations
- **`stepflow-components-mcp`** - MCP (Model Context Protocol) integration
- **`stepflow-main`** - CLI and service binaries
- **`stepflow-server`** - HTTP API server
- **`stepflow-state`** - State storage interfaces and implementations
- **`stepflow-state-sql`** - SQL-based state storage
- **`stepflow-analysis`** - Workflow analysis and validation
- **`stepflow-mock`** - Mock implementations for testing

## Documentation

For comprehensive documentation, examples, and guides, visit the [Stepflow Documentation](https://fuzzy-journey-4j3y1we.pages.github.io/).

See also:
- [CLAUDE.md](../CLAUDE.md) - Development guide for contributors
- [Project README](../README.md) - Main project overview