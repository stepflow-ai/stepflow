# StepFlow Rust Engine

The core Rust-based execution engine and runtime for StepFlow workflows.

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
cargo run -- run --flow=examples/python/basic.yaml --input=examples/python/input1.json

# Run with inline JSON input
cargo run -- run --flow=examples/python/basic.yaml --input-json='{"m": 3, "n": 4}'

# Run with inline YAML input
cargo run -- run --flow=examples/python/basic.yaml --input-yaml='m: 2\nn: 7'

# Run reading from stdin (JSON format)
echo '{"m": 1, "n": 2}' | cargo run -- run --flow=examples/python/basic.yaml --format=json

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

StepFlow supports flexible plugin configuration with environment variable substitution:

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
- Works with both StepFlow and MCP plugins
- Applies to both command arguments (`args`) and environment variables (`env`)

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

For comprehensive documentation, examples, and guides, visit the [StepFlow Documentation](https://fuzzy-journey-4j3y1we.pages.github.io/).

See also:
- [CLAUDE.md](../CLAUDE.md) - Development guide for contributors
- [Project README](../README.md) - Main project overview