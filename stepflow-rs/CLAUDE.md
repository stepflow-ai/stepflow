# Rust Development Guide

This guide covers Rust-specific conventions and best practices for the Stepflow project.

See the root `/CLAUDE.md` for project overview, configuration, and workflow syntax.

## Quick Start

```bash
# Build and test
cargo build
cargo test

# Run a workflow
cargo run -- run --flow=../examples/basic/workflow.yaml --input=../examples/basic/input1.json --config=../examples/basic/stepflow-config.yml

# Validate workflow and configuration
cargo run -- validate --flow=../examples/basic/workflow.yaml --config=../examples/basic/stepflow-config.yml

# Start service
cargo run -- serve --port=7837 --config=../examples/basic/stepflow-config.yml
```

## Building and Testing

### Building

```bash
# Build entire project
cargo build

# Build with release optimizations
cargo build --release

# Check compilation without building
cargo check

# Check specific crate
cargo check -p stepflow-core
```

### Testing

```bash
# Fast unit tests only (no external dependencies)
cargo test

# Run tests for a specific crate
cargo test -p stepflow-execution

# Run a specific test
cargo test -p stepflow-execution -- execute_flows

# Integration tests (requires Python environment and stepflow binary)
../scripts/test-integration.sh

# Complete test suite (unit + Python SDK + integration)
../scripts/test-all.sh
```

### Snapshot Testing

Tests in `crates/stepflow-main/tests/test_run.rs` use `insta` for snapshot testing.

Each file in `crates/stepflow-main/tests/flows` contains a stepflow_config, workflow, and test cases. Add new tests by either:
1. Adding a case to an existing test workflow
2. Creating a new file with new configuration, workflow, and test cases

```bash
# Run tests and delete unused snapshots
cargo insta test --unreferenced=delete --review

# List pending snapshots
cargo insta pending-snapshots

# Show a specific snapshot
cargo insta show <path>

# Accept all snapshots
cargo insta accept
```

### Conditional Test Skipping

Tests that require external infrastructure (Docker, NATS, etcd) must use the `require_docker!()` macro from `stepflow-test-utils`. This macro:

- **In CI** (`CI` env var set): **panics** so the test fails loudly if infrastructure is missing.
- **Local development** (`CI` not set): prints a skip message and returns early.

```rust
#[tokio::test]
async fn my_docker_test() {
    stepflow_test_utils::require_docker!();
    // Docker is guaranteed available here — start containers
}
```

**Never silently skip tests using bare `return` or `if !available { return; }`.** CI must treat missing infrastructure as a failure, not a pass. If you need to conditionally skip a test, either:

1. Use `require_docker!()` for Docker/container-based tests (preferred).
2. Use `#[ignore = "reason"]` for tests that need special binaries or manual setup and are excluded from the default `cargo test` run.

### Code Linting

```bash
# Run clippy on all crates
cargo clippy

# Auto-fix linting issues where possible
cargo clippy --fix

# Format code
cargo fmt
```

### Schema Generation

When modifying core types, regenerate schemas and derived code:

```bash
# Regenerate JSON schemas from Rust types
STEPFLOW_OVERWRITE_SCHEMA=1 cargo test -p stepflow-core

# Then regenerate Python types from updated schemas
cd ../sdks/python
uv run python generate.py

# Check if Python types are up to date without regenerating
uv run python generate.py --check
```

**Important**: Always regenerate schemas after modifying workflow and configuration types in `stepflow-core`. Schema files are used by:
- Python SDK type generation
- Documentation and examples
- API validation in server components

### Proto / gRPC Code Generation

Proto files live in `stepflow-rs/proto/stepflow/v1/`. Rust and Python stubs are generated separately:

```bash
# Rust: prost auto-generates from proto during cargo build.
# No manual step needed — just rebuild.
cargo build

# Python: regenerate gRPC stubs after modifying any proto file.
../scripts/generate-python-proto.sh
```

**Important**: After modifying any `.proto` file, always regenerate the Python stubs. Rust stubs are regenerated automatically by prost on build.

Key proto files:
- `tasks.proto` — TaskAssignment (oneof execute | list_components), PullTasksRequest, ComponentInfo
- `orchestrator.proto` — CompleteTask, TaskHeartbeat, ListComponentsResult
- `components.proto` — ComponentsService (public REST/gRPC API for listing components)
- `common.proto` — Shared types (ObservabilityContext, TaskErrorCode)

## Project Architecture

### Crate Overview

**Core Types** (`stepflow-core`):
- Rust structs for workflows, components, value expressions
- Flow structure, steps, components, schema definitions
- `FlowResult` type for workflow execution results

**Execution Engine** (`stepflow-execution`):
- Core workflow execution logic
- Parallel execution, error handling, state management
- Workflow state management and coordination

**Plugin System** (`stepflow-plugin`):
- Extensible component services
- External plugin communication
- Core plugin trait and management

**Built-in Components** (`stepflow-builtins`):
- Built-in component implementations
- OpenAI API integration and other core components
- Plugin trait implementation for built-in functionality

**MCP Integration** (`stepflow-components-mcp`):
- Model Context Protocol (MCP) tool integration
- Allows workflows to use MCP-compatible components

**CLI & Service** (`stepflow-main`):
- Main binary for executing workflows
- Local execution and service modes
- CLI commands: run, serve, submit, validate

**Testing Support** (`stepflow-mock`):
- Mock implementations for testing
- Unit testing facilitation

### Data Flow

1. Workflows defined in YAML/JSON files
2. Parsed and validated using `stepflow-core` types
3. Execution engine processes workflow:
   - Instantiates required plugins
   - Executes steps in parallel when possible
   - Manages state and data flow between steps
4. Steps executed by components:
   - Built-in components from `stepflow-builtins`
   - Workers via gRPC (PullTasks/CompleteTask)
   - MCP tools via `stepflow-components-mcp`
   - Python, TypeScript, or other SDK-based components

## Code Style & Best Practices

### Rust Code Standards

- Use `rustfmt` for consistent formatting (`cargo fmt`)
- Use `clippy` for linting (`cargo clippy`)
- Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Maximum line length: 100 characters

### Module Organization

- Use `foo.rs` files, not `foo/mod.rs` — enforced by `clippy::mod_module_files` (deny)
- When a module has submodules, create `foo.rs` alongside a `foo/` directory
- Example: `src/recovery.rs` + `src/recovery/tree.rs`, NOT `src/recovery/mod.rs`

### Documentation

- Use `///` for public API documentation
- Use `//!` for module-level documentation
- Include examples in documentation where appropriate
- Document all public types, functions, and traits
- Use markdown in documentation comments

### Testing Practices

- Place unit tests in the same file as the code they test
- Use `#[test]` for unit tests
- Use `#[cfg(test)]` for test modules
- Follow pattern: `mod tests { ... }`
- Place integration tests in `tests/` directory
- Test both success and failure cases

**Test Organization**:
- **Unit tests**: Inline with source files using `#[cfg(test)]` modules
- **Crate-level integration tests**: In `<crate>/tests/` directories (e.g., `crates/stepflow-main/tests/`)
- **Workflow-level integration tests**: In `/tests/` at project root using actual workflow YAML files

Use descriptive test names: `test_<feature>_<scenario>`

## Performance Guidelines

### Avoid Unnecessary Cloning

- Use `Arc<T>` for shared expensive-to-clone data structures
  - Examples: `Arc<Flow>`, `Arc<dyn StateStore>`
- Prefer references over clones when passing data to async tasks
- Use `Cow<'static, str>` for string data that may be borrowed or owned
- Example: Instead of `step.clone()` in async tasks, pass `Arc<Flow>` and use step indices

### Async Patterns

- Use `async fn` consistently for I/O operations
- Prefer `tokio::spawn` for concurrent execution
- Use `futures::future::BoxFuture` for trait objects returning futures instead of `Pin<Box<dyn Future>>`
- Always import `futures::future::FutureExt as _` when using `.boxed()`

Example:
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

## Logging and Tracing

Stepflow uses separate systems for logging and distributed tracing.

### When to Use Logging (`log` crate)

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

### When to Use Tracing (`fastrace` crate)

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

### Trace Context in Logs

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

### Zero-Cost Tracing

Fastrace uses zero-cost abstraction - when tracing is disabled, instrumentation has no runtime overhead. This makes it safe to instrument liberally in library code.

### OTLP Compression

Both trace and log OTLP exporters use **Zstd compression by default** for efficient network transmission.

### Trace ID and Run ID Relationship

**Design Decision**: Stepflow uses the workflow `run_id` (UUID) as the OpenTelemetry `trace_id` for execution.

Each flow execution is treated as a single distributed trace, with the `run_id` serving as both:
- The business identifier for the flow run
- The OpenTelemetry trace ID for the entire execution graph

This design choice prioritizes developer experience and observability UX over theoretical purity.

## Error Handling

Stepflow uses a dual error approach to distinguish between business logic and system failures.

### Dual Error System

**1. FlowError**: Business logic failures that are part of normal workflow execution
- Used for validation failures, missing data, expected conditions
- Allow workflows to continue and handle errors gracefully
- Example: `FlowResult::Failed { error: FlowError::new(400, "Invalid input") }`

**2. System Errors**: Implementation or infrastructure failures
- Used for plugin communication failures, serialization errors, etc.
- Represent unexpected conditions that should halt execution
- Each crate has its own `error.rs` module with custom error types
- Uses `error-stack` for rich error context and `thiserror` for error enums

### FlowResult Enum

Enables proper error propagation through workflow execution:
- `Success(ValueRef)`: Step completed successfully with output
- `Skipped`: Step was conditionally skipped
- `Failed(FlowError)`: Step failed with a business logic error

### Error Handling Best Practices

- Define custom error types in `error.rs` at the crate root (or in appropriate modules)
- Include a `type Result<T, E = TheErrorType> = std::result::Result<T, E>` alias
- Use `thiserror` for defining error types
- Include context in error messages
- Document error variants and their meanings

### Advanced Error Patterns

**Basic Pattern**:
- Use `FlowError` for expected business failures
- Use `Result<T, error_stack::Report<YourError>>` for system failures
- Add context with `error_stack::ResultExt::attach_printable`
- Define custom error types with `thiserror::Error`

### Domain-Specific vs Boundary Errors

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

**Benefits**:
- Precise error categorization internally
- Maintains trait compatibility
- Preserves error chains for debugging via `error_stack`

### Parameterized Error Variants

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

### Error Context Guidelines

**When to use `attach_printable`**:
- Adding valuable runtime context (variables, computed values)
- Dynamic information not captured in the error type

**When to avoid `attach_printable`**:
- Error type + line number provide sufficient context
- Information that just restates the error variant

**Additional patterns**:
- Use `attach_printable_lazy` for expensive string formatting
- Use `error_stack::report!` macro instead of `Report::new()`
- Prefer parameterized error variants over attachments for simple context

### Error Inspection Patterns

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

## Derive Patterns

Use consistent derive patterns based on type purpose:

- **Core workflow types**: `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]`
- **Error types**: `#[derive(Error, Debug)]` (with `thiserror`)
- **Configuration types**: `#[derive(Serialize, Deserialize)]` with serde attributes
- **CLI types**: `#[derive(clap::Parser)]`

**Note**: Avoid `Clone` on large structs like `Step` unless absolutely necessary. Use `Arc` references instead.

## Handling Explicit Null in JSON

Structs deserialized from JSON/YAML must tolerate explicit `null` for fields that have `#[serde(default)]`. This is critical because Python clients (Pydantic `model_dump()`, msgspec `to_builtins()`) may serialize optional fields as `null` rather than omitting them.

Plain `#[serde(default)]` handles *absent* fields but rejects `null` for non-`Option` types like `Vec`, `HashMap`, and `bool`. Two approaches handle this:

### 1. `DefaultOnNull` — when `T::default()` is the correct default

Use `serde_with::DefaultOnNull` for fields where `T::default()` matches the desired default (e.g., `Vec` → empty, `HashMap` → empty, enums with a `Default` impl):

```rust
use serde_with::{DefaultOnNull, serde_as};

#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct MyStruct {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[serde_as(as = "DefaultOnNull")]
    pub items: Vec<String>,
}
```

### 2. `deserialize_with` — when the custom default differs from `T::default()`

`DefaultOnNull` uses `T::default()`, not the serde `#[serde(default = "...")]` value. For fields like `enabled: bool` (default `true`) or `max_retries: u32` (default `3`), use a `deserialize_with` function instead:

```rust
fn null_or_default_enabled<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(Option::deserialize(d)?.unwrap_or(true))
}

#[derive(Serialize, Deserialize)]
pub struct MyConfig {
    #[serde(default = "default_enabled", deserialize_with = "null_or_default_enabled")]
    pub enabled: bool,
}
```

The pattern is: deserialize as `Option<T>`, then `unwrap_or` with the custom default.

### Testing Null Fields

Every struct deserialized from external JSON should have a test verifying that all fields accept explicit `null` and produce the correct custom default:

```rust
#[test]
fn test_my_config_null_fields_use_custom_defaults() {
    let json = serde_json::json!({
        "enabled": null,
        "items": null,
    });
    let c: MyConfig = serde_json::from_value(json).unwrap();
    assert!(c.enabled);  // true, not false
    assert!(c.items.is_empty());
}
```

See `stepflow-config/src/recovery_config.rs` for a real example.

## OpenAPI Discriminator Pattern

Tagged enums need explicit discriminator annotations for proper Python codegen.
Without them, `openapi-generator` produces `OneOf`, `OneOf1`, etc. instead of
named variant classes.

### Basic discriminator

Add `AddDiscriminator` transform to the enum and `title` to each variant:

```rust
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
#[schemars(transform = stepflow_core::discriminator_schema::AddDiscriminator::new("type"))]
enum MyEnum {
    #[schemars(title = "MyEnumVariantA")]
    VariantA { value: String },
    #[schemars(title = "MyEnumVariantB")]
    VariantB { count: i32 },
}
```

- The `AddDiscriminator` property name must match the `serde(tag = "...")` value.
- Each variant needs `#[schemars(title = "...")]` — the title becomes the `$defs`
  key and the generated Python class name.
- The `generate_json_schema_with_defs` pipeline (in `stepflow-core/src/json_schema.rs`)
  extracts inline variants to `$defs` and builds discriminator mappings from titles.

### Wrapper struct with shared fields (flatten pattern)

When a struct wraps a tagged enum with `#[serde(flatten)]` and adds shared fields,
use `MergePropertiesIntoOneOf` to push the shared fields into each variant schema:

```rust
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(transform = stepflow_core::discriminator_schema::MergePropertiesIntoOneOf)]
struct Wrapper {
    sequence_number: u64,
    timestamp: DateTime<Utc>,
    #[serde(flatten)]
    kind: MyEnum,
}
```

This ensures each generated Python variant model includes `sequence_number` and
`timestamp` as fields, rather than having them only on the wrapper (which
`openapi-generator` ignores when `oneOf` is present).

### Implementation

Both transforms live in `stepflow-core/src/discriminator_schema.rs`.

### Regenerating after schema changes

```bash
# Regenerate Python SDK
cd ../sdks/python && uv run poe codegen-fix
```
## Project Dependencies

- Keep dependencies minimal and well-documented
- Use workspace dependencies where appropriate
- Document the purpose of each dependency
- Keep dependencies up to date
