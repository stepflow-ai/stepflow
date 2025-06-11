# Step Flow

StepFlow is an open protocol and runtime for building, executing, and scaling GenAI workflows across local and cloud environments. Its modular architecture ensures secure, isolated execution of components—whether running locally or deployed to production. With durability, fault-tolerance, and an open specification, StepFlow empowers anyone to create, share, and run AI workflows across platforms and tools.

- **⚙️ Reliable, Scalable Workflow Execution**
   Run workflows locally with confidence they’ll scale. StepFlow provides built-in durability and fault tolerance—ready for seamless transition to production-scale deployments.
- **🔐 Secure, Isolated Components**
   Each workflow step runs in a sandboxed process or container with strict resource and environment controls. StepFlow's design prioritizes security, reproducibility, and platform independence.
- **🌐 Open, Portable Workflow Standard**
   Build once, run anywhere. The StepFlow protocol is open and extensible, enabling workflow portability across different environments and platforms.

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
- `crates/stepflow-core` defines the Rust structs representing workflows, components, and value expressions.
- `crates/stepflow-plugin` provides the trait for component execution plugins and plugin management.
- `crates/stepflow-builtins` provides built-in component implementations including OpenAI integration.
- `crates/stepflow-components-mcp` provides a component plugin for executing MCP (Model Context Protocol) tools.
- `crates/stepflow-execution` provides the core execution logic for workflows with parallel execution support.
- `crates/stepflow-main` provides the main binary for executing workflows or running a stepflow service.
- `crates/stepflow-mock` provides mock implementations for testing purposes.

## Getting Started

*[Installation and usage instructions will be added as the project develops]*

### Build and Run

The easiest way to run a workflow is to run it locally.
To do this, you need to create a `stepflow-config.yaml`.
If you don't specify one, the CLI will attempt to locate one in the directory containing the workflow.

The following command builds and uses the CLI to run a workflow.

```sh
cargo run -- run --flow=<flow.yaml> --input=<input_path.json>
```

If you wish to build and run separately, you can use the following commands:

```sh
cargo build
./target/debug/stepflow-main run --flow=<flow.yaml> --input=<input_path.json>
```

## Development

This project is built in Rust and uses:
- `serde` for serialization/deserialization
- JSON-RPC for service communication
- Async runtime for parallel execution

Run tests with `cargo test` or `cargo insta test --unreferenced=delete --review`.
The latter runs uses `insta` to delete outdated snapshots and review any changes after the tests run.
Both commands will fail if any test fails, including if the snapshot output doesn't match the actual output.

### Cargo Deny

This project uses [cargo-deny](https://github.com/EmbarkStudios/cargo-deny) to check dependencies for security vulnerabilities and license compliance.
To run cargo-deny, use the following command:

```sh
cargo install --locked cargo-deny
cargo deny check
```

## License

StepFlow and it's components are licensed under the Apache License 2.0.
See the [LICENSE](LICENSE) file for details.