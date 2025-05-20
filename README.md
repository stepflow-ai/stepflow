# Step Flow

Step Flow is a powerful execution engine for AI workflows, built in Rust. It provides a flexible and scalable way to orchestrate complex AI workflows through a combination of built-in steps and extensible step services.
## Overview

Step Flow enables developers to:
- Define AI workflows using YAML or JSON
- Execute workflows with built-in support for parallel execution
- Extend functionality through step services
- Handle errors at both flow and step levels
- Use as both a library and a service

## Architecture

Most steps are defined in a step-service, which the executor invokes using a JSON-RPC protocol similar to the Language Server Protocol (LSP) or Model Context Protocol (MCP).

### Core Components

1. **Execution Engine**
   - Workflow parser and validator
   - Parallel execution support
   - Error handling and retry mechanisms
   - State management

2. **Step Services**
   - JSON-RPC based communication
   - Service discovery and registration
   - Built-in step implementations
   - Extensible service architecture

3. **Workflow Definition**
   - YAML/JSON based workflow specification
   - Support for parallel execution
   - Configurable error handling
   - Step service integration

### Organization

- `crates/stepflow-protocol` defines the JSON-RPC protocol using Rust structs and `serde`.
- `crates/stepflow-workflow` defines the Rust structs representing a workflow.
- `crates/stepflow-compile` defines the logic for validating and compiling workflows.
- `crates/stepflow-components` provides the trait for step execution plugins.
- `crates/stepflow-components-client` provides a step execution plugin using the stepflow protocol.
  It supports JSON-RPC over stdio (similar to LSP and MCP) and JSON-RPC over HTTP.
- `crates/stepflow-components-mcp` provides a step execution plugin for executing MCP tools.
- `crates/stepflow-execution` provides the core execution logic for a workflow. It also provides
  the built-in control flow steps.
- `crates/stepflow-main` provides the main binary for executing a workflow or running a stepflow service.

## Getting Started

*[Installation and usage instructions will be added as the project develops]*

## Development

This project is built in Rust and uses:
- `serde` for serialization/deserialization
- JSON-RPC for service communication
- Async runtime for parallel execution

Run tests with `cargo test` or `cargo insta test --unreferenced=delete --review`.
The latter runs uses `insta` to delete outdated snapshots and review any changes after the tests run.
Both commands will fail if any test fails, including if the snapshot output doesn't match the actual output.

## License

*[License information to be added]*
