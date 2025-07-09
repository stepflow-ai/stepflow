# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Development Commands

### Building the Project
```bash
# Build the entire project (run from stepflow-rs directory)
cd stepflow-rs
cargo build

# Build with release optimizations
cargo build --release
```

### Running Tests
```bash
# Run all tests (run from stepflow-rs directory)
cd stepflow-rs
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
# Run a workflow with input file (run from stepflow-rs directory)
cd stepflow-rs
cargo run -- run --flow=examples/python/basic.yaml --input=examples/python/input1.json

# Run a workflow with inline JSON input
cargo run -- run --flow=examples/python/basic.yaml --input-json='{"m": 3, "n": 4}'

# Run a workflow with inline YAML input
cargo run -- run --flow=examples/python/basic.yaml --input-yaml='m: 2\nn: 7'

# Run a workflow reading from stdin (JSON format)
echo '{"m": 1, "n": 2}' | cargo run -- run --flow=examples/python/basic.yaml --format=json

# Run with a custom config file
cargo run -- run --flow=<flow.yaml> --input=<input.json> --config=<stepflow-config.yml>

# Run a workflow service
cargo run -- serve --port=7837 --config=<stepflow-config.yml>

# Submit a workflow to a running service
cargo run -- submit --url=http://localhost:7837/api/v1 --flow=<flow.yaml> --input=<input.json>
```

### Code Linting
```bash
# Run clippy on all crates (run from stepflow-rs directory)
cd stepflow-rs
cargo clippy

# Auto-fix linting issues where possible
cargo clippy --fix

# Format code
cargo fmt

# Check compilation without building
cargo check

# Check compilation for a specific crate
cargo check -p stepflow-protocol
```

### Python SDK Development
```bash
# Test Python SDK
uv run --project sdks/python pytest
```

### UI Development (stepflow-ui)
```bash
# Run development server
pnpm dev

# Install dependencies
pnpm install

# Run tests
pnpm test

# Build for production
pnpm build

# Run linting
pnpm lint

# Run type checking
pnpm type-check
```

### Schema Generation
When making changes to core types, you may need to regenerate schemas and derived code:

```bash
# Regenerate JSON schemas from Rust types (run from stepflow-rs directory)
cd stepflow-rs
STEPFLOW_OVERWRITE_SCHEMA=1 cargo test

# Regenerate Python types from updated schemas
cd sdks/python
uv run python generate.py

# Check if Python types are up to date without regenerating
cd sdks/python
uv run python generate.py --check
```

**Important**: Always regenerate schemas after modifying the workflow and protocol types (in `stepflow-core` and `stepflow-protocol`, respectively). The schema files are used by:
- Python SDK type generation
- Documentation and examples
- API validation in server components

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
   - Supports bidirectional communication allowing components to send requests to the runtime

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
6. **Blob**: Persistent JSON data storage with content-based IDs (SHA-256 hashes)
7. **Context**: Runtime environment provided to components for system calls

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

#### Error Handling Best Practices

- Define custom error types in `error.rs` at the crate root (or in appropriate modules)
- Include a `type Result<T, E = TheErrorType> = std::result::Result<T, E>` alias
- Use `thiserror` for defining error types
- Include context in error messages
- Document error variants and their meanings

## Code Style & Best Practices

### Rust Code Standards

- Use `rustfmt` for consistent code formatting (run `cargo fmt`)
- Use `clippy` for linting (run `cargo clippy`)
- Follow the Rust API Guidelines (https://rust-lang.github.io/api-guidelines/)
- Maximum line length: 100 characters

### Module Organization

- Prefer `foo.rs` files over `foo/mod.rs` for module organization
- When creating a module directory, create a corresponding `.rs` file in the parent directory with the same name
- Example: Instead of `workflow/mod.rs`, create `workflow.rs` at the same level as the `workflow/` directory

### Performance Guidelines

#### Avoid Unnecessary Cloning
- Use `Arc<T>` for shared expensive-to-clone data structures (e.g., `Arc<Flow>`, `Arc<dyn StateStore>`)
- Prefer references over clones when passing data to async tasks
- Use `Cow<'static, str>` for string data that may be borrowed or owned
- Example: Instead of `step.clone()` in async tasks, pass `Arc<Flow>` and use step indices

#### Async Patterns
- Use `async fn` consistently for I/O operations
- Prefer `tokio::spawn` for concurrent execution
- Use `futures::future::BoxFuture` for trait objects returning futures instead of `Pin<Box<dyn Future>>`
- Always import `futures::future::FutureExt as _` when using `.boxed()`
- Example:
```rust
use futures::future::{BoxFuture, FutureExt as _};

// Trait method signature
fn my_async_method(&self) -> BoxFuture<'_, Result<String, Error>>;

// Implementation
fn my_async_method(&self) -> BoxFuture<'_, Result<String, Error>> {
    async move {
        // async logic here
        Ok("result".to_string())
    }.boxed()
}
```

### Documentation

- Use `///` for public API documentation
- Use `//!` for module-level documentation
- Include examples in documentation where appropriate
- Document all public types, functions, and traits
- Use markdown in documentation comments

### Testing

- Place unit tests in the same file as the code they test
- Use `#[test]` for unit tests
- Use `#[cfg(test)]` for test modules
- Follow the pattern: `mod tests { ... }`
- Place integration tests in the `tests/` directory
- Test both success and failure cases

### Git Commit Messages

- Use conventional commit prefixes:
  - `feat:` for new features
  - `fix:` for bug fixes
  - `docs:` for documentation changes
  - `style:` for formatting changes
  - `refactor:` for code refactoring
  - `test:` for adding or modifying tests
  - `chore:` for maintenance tasks
- Use present tense ("Add feature" not "Added feature")
- Start with a capital letter
- Keep the first line under 50 characters

### Logging and Tracing

- Use the `tracing` package for all logging and instrumentation
- Use appropriate log levels (error, warn, info, debug, trace)
- Include relevant context in log messages
- Use structured logging where appropriate
- Use spans for tracking operation context
- Use events for discrete log messages

### Error Handling Patterns

#### Dual Error System
StepFlow uses a dual error approach to distinguish between business logic and system failures:

1. **FlowError**: Business logic failures that are part of normal workflow execution
   - Used for validation failures, missing data, expected conditions
   - Allow workflows to continue and handle errors gracefully
   - Example: `FlowResult::Failed { error: FlowError::new(400, "Invalid input") }`

2. **System Errors**: Implementation or infrastructure failures
   - Used for plugin communication failures, serialization errors, etc.
   - Represent unexpected conditions that should halt execution
   - Each crate has its own `error.rs` module with custom error types
   - Uses `error-stack` for rich error context and `thiserror` for error enums

#### Advanced Error Patterns
- Use `FlowError` for expected business failures
- Use `Result<T, error_stack::Report<YourError>>` for system failures
- Add context with `error_stack::ResultExt::attach_printable`
- Define custom error types with `thiserror::Error`

### Derive Patterns

Use consistent derive patterns based on type purpose:

- **Core workflow types**: `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]`
- **Error types**: `#[derive(Error, Debug)]` (with `thiserror`)
- **Configuration types**: `#[derive(Serialize, Deserialize)]` with serde attributes
- **CLI types**: `#[derive(clap::Parser)]`

**Note**: Avoid `Clone` on large structs like `Step` unless absolutely necessary. Use `Arc` references instead.

## Configuration

The `stepflow-config.yml` file defines plugins and state storage available to the workflow executor:

### Plugin Configuration

```yaml
plugins:
  - name: builtin  # Built-in components
    type: builtin
  - name: python   # Plugin identifier
    type: stdio    # Communication method (stdio or http)
    command: uv    # Command to execute
    args: ["--project", "../sdks/python", "run", "stepflow_sdk"]  # Arguments
```

### Plugin Types

- **builtin**: Built-in components (OpenAI, etc.)
- **stdio**: JSON-RPC over stdio communication
- **http**: JSON-RPC over HTTP (future feature)

### State Store Configuration

StepFlow supports multiple state storage backends for persisting workflow execution state and blobs:

#### In-Memory State Store (Default)
```yaml
# No configuration needed - this is the default
state_store:
  type: in_memory
```

#### SQLite State Store
```yaml
state_store:
  type: sqlite
  database_url: "sqlite:workflow_state.db"  # File path or ":memory:"
  auto_migrate: true                        # Automatically create/update schema
  max_connections: 10                       # Connection pool size
```

### Project Dependencies

- Keep dependencies minimal and well-documented
- Use workspace dependencies where appropriate
- Document the purpose of each dependency
- Keep dependencies up to date

## Testing

### Test Organization

#### Unit Tests
- Located inline with source files using `#[cfg(test)]` modules
- Use descriptive test names: `test_<feature>_<scenario>`

#### Integration Tests
- **Crate-level integration tests**: Located in `<crate>/tests/` directories (e.g., `crates/stepflow-main/tests/`)
- **Workflow-level integration tests**: Located in `/tests/` at the project root, using actual workflow YAML files

#### Snapshot Testing
Tests in `crates/stepflow-main/tests/test_run.rs` use `insta` to perform snapshot testing.

Each file in `crates/stepflow-main/tests/flows` contains a stepflow_config, a workflow, and a sequence of test cases.
Adding new tests can be done by either adding a case using an existing test workflow, or creating a new file with new configuration, workflow and test cases.

**Snapshot Testing Commands:**
* `cargo insta test --unreferenced=delete` will run all the tests (including the insta tests) and delete unused snapshots.
* `cargo insta pending-snapshots` produces a list of pending snapshots
* `cargo insta show <path>` shows a specific snapshot
* `cargo insta accept` will accept all of the snapshots.

## Bidirectional Communication & Blob Support

The protocol supports bidirectional communication allowing component servers to make calls back to the stepflow runtime.

### Protocol Methods

**From stepflow to components:**
- `initialize`: Initialize the component server
- `component_info`: Get component schema information
- `component_execute`: Execute a component with input data

**From components to stepflow:**
- `blob_store`: Store JSON data as a blob, returns content-based ID
- `blob_get`: Retrieve JSON data by blob ID

### Python SDK Usage

Components can receive a `StepflowContext` parameter to access runtime operations:

```python
from stepflow_sdk import StepflowStdioServer, StepflowContext
import msgspec

class MyInput(msgspec.Struct):
    data: dict

class MyOutput(msgspec.Struct):
    blob_id: str

server = StepflowStdioServer()

@server.component
def my_component(input: MyInput, context: StepflowContext) -> MyOutput:
    # Store data as a blob
    blob_id = await context.put_blob(input.data)
    return MyOutput(blob_id=blob_id)

server.run()
```
