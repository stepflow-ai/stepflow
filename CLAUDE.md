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
# Fast unit tests only (no external dependencies required)
cd stepflow-rs
cargo test

# Run with snapshot tests and automatic snapshot updates
cargo insta test --unreferenced=delete --review

# Run tests for a specific crate
cargo test -p stepflow-execution

# Run a specific test
cargo test -p stepflow-execution -- execute_flows

# Integration tests (requires Python environment and stepflow binary)
./scripts/test-integration.sh

# Complete test suite (unit tests + Python SDK tests + integration tests)
./scripts/test-all.sh
```

### Running the App
```bash
# Run a workflow with input file (run from stepflow-rs directory)
cd stepflow-rs
cargo run -- run --flow=examples/basic/workflow.yaml --input=examples/basic/input1.json --config=examples/basic/stepflow-config.yml

# Run a workflow with inline JSON input
cargo run -- run --flow=examples/basic/workflow.yaml --input-json='{"m": 3, "n": 4}' --config=examples/basic/stepflow-config.yml

# Run a workflow with inline YAML input
cargo run -- run --flow=examples/basic/workflow.yaml --input-yaml='m: 2\nn: 7' --config=examples/basic/stepflow-config.yml

# Run a workflow reading from stdin (JSON format)
echo '{"m": 1, "n": 2}' | cargo run -- run --flow=examples/basic/workflow.yaml --format=json --config=examples/basic/stepflow-config.yml

# Run with a custom config file
cargo run -- run --flow=<flow.yaml> --input=<input.json> --config=<stepflow-config.yml>

# Run a workflow service
cargo run -- serve --port=7837 --config=<stepflow-config.yml>

# Submit a workflow to a running service
cargo run -- submit --url=http://localhost:7837/api/v1 --flow=<flow.yaml> --input=<input.json>
```

### Validating Workflows and Configuration
```bash
# Validate workflow and configuration files (run from stepflow-rs directory)
cd stepflow-rs
cargo run -- validate --flow ../examples/basic/workflow.yaml --config ../examples/basic/stepflow-config.yml

# Validate workflow with automatic config resolution (searches for stepflow-config.yml)
cargo run -- validate --flow ../examples/basic/workflow.yaml

# Validate workflow in current directory with local config
cargo run -- validate --flow my-workflow.yaml
```

**Validation Features:**
- **Workflow validation**: Uses `stepflow-analysis` to validate workflow structure, step dependencies, and value references
- **Configuration validation**: Checks plugin definitions, routing rules, and plugin-route consistency
- **Component routing validation**: Ensures all workflow components have valid routing to configured plugins
- **Schema validation**: Validates component schemas and input/output types when available
- **User-friendly output**: Clear success/failure indicators with emoji icons and detailed error messages
- **Exit codes**: Returns 0 for success, 1+ for validation failures (suitable for CI/CD pipelines)

**Command Requirements:**
- **--flow** is required: Workflow files cannot exist meaningfully without component routing configuration
- **--config** is optional: Uses automatic config resolution (workflow directory → current directory → builtin only)

**Why both flow and config are needed:**
- Workflows reference components like `/python/udf`, `/filesystem/write_file`, `/builtin/openai`
- These component paths require routing rules in stepflow-config.yml to map to actual plugins
- Without proper configuration, component references would fail at runtime

**When to use validate:**
- Before committing workflow or configuration changes
- In CI/CD pipelines to catch configuration errors early
- When debugging workflow execution issues
- After modifying plugin configurations or routing rules
- Before deploying to production environments

**Integration with development workflow:**
```bash
# Validate before running a workflow
cargo run -- validate --flow my-workflow.yaml --config my-config.yml
cargo run -- run --flow my-workflow.yaml --config my-config.yml --input input.json

# Validate in CI/CD (exit code 0 = success, 1+ = failure)
cargo run -- validate --flow workflows/production.yaml --config configs/prod-config.yml

# Quick validation during development (uses config auto-discovery)
cargo run -- validate --flow my-workflow.yaml
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
# Test Python SDK with current Python version
uv run --project sdks/python pytest

# Test Python SDK across all supported versions (3.11, 3.12, 3.13)
./scripts/test-python-versions.sh

# Run Python SDK in stdio mode (default)
uv run --project sdks/python stepflow_py

# Run Python SDK in HTTP mode
uv run --project sdks/python --extra http stepflow_py --http --port 8080

# Run Python SDK with custom host and port
uv run --project sdks/python --extra http stepflow_py --http --host 0.0.0.0 --port 8080
```

#### Python Version Compatibility

The Python SDK supports Python 3.11, 3.12, and 3.13. All versions are tested in CI.

**To test a specific Python version locally:**
```bash
cd sdks/python

# Install specific Python version
uv python install 3.11  # or 3.12, 3.13

# Pin the version for testing
uv python pin 3.11

# Install dependencies and run tests
uv sync --extra http
uv run poe test
uv run poe typecheck
```

#### Python SDK HTTP Mode

The Python SDK supports HTTP mode for remote component servers:

**Installation with HTTP dependencies:**
```bash
# Install with HTTP support
pip install stepflow-py[http]

# Or with uv
uv add stepflow-py[http]
```

**Running the HTTP server:**
```bash
# Basic HTTP mode
stepflow_py --http

# Custom host and port
stepflow_py --http --host 0.0.0.0 --port 8080
```

**Features:**
- FastAPI-based HTTP server
- JSON-RPC over HTTP with MCP-style session negotiation
- Server-Sent Events (SSE) for bidirectional communication
- Per-session isolation and context management
- Compatible with existing component registration
- Automatic discovery and component info endpoints
- Backward compatibility with non-MCP clients

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
STEPFLOW_OVERWRITE_SCHEMA=1 cargo test -p stepflow-protocol

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

Stepflow uses separate systems for logging and distributed tracing:

#### When to Use Logging (`log` crate)

Use logging for detailed implementation information and debugging:

- **Internal details**: Variable values, state changes, conditional logic
- **Debug information**: Cache behavior, recovery operations, fallback handling
- **Low-level operations**: Individual queries, parsing steps, data transformations
- **Trace context is automatic**: All logs automatically include `trace_id` and `span_id` from active span

Examples:
```rust
log::debug!("Resolved {} step inputs", count);
log::info!("Step {} completed with result", step_id);
log::error!("Failed to connect to plugin: {}", error);

// Trace context is added automatically by the logger:
// {"level":"INFO","message":"Step completed","trace_id":"a1b2c3...","span_id":"e5f6..."}
```

#### When to Use Tracing (`fastrace` crate)

Use tracing for the structural, user-facing execution view:

- **Execution structure**: When operations start/end (requests, workflows, steps, components)
- **Key inputs/outputs**: Main operation parameters and results
- **User-relevant errors**: Failures that affect workflow execution
- **System boundaries**: HTTP requests, plugin calls, database operations, external APIs
- **Guideline**: Be conservative - if a support engineer needs it to understand "what happened", trace it

Examples:
```rust
// High-level operation
#[trace(name = "execute_workflow")]
async fn execute_workflow(run_id: Uuid, flow: Arc<Flow>, input: ValueRef) -> Result<FlowResult> {
    // Automatic span with operation name
    log::info!("Starting workflow execution");
    // ... work ...
}

// Component boundary with dynamic name
async fn execute_component(component: &str, input: ValueRef) -> Result<FlowResult> {
    let _guard = LocalSpan::enter_with_local_parent(&format!("component:{}", component));
    log::debug!("Executing component with input: {:?}", input);
    // ... call plugin ...
}

// Record span events for key I/O
let _guard = LocalSpan::enter_with_local_parent("step:transform");
LocalSpan::add_property(|| ("input_size", input.len().to_string()));
let result = transform(input).await?;
LocalSpan::add_property(|| ("output_size", result.len().to_string()));
```

#### Trace Context in Logs

The observability system automatically injects trace context into all log records:

```rust
// You write:
log::info!("Step execution completed");

// Logger outputs (JSON format):
{
  "timestamp": "2025-01-16T10:30:00Z",
  "level": "INFO",
  "message": "Step execution completed",
  "trace_id": "a1b2c3d4e5f6g7h8...",
  "span_id": "i9j0k1l2m3n4o5p6...",
  "target": "stepflow_execution::workflow_executor",
  "file": "workflow_executor.rs",
  "line": 145
}
```

This allows filtering logs by trace ID: "Show me all logs for this workflow run."

#### Zero-Cost Tracing

Fastrace uses zero-cost abstraction - when tracing is disabled, instrumentation has no runtime overhead. This makes it safe to instrument liberally in library code.

#### OTLP Compression

Both trace and log OTLP exporters use **Zstd compression by default** for efficient network transmission.

#### Trace ID and Run ID Relationship

**Design Decision:** Stepflow uses the workflow `run_id` (UUID) as the OpenTelemetry `trace_id` for execution.

Each flow execution is treated as a single distributed trace, with the `run_id` serving as both:
- The business identifier for the flow run
- The OpenTelemetry trace ID for the entire execution graph

This design choice prioritizes developer experience and observability UX over theoretical purity.

### Error Handling Patterns

#### Dual Error System
Stepflow uses a dual error approach to distinguish between business logic and system failures:

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

#### Domain-Specific vs Boundary Errors
Use strongly-typed domain errors internally, but convert to broader boundary errors at trait/API boundaries:

```rust
// Internal method with domain-specific error
async fn connect_internal(&self) -> McpResult<Connection> {
    // Use specific McpError variants
    client.connect().change_context(McpError::Communication)?
}

// Trait method converts to boundary error
async fn connect(&self) -> Result<Connection> {
    self.connect_internal()
        .await
        .change_context(PluginError::Execution)
}
```

**Benefits:**
- Precise error categorization internally
- Maintains trait compatibility
- Preserves error chains for debugging via `error_stack`

#### Parameterized Error Variants
Use parameterized error variants for context instead of verbose attachments:

```rust
// Good: Parameterized variant
#[derive(Error, Debug)]
pub enum McpError {
    #[error("Failed to setup I/O: {0}")]
    ProcessSetup(&'static str),
}

// Usage
error_stack::report!(McpError::ProcessSetup("stdin"))

// Avoid: Redundant attachment
error_stack::report!(McpError::ProcessSetup)
    .attach_printable("Failed to capture stdin handle")
```

#### Error Context Guidelines
**When to use `attach_printable`:**
- Adding valuable runtime context (variables, computed values)
- Dynamic information not captured in the error type

**When to avoid `attach_printable`:**
- Error type + line number provide sufficient context
- Information that just restates the error variant

**Additional patterns:**
- Use `attach_printable_lazy` for expensive string formatting
- Use `error_stack::report!` macro instead of `Report::new()`
- Prefer parameterized error variants over attachments for simple context

#### Error Inspection Patterns
Use `downcast_ref()` to inspect specific error types in error chains:

```rust
if let Some(mcp_error) = error.downcast_ref::<McpError>() {
    match mcp_error {
        McpError::ToolExecution => {
            // Handle as business logic failure
            return Ok(FlowResult::Failed {
                error: FlowError::new(500, "Tool failed")
            });
        }
        _ => {} // Handle as system error
    }
}
```

This enables fine-grained error handling while preserving the complete error context.

### Derive Patterns

Use consistent derive patterns based on type purpose:

- **Core workflow types**: `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]`
- **Error types**: `#[derive(Error, Debug)]` (with `thiserror`)
- **Configuration types**: `#[derive(Serialize, Deserialize)]` with serde attributes
- **CLI types**: `#[derive(clap::Parser)]`

**Note**: Avoid `Clone` on large structs like `Step` unless absolutely necessary. Use `Arc` references instead.

## Configuration

The `stepflow-config.yml` file defines plugins, routing rules, and state storage available to the workflow executor:

### Plugin Configuration

Plugins are defined as key-value pairs where the key is the plugin name and the value specifies the plugin type and configuration:

```yaml
plugins:
  builtin:  # Plugin name (used in routing rules)
    type: builtin
  python:   # Plugin name for Python component server (stdio)
    type: stepflow
    transport: stdio
    command: uv    # Command to execute
    args: ["--project", "../sdks/python", "run", "stepflow_py"]  # Arguments
  python_http:  # Plugin name for Python component server (HTTP)
    type: stepflow
    transport: http
    url: "http://localhost:8080"  # URL of the HTTP component server
  filesystem:  # Plugin name for MCP filesystem server
    type: mcp
    command: npx
    args: ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
```

### Plugin Types

- **builtin**: Built-in components (OpenAI, create_messages, eval, etc.)
- **stepflow**: Stepflow component servers with configurable transport
  - **stdio transport**: JSON-RPC over stdio communication with external processes
  - **http transport**: JSON-RPC over HTTP with Server-Sent Events for bidirectional communication
- **mcp**: Model Context Protocol servers

#### MCP Plugin Configuration

MCP plugins support environment variable substitution in both command arguments and environment variables:

```yaml
filesystem:
  type: mcp
  command: npx
  args: ["-y", "@modelcontextprotocol/server-filesystem", "${HOME}/workspace"]
  env:
    MCP_LOG_LEVEL: "${LOG_LEVEL:-info}"
    MCP_CONFIG_DIR: "${HOME}/.config/mcp"
    MCP_WORKSPACE: "${WORKSPACE_DIR:-${HOME}/workspace}"
```

**MCP Environment Variable Features:**
- Same substitution syntax as Stepflow plugins: `${VAR}` and `${VAR:-default}`
- Environment variables are substituted when the MCP server process is launched
- Both `args` and `env` fields support full substitution
- Useful for configuring MCP servers with user-specific paths and settings
- Command arguments can include environment variables for flexible server configuration

### Stepflow Plugin Transport Options

The `stepflow` plugin type supports two transport methods:

#### Stdio Transport
```yaml
python_stdio:
  type: stepflow
  transport: stdio
  command: uv
  args: ["--project", "${PROJECT_DIR:-../sdks/python}", "run", "stepflow_py"]
  env:  # Optional environment variables with substitution support
    PYTHONPATH: "${HOME}/custom/path"
    USER_CONFIG: "${USER:-anonymous}"
    CUSTOM_PATH: "${HOME}/projects/${USER}"
```

**Environment Variable Substitution:**
- Environment variables support shell-like substitution using `${VAR}` syntax
- Default values can be specified using `${VAR:-default}` syntax
- Nested substitution is supported: `${HOME}/projects/${USER}`
- Values are substituted from the current process environment when the plugin is launched
- If a variable is not found and no default is provided, substitution will fail with an error
- **Applies to both**: Command arguments (`args`) and environment variables (`env`)

#### HTTP Transport
```yaml
python_http:
  type: stepflow
  transport: http
  url: "http://localhost:8080"
```

**HTTP Transport Features:**
- Remote component servers
- HTTP-based JSON-RPC communication
- Server-Sent Events (SSE) for bidirectional communication
- MCP-style session negotiation for connection isolation
- Automatic session management and cleanup
- Scalable for distributed deployments
- No process management required
- Backward compatibility with non-MCP servers

#### HTTP Transport Session Negotiation

The HTTP transport implements MCP-style session negotiation for proper connection isolation:

**Connection Flow:**
1. Client connects to `/runtime/events` SSE endpoint
2. Server sends an `endpoint` event with a session-specific URL: `{"endpoint": "/?sessionId=<uuid>"}`
3. Client uses the sessionId URL for all subsequent JSON-RPC requests
4. Each session has isolated context and request handling
5. Sessions are automatically cleaned up when SSE connections close

**Fallback Behavior:**
- If no `endpoint` event is received within 5 seconds, the client falls back to direct HTTP communication
- This ensures compatibility with older or non-MCP servers
- Fallback mode uses the base URL without session isolation

**Session Benefits:**
- **Isolation**: Each client gets its own session context and message handling
- **Reliability**: Request/response matching is scoped to individual sessions
- **Scalability**: Multiple clients can connect simultaneously without interference
- **Cleanup**: Resources are automatically freed when clients disconnect

### Routes Configuration

Stepflow uses routes to map component paths to specific plugins. **Route rules are required** - components will not be accessible without appropriate route rules.

```yaml
routes:
  "/python/{*component}":
    - plugin: python
  "/python_http/{*component}":
    - plugin: python_http
  "/filesystem/{*component}":
    - plugin: filesystem
  "/{*component}":
    - plugin: builtin
```

#### Route Rules

- **Path patterns**: Component path patterns with placeholders like `{component}` or `{*component}`
- **plugin**: Plugin name to route to (must match a key in the plugins section)
- Rules are evaluated in order, first match wins
- Use `/{*component}` as a catch-all pattern for fallback routing
- **Important**: Target plugins referenced in routing rules must be defined in the plugins section

#### Advanced Routes with Input Conditions

Route rules can include input-based conditions for more sophisticated routing:

```yaml
routes:
  "/custom/{*component}":
    - conditions:
        - path: "$.model"
          value: "gpt-4"
      plugin: openai
    - plugin: fallback
```

This routes `/custom/*` components to the `openai` plugin when the input contains `"model": "gpt-4"`, otherwise to the `fallback` plugin.

**Input conditions support:**
- **path**: JSON path expression to extract value from input (e.g., `$.model`, `$.config.temperature`)
- **value**: Expected value for the path (exact match required)
- Multiple conditions can be specified - all must match for the rule to apply

#### Route Rule Examples

```yaml
routes:
  # Route Python components to Python SDK (stdio)
  "/python/{*component}":
    - plugin: python_stdio

  # Route Python HTTP components to remote Python server
  "/python_http/{*component}":
    - plugin: python_http

  # Route specific builtin components
  "/builtin/openai":
    - plugin: builtin

  # Route filesystem operations to MCP server
  "/filesystem/{*component}":
    - plugin: filesystem

  # Route based on input conditions
  "/ai/chat":
    - conditions:
        - path: "$.provider"
          value: "openai"
      plugin: openai_plugin

  # Fallback to builtin for everything else
  "/{*component}":
    - plugin: builtin
```

### Complete Configuration Example

Here's a complete example showing plugins, routing, and state store configuration:

```yaml
plugins:
  builtin:
    type: builtin
  python_stdio:
    type: stepflow
    transport: stdio
    command: uv
    args: ["--project", "../sdks/python", "run", "stepflow_py"]
    env:
      PYTHONPATH: "${HOME}/custom/path"
      USER_CONFIG: "${USER:-anonymous}"
  python_http:
    type: stepflow
    transport: http
    url: "http://localhost:8080"
  filesystem:
    type: mcp
    command: npx
    args: ["-y", "@modelcontextprotocol/server-filesystem", "${HOME}/workspace"]
    env:
      MCP_LOG_LEVEL: "${LOG_LEVEL:-info}"
      MCP_CONFIG_DIR: "${HOME}/.config/mcp"

routes:
  "/python/{*component}":
    - plugin: python_stdio
  "/python_http/{*component}":
    - plugin: python_http
  "/filesystem/{*component}":
    - plugin: filesystem
  "/{*component}":
    - plugin: builtin

stateStore:
  type: sqlite
  databaseUrl: "sqlite:workflow_state.db"
  autoMigrate: true
```

### State Store Configuration

Stepflow supports multiple state storage backends for persisting workflow execution state and blobs:

#### In-Memory State Store (Default)
```yaml
# No configuration needed - this is the default
stateStore:
  type: inMemory
```

#### SQLite State Store
```yaml
stateStore:
  type: sqlite
  databaseUrl: "sqlite:workflow_state.db"  # File path or ":memory:"
  autoMigrate: true                        # Automatically create/update schema
  maxConnections: 10                       # Connection pool size
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
from stepflow_py import StepflowStdioServer, StepflowContext
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

## Workflow Path Syntax & Value Resolution

### JSON Path Syntax for Nested Data Access

When accessing nested fields in workflow inputs or step outputs, use JSONPath syntax:

**For workflow input:**
```yaml
input:
  # Simple field access
  message: { $from: { workflow: input }, path: "message" }

  # Nested object field access (use JSONPath with $.)
  a: { $from: { workflow: input }, path: "$.numbers.a" }
  b: { $from: { workflow: input }, path: "$.numbers.b" }
```

**For step output:**
```yaml
input:
  # Access specific field from step result
  value: { $from: { step: math_add }, path: "result" }

  # Access nested fields from step result
  details: { $from: { step: analysis }, path: "$.analysis.summary" }
```

### Path Resolution Rules

1. **Simple paths** (no dots): `path: "field_name"` - Access direct field
2. **JSONPath syntax** (with $.): `path: "$.nested.field"` - Access nested fields
3. **No path specified**: Returns the entire object/result

### Common Path Patterns

```yaml
# Input data structure:
# {
#   "message": "hello",
#   "config": {
#     "temperature": 0.7,
#     "model": "gpt-4"
#   },
#   "items": ["a", "b", "c"]
# }

# Correct path usage:
message: { $from: { workflow: input }, path: "message" }           # "hello"
temp: { $from: { workflow: input }, path: "$.config.temperature" } # 0.7
model: { $from: { workflow: input }, path: "$.config.model" }      # "gpt-4"
items: { $from: { workflow: input }, path: "items" }              # ["a", "b", "c"]
```

## ICLA (Individual Contributor License Agreement) System

### Overview

StepFlow requires all contributors to sign an Individual Contributor License Agreement (ICLA) before their pull requests can be merged. This is enforced automatically through GitHub Actions.

### For New Contributors

**⚠️ CRITICAL**: All contributors MUST sign the ICLA before making contributions.

**Check ICLA status:**
```bash
python scripts/check_icla.py
```

**Sign the ICLA (required for first-time contributors):**
```bash
python scripts/sign_icla.py
```

The signing process will:
- Collect required information (name, email, GitHub username, country, company)
- Generate a unique signature ID
- Store the signature in `.github/cla-signatures.json`
- Display confirmation with signature details

**ICLA Information Required:**
- Full legal name
- Email address (must match git config)
- GitHub username
- Country of residence
- Company/organization (optional)

### GitHub Actions Integration

Pull requests automatically trigger ICLA verification:
- ✅ **Signed**: PR can proceed with review
- ❌ **Not signed**: Automated comment with signing instructions
- 🤖 **Bot accounts**: Automatically skipped (dependabot, renovate, etc.)

The GitHub Actions workflow uses a custom action located at `.github/actions/icla-check/` for modular and testable ICLA verification.

### ICLA System Components

**Core files:**
- `ICLA.md` - Complete license agreement document
- `scripts/check_icla.py` - Local signature verification script
- `scripts/sign_icla.py` - Interactive signing script
- `.github/actions/icla-check/action.yml` - GitHub Action definition
- `.github/actions/icla-check/check_signature.py` - GitHub Action signature check
- `.github/workflows/cla-check.yml` - GitHub Actions workflow
- `.github/cla-signatures.json` - Signature storage (auto-created)

**Key features:**
- **GitHub Actions only**: No local pre-commit hooks to avoid developer friction
- **Signature deduplication**: Updates existing signatures instead of creating duplicates
- **Color-coded output**: Green for signed, red for unsigned status
- **Modular design**: Encapsulated GitHub Action for easy testing and maintenance
- **Legal compliance**: Based on Apache Software Foundation ICLA template

### When to Guide Developers Through ICLA

**Always mention ICLA requirements when:**
- New contributor asks about contributing
- Developer wants to submit their first PR
- GitHub Actions CLA check fails on PRs

**ICLA guidance template:**
```
Before you can contribute to StepFlow, you need to sign our Individual Contributor License Agreement (ICLA). This is a one-time legal requirement.

1. Clone the repository locally
2. Sign the ICLA: `python scripts/sign_icla.py`
3. Commit the signature file: `git add .github/cla-signatures.json && git commit -m "Add ICLA signature"`
4. Push your changes

The ICLA ensures clear intellectual property rights and is required for all Apache-style open source projects. See ICLA.md for details.
```

### Troubleshooting ICLA Issues

**Common issues and solutions:**

1. **GitHub Actions CLA check fails:**
   - Sign ICLA locally: `python scripts/sign_icla.py`
   - Commit the updated `.github/cla-signatures.json`
   - Push changes to update the PR

2. **Email mismatch:**
   - Git email and ICLA email must match
   - Update git config: `git config user.email "correct@email.com"`
   - Re-sign ICLA if needed

3. **Missing signature file:**
   - The `.github/cla-signatures.json` file is auto-created on first signature
   - Commit and push this file after signing

## HTTP Server Architecture

### Python SDK HTTP Transport

The Python SDK supports both STDIO and HTTP transports:

**STDIO Transport** (default):
```bash
# Run Python component server in stdio mode
uv run --project sdks/python stepflow_py

# Used by Rust runtime via process spawning
```

**HTTP Transport** (streaming):
```bash
# Run Python component server in HTTP mode
uv run --project sdks/python --extra http stepflow_py --http --port 8080

# Provides streamable HTTP transport with bidirectional communication
```

### HTTP Server Features

1. **Health Endpoint**: `/health` - Returns server status for integration tests
2. **Streamable Transport**: Supports both direct JSON and SSE streaming responses
3. **Accept Header Support**: Requires `Accept: application/json` or `Accept: text/event-stream`
4. **Bidirectional Communication**: Components with `StepflowContext` parameter trigger streaming mode
5. **Session Management**: Automatic context management for bidirectional requests

### Configuration for HTTP Transport

```yaml
plugins:
  python_http:
    type: stepflow
    transport: http
    url: "http://localhost:8080"

routes:
  "/python/{*component}":
    - plugin: python_http
```

### Architecture Changes (Recent)

The Python SDK has undergone significant architectural improvements:

1. **Unified Core Server**: Both HTTP and STDIO servers delegate to `StepflowServer`
2. **Context Inheritance**: `StepflowStreamingContext` inherits from `StepflowContext`
3. **MessageDecoder Consolidation**: Single request/response correlation system
4. **Clean Test Separation**: Core functionality vs transport layer testing
