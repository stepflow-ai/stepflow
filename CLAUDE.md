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

1. **Workflow Definition** (`stepflow-workflow`): 
   - Defines the Rust structs representing a workflow
   - Handles flow structure, steps, components, and value expressions

2. **Workflow Compiler** (`stepflow-compiler`):
   - Not implemented yet.
   - Validates and compiles workflows before execution
   - Ensures workflows are well-formed and components exist
   - Annotates workflows with runtime information such as number of uses
     to facilitate early cancellation / deletion, etc.

3. **Execution Engine** (`stepflow-execution`):
   - Runs the core workflow execution logic
   - Handles parallel execution, error handling, and state management
   - Provides built-in control flow steps

4. **Plugin System** (`stepflow-plugin`, `stepflow-plugin-protocol`):
   - Manages extensible step services
   - Handles communication with external plugins

5. **Protocol** (`stepflow-protocol`):
   - Defines the JSON-RPC protocol for component communication
   - Uses structs and serde for serialization/deserialization

6. **CLI & Service** (`stepflow-main`):
   - Provides the main binary for executing workflows
   - Supports running workflows locally or as a service

### Data Flow

1. Workflows are defined in YAML/JSON files
2. The compiler validates the workflow structure
3. The execution engine processes the workflow:
   - Instantiates the required plugins
   - Executes steps in parallel when possible
   - Manages state and data flow between steps
4. Steps are executed by plugins, which can be:
   - Built-in Rust plugins
   - External services using JSON-RPC protocol
   - Python, TypeScript, or other language SDKs

### Key Concepts

1. **Flow**: A complete workflow definition with steps, inputs, and outputs
2. **Step**: A single operation within a workflow
3. **Component**: A specific implementation that a step invokes
4. **Plugin**: A service that provides one or more components
5. **Value**: Data that flows between steps, with references to input or other steps

### Error Handling

- Each crate has its own `error.rs` module with custom error types
- Uses `error-stack` for rich error context and propagation
- Uses `thiserror` for defining error enums

## Configuration

The `stepflow-config.yml` file defines plugins available to the workflow executor:

```yaml
plugins:
  - name: python  # Plugin identifier
    type: stdio   # Communication method (stdio or http)
    command: uv   # Command to execute
    args: ["--project", "../../sdks/python", "run", "stepflow_sdk"]  # Arguments
```