# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Development Commands

### Building the Project
```bash
# Build the entire project
cargo build

# Build with release optimizations
cargo build --release
```

### Running Tests
```bash
# Run all tests
cargo test

# Run with snapshot tests and automatic snapshot updates
cargo insta test --unreferenced=delete --review

# Run tests for a specific crate
cargo test -p stepflow-execution

# Run a specific test
cargo test -p stepflow-execution -- execute_flows
```

### Running the App
```bash
# Run a workflow with input
cargo run -- run --flow=examples/python/basic.yaml --input=examples/python/input1.json

# Run with a custom config file
cargo run -- run --flow=<flow.yaml> --input=<input.json> --config=<stepflow-config.yml>

# Run a workflow service
cargo run -- serve --port=8080 --config=<stepflow-config.yml>

# Submit a workflow to a running service
cargo run -- submit --url=http://localhost:8080 --flow=<flow.yaml> --input=<input.json>
```

### Code Linting
```bash
# Run clippy on all crates
cargo clippy

# Auto-fix linting issues where possible
cargo clippy --fix

# Format code
cargo fmt
```

## Project Architecture

Step Flow is an execution engine for AI workflows, built in Rust. The project is structured as a workspace with multiple crates, each serving a specific purpose.

### Core Components

1. **Core Types** (`stepflow-core`): 
   - Defines the Rust structs representing workflows, components, and value expressions
   - Handles flow structure, steps, components, and schema definitions
   - Contains the `FlowResult` type for workflow execution results

2. **Execution Engine** (`stepflow-execution`):
   - Runs the core workflow execution logic
   - Handles parallel execution, error handling, and state management
   - Provides workflow state management and execution coordination

3. **Plugin System** (`stepflow-plugin`):
   - Manages extensible component services
   - Handles communication with external plugins
   - Provides the core plugin trait and plugin management

4. **Built-in Components** (`stepflow-builtins`):
   - Provides built-in component implementations
   - Includes OpenAI API integration and other core components
   - Implements the plugin trait for built-in functionality

5. **Protocol** (`stepflow-protocol`):
   - Defines the JSON-RPC protocol for component communication
   - Uses structs and serde for serialization/deserialization
   - Implements stdio-based communication with sub-processes

6. **MCP Integration** (`stepflow-components-mcp`):
   - Provides integration with Model Context Protocol (MCP) tools
   - Allows workflows to use MCP-compatible components

7. **CLI & Service** (`stepflow-main`):
   - Provides the main binary for executing workflows
   - Supports running workflows locally or as a service
   - Includes CLI commands for run, serve, and submit operations

8. **Testing Support** (`stepflow-mock`):
   - Provides mock implementations for testing
   - Facilitates unit testing of workflow components

### Data Flow

1. Workflows are defined in YAML/JSON files
2. The workflow structure is parsed and validated using `stepflow-core` types
3. The execution engine processes the workflow:
   - Instantiates the required plugins
   - Executes steps in parallel when possible
   - Manages state and data flow between steps
4. Steps are executed by components, which can be:
   - Built-in components from `stepflow-builtins`
   - External services using JSON-RPC protocol via `stepflow-protocol`
   - MCP tools via `stepflow-components-mcp`
   - Python, TypeScript, or other language SDKs

### Key Concepts

1. **Flow**: A complete workflow definition with steps, inputs, and outputs
2. **Step**: A single operation within a workflow
3. **Component**: A specific implementation that a step invokes
4. **Plugin**: A service that provides one or more components
5. **Value**: Data that flows between steps, with references to input or other steps

### Error Handling

Step Flow distinguishes between two types of errors:

1. **Flow Errors** (`FlowError`): Business logic failures that are part of normal workflow execution
   - Represented by the `FlowError` type with error codes and messages
   - Used for validation failures, missing data, or expected failure conditions
   - Allow workflows to continue and handle errors gracefully

2. **System Errors** (`Result::Err`): Implementation or infrastructure failures
   - Used for plugin communication failures, serialization errors, etc.
   - Represent unexpected conditions that should halt execution
   - Each crate has its own `error.rs` module with custom error types
   - Uses `error-stack` for rich error context and propagation
   - Uses `thiserror` for defining error enums

The `FlowResult` enum enables proper error propagation through workflow execution:
- `Success(ValueRef)`: Step completed successfully with output
- `Skipped`: Step was conditionally skipped
- `Failed(FlowError)`: Step failed with a business logic error

## Code Style

### Module Organization

- Prefer `foo.rs` files over `foo/mod.rs` for module organization
- When creating a module directory, create a corresponding `.rs` file in the parent directory with the same name
- Example: Instead of `workflow/mod.rs`, create `workflow.rs` at the same level as the `workflow/` directory

## Configuration

The `stepflow-config.yml` file defines plugins available to the workflow executor:

```yaml
plugins:
  - name: python  # Plugin identifier
    type: stdio   # Communication method (stdio or http)
    command: uv   # Command to execute
    args: ["--project", "../../sdks/python", "run", "stepflow_sdk"]  # Arguments
```