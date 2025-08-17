# Contributing to Stepflow

Thank you for your interest in contributing to Stepflow! This guide will help you get started with development and ensure your contributions align with our project standards.

## Table of Contents

- [Development Environment Setup](#development-environment-setup)
- [Building and Testing](#building-and-testing)
- [Project Architecture](#project-architecture)
- [Code Style and Conventions](#code-style-and-conventions)
- [Making Contributions](#making-contributions)
- [Submitting Changes](#submitting-changes)

## Development Environment Setup

### Prerequisites

- **Rust 1.70+** - Install via [rustup](https://rustup.rs/)
- **Python 3.8+** - For Python SDK development and examples
- **uv** - Python package manager (install via `pip install uv`)

### Getting Started

1. **Clone the repository**
   ```bash
   git clone https://github.com/stepflow-ai/stepflow.git
   cd stepflow
   ```

2. **Run the development setup script**
   ```bash
   ./scripts/setup_dev.sh
   ```
   This script will:
   - Install pre-commit hooks
   - Check your git configuration
   - Verify your development environment
   - Remind you to sign the ICLA (see below)

3. **Sign the Individual Contributor License Agreement (ICLA)**

   **⚠️ REQUIRED**: Before making your first contribution, you must sign the ICLA:

   ```bash
   python scripts/sign_icla.py
   ```

   The ICLA is a one-time legal agreement that:
   - Grants the project rights to use your contributions
   - Confirms you own the copyright to your work
   - Ensures clear licensing for all project code

   **Why is this required?**
   - Protects both you and the project legally
   - Ensures we can distribute your contributions under the Apache 2.0 license
   - Required for all open source Apache Software Foundation projects

   **What information is needed?**
   - Your full legal name
   - Email address
   - GitHub username
   - Country of residence
   - Company/organization (optional)

   For full details, see [ICLA.md](ICLA.md).

4. **Build the stepflow-rs project**
   ```bash
   cd stepflow-rs
   cargo build
   ```

5. **Run tests to verify setup**
   ```bash
   cargo test
   ```

6. **Try running an example**
   ```bash
   cargo run -- run --flow=examples/basic/workflow.yaml --input=examples/basic/input1.json --config=examples/basic/stepflow-config.yml
   ```

## Building and Testing

### Building

```bash
# Build the entire project (run from stepflow-rs directory)
cd stepflow-rs
cargo build

# Build with release optimizations
cargo build --release
```

### Running Tests

```bash
# Run fast unit tests (no external dependencies)
cd stepflow-rs
cargo test

# Run with snapshot testing (preferred for development)
cargo insta test --unreferenced=delete --review

# Run tests for a specific crate
cargo test -p stepflow-execution

# Run a specific test
cargo test -p stepflow-execution -- execute_flows

# Run integration tests (requires Python environment)
cd ..
./scripts/test-integration.sh

# Run complete test suite (unit + Python SDK + integration)
./scripts/test-all.sh
```

### Code Quality

```bash
# Run clippy for linting (run from stepflow-rs directory)
cd stepflow-rs
cargo clippy

# Auto-fix linting issues where possible
cargo clippy --fix

# Format code
cargo fmt

# Auto-fix unused dependencies
cargo machete --fix --with-metadata
```

**Important**: Always run `cargo clippy` and `cargo fmt` before submitting changes.

## Project Architecture

### Repository Structure

This repository contains multiple components:

- **`stepflow-rs/`** - Main Rust-based execution engine and runtime
- **`stepflow-ui/`** - Web-based frontend for workflow management
- **`sdks/python/`** (`stepflow-py`) - Python SDK for building components
- **`sdks/typescript/`** (`stepflow-ts`) - TypeScript SDK for building components

### Rust Workspace

The main `stepflow-rs/` directory contains a Rust workspace with multiple crates:

### Core Crates

- **`stepflow-core`** - Core types and workflow definitions
- **`stepflow-execution`** - Workflow execution engine
- **`stepflow-plugin`** - Plugin system and trait definitions
- **`stepflow-protocol`** - JSON-RPC communication protocol
- **`stepflow-builtins`** - Built-in component implementations
- **`stepflow-components-mcp`** - MCP (Model Context Protocol) integration
- **`stepflow-main`** - CLI binary and service implementation
- **`stepflow-mock`** - Mock implementations for testing

### Key Concepts

- **Flow**: Complete workflow definition with steps, inputs, and outputs
- **Step**: Single operation within a workflow
- **Component**: Specific implementation that a step invokes
- **Plugin**: Service providing one or more components
- **Value**: Data flowing between steps with references

### Error Handling

Stepflow uses two distinct error types:

1. **Flow Errors** (`FlowError`): Business logic failures that are part of normal workflow execution
2. **System Errors** (`Result::Err`): Implementation or infrastructure failures

See `CLAUDE.md` for detailed architecture information.

## Code Style and Conventions

### Rust Code Standards

- **Formatting**: Use `rustfmt` (run `cargo fmt`)
- **Linting**: Use `clippy` (run `cargo clippy`)
- **Line length**: Maximum 100 characters
- **API Guidelines**: Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)

### Documentation

- Use `///` for public API documentation
- Use `//!` for module-level documentation
- Include examples in documentation where appropriate
- Document all public types, functions, and traits
- Use markdown in documentation comments

### Error Handling

#### Basic Requirements

- Define custom error types in `error.rs` modules
- Use `thiserror` for defining error types
- Include context in error messages
- Document error variants and their meanings
- Include `type Result<T, E = TheErrorType>` aliases

#### Practical Guidelines

**1. Choose the right error type:**
- Use domain-specific errors (`McpError`, `StateError`) for internal implementations
- Convert to boundary errors (`PluginError`) at trait/public API boundaries
- Use `FlowError` for business logic failures that workflows should handle

**2. Context management:**
- Prefer parameterized error variants over verbose `attach_printable` calls
- Only add `attach_printable` when it provides valuable runtime context
- Use `error_stack::report!` macro instead of `Report::new()`

**3. Error conversion patterns:**
```rust
// Convert at boundaries while preserving error chain
internal_method()
    .await
    .change_context(PublicApiError::SomeCategory)
```

**4. Testing error conditions:**
- Test both success and failure paths
- Use `downcast_ref()` to verify specific error types in tests
- Ensure error messages are helpful for debugging

#### Common Patterns to Avoid

- ❌ Redundant error attachments when type + line number suffice
- ❌ Using `Report::new()` instead of `report!` macro
- ❌ Deep error conversion chains (convert once at boundaries)
- ❌ Adding `attach_printable` that just restates the error type

#### Example: Good Error Handling

```rust
// 1. Define parameterized error variants
#[derive(Error, Debug)]
pub enum MyError {
    #[error("Connection failed to {host}")]
    ConnectionFailed { host: String },

    #[error("Invalid configuration: {0}")]
    InvalidConfig(&'static str),
}

// 2. Use domain-specific errors internally
async fn connect_internal(&self, host: &str) -> MyResult<Connection> {
    client.connect(host)
        .change_context(MyError::ConnectionFailed {
            host: host.to_string()
        })
}

// 3. Convert at API boundaries
async fn connect(&self, host: &str) -> Result<Connection> {
    self.connect_internal(host)
        .await
        .change_context(PluginError::Execution)
}
```

See `CLAUDE.md` for detailed architecture and advanced patterns.

### Testing

- Place unit tests in the same file as the code they test
- Use `#[cfg(test)]` for test modules
- Follow the pattern: `mod tests { ... }`
- Place integration tests in the `tests/` directory
- Test both success and failure cases

### Logging and Tracing

- Use the `tracing` package for all logging and instrumentation
- Use appropriate log levels (error, warn, info, debug, trace)
- Include relevant context in log messages
- Use structured logging where appropriate
- Use spans for tracking operation context
- Use events for discrete log messages

## Making Contributions

### Types of Contributions

We welcome various types of contributions:

- **Bug fixes** - Fix existing issues
- **New features** - Add new functionality
- **Documentation** - Improve docs, examples, or comments
- **Performance** - Optimize existing code
- **Testing** - Add or improve tests
- **Examples** - Create new workflow examples

### Before You Start

1. **Check existing issues** - Look for related issues or discussions
2. **Create an issue** - For significant changes, create an issue first to discuss the approach
3. **Fork the repository** - Create your own fork to work in

### Development Workflow

1. **Create a feature branch**
   ```bash
   git checkout -b feature/your-feature-name
   ```

2. **Make your changes**
   - Write code following our conventions
   - Add tests for new functionality
   - Update documentation as needed

3. **Test your changes**
   ```bash
   cd stepflow-rs
   cargo test
   cargo clippy
   cargo fmt --check
   ```

4. **Commit your changes**
   ```bash
   git add .
   git commit -m "feat: add your feature description"
   git push origin feature/your-feature-name
   ```

   **Note**: Pre-commit hooks will automatically run and check:
   - Code formatting (rustfmt, black, isort)
   - Linting (clippy, flake8)
   - ICLA signature status

   If you haven't signed the ICLA, the commit will be blocked with instructions.

### Commit Message Format

Use conventional commit prefixes:

- `feat:` - New features
- `fix:` - Bug fixes
- `docs:` - Documentation changes
- `style:` - Formatting changes
- `refactor:` - Code refactoring
- `test:` - Adding or modifying tests
- `chore:` - Maintenance tasks

**Format**: `type: description`

**Example**: `feat: add support for HTTP-based plugins`

**Full Specification**: [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/)

## Submitting Changes

### Pull Request Process

1. **Push your branch** to your fork
2. **Create a pull request** from your branch to `main`
3. **Fill out the PR template** with:
   - Clear description of changes
   - Link to related issues
   - Testing notes
   - Breaking changes (if any)

### PR Requirements

Your pull request should:

- ✅ **ICLA signed** - Individual Contributor License Agreement must be signed
- ✅ Pass all existing tests
- ✅ Include tests for new functionality
- ✅ Pass `cargo clippy` without warnings
- ✅ Be formatted with `cargo fmt`
- ✅ Include appropriate documentation
- ✅ Have a clear commit history

**Note**: The ICLA check will run automatically on your PR. If you haven't signed it, the check will fail with instructions on how to sign.

### Review Process

1. **Automated checks** will run on your PR
2. **Maintainers will review** your changes
3. **Address feedback** by pushing additional commits
4. **Squash and merge** once approved

## Getting Help

- **Issues**: [GitHub Issues](https://github.com/stepflow-ai/stepflow/issues)
- **Discussions**: [GitHub Discussions](https://github.com/stepflow-ai/stepflow/discussions)
- **Documentation**: [Project Docs](https://fuzzy-journey-4j3y1we.pages.github.io/)

## Development Tips

### Common Commands

```bash
# Quick development cycle
cargo check          # Fast syntax check
cargo test            # Run tests
cargo clippy          # Check for issues
cargo fmt             # Format code

# Working with examples (run from stepflow-rs directory)
cd stepflow-rs
cargo run -- run --flow=examples/python/basic.yaml --input=examples/python/input1.json

# Debugging
RUST_LOG=debug cargo run -- run --flow=your-flow.yaml --input=your-input.json
```

### IDE Setup

For VS Code users, we recommend:

- `rust-analyzer` extension
- `Better TOML` extension
- Configure format-on-save for consistent formatting

Thank you for contributing to Stepflow! 🚀
