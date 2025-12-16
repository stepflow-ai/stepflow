---
sidebar_position: 100
---

# Contributing to Stepflow

Thank you for your interest in contributing to Stepflow! We welcome contributions of all kinds—from bug fixes and new features to documentation improvements and examples.

This guide will help you get started quickly. For comprehensive development guidelines, see the full [CONTRIBUTING.md](https://github.com/stepflow-ai/stepflow/blob/main/CONTRIBUTING.md) in the repository.

## Ways to Contribute

There are many ways to contribute to Stepflow:

- 🐛 **Report bugs** - Help us identify and fix issues
- ✨ **Suggest features** - Share ideas for new functionality
- 📝 **Improve documentation** - Help others understand Stepflow better
- 🔧 **Submit code** - Fix bugs or implement new features
- 💡 **Create examples** - Show how to use Stepflow in different scenarios
- 🧪 **Write tests** - Improve code quality and coverage

## Quick Start

### Prerequisites

Before you begin, make sure you have:

- **Rust 1.70+** - Install via [rustup](https://rustup.rs/)
- **Python 3.8+** - For Python SDK development and examples
- **uv** - Python package manager: `pip install uv`

### Getting Started

1. **Fork and clone the repository**
   ```bash
   git clone https://github.com/YOUR_USERNAME/stepflow.git
   cd stepflow
   ```

2. **Sign the Individual Contributor License Agreement (ICLA)**

   :::danger REQUIRED BEFORE FIRST CONTRIBUTION
   
   Before making your first contribution, you **must** sign the ICLA:
   
   ```bash
   python scripts/sign_icla.py
   ```
   
   The ICLA is a one-time legal agreement that:
   - Grants the project rights to use your contributions
   - Confirms you own the copyright to your work
   - Ensures clear licensing for all project code
   
   **Your PR will be blocked until the ICLA is signed.** For full details, see [ICLA.md](https://github.com/stepflow-ai/stepflow/blob/main/ICLA.md).
   
   :::

3. **Build the project**
   ```bash
   cd stepflow-rs
   cargo build
   ```

4. **Run tests to verify setup**
   ```bash
   cargo test
   ```

5. **Try running an example**
   ```bash
   cargo run -- run --flow=examples/basic/workflow.yaml \
     --input=examples/basic/input1.json \
     --config=examples/basic/stepflow-config.yml
   ```

## Development Workflow

### 1. Create a Feature Branch

```bash
git checkout -b feature/your-feature-name
```

### 2. Make Your Changes

- Write code following our [conventions](#code-standards)
- Add tests for new functionality
- Update documentation as needed

### 3. Test Your Changes

```bash
# Run unit tests
cd stepflow-rs
cargo test

# Run with snapshot testing (recommended)
cargo insta test --unreferenced=delete --review

# Check code quality
cargo clippy
cargo fmt --check
```

### 4. Commit Your Changes

Use [conventional commit](https://www.conventionalcommits.org/) format:

```bash
git add .
git commit -m "feat: add your feature description"
```

**Commit prefixes:**
- `feat:` - New features
- `fix:` - Bug fixes
- `docs:` - Documentation changes
- `style:` - Formatting changes
- `refactor:` - Code refactoring
- `test:` - Adding or modifying tests
- `chore:` - Maintenance tasks

:::tip
Pre-commit hooks will automatically check formatting, linting, and ICLA status.
:::

### 5. Push and Create a Pull Request

```bash
git push origin feature/your-feature-name
```

Then create a pull request on GitHub with:
- Clear description of changes
- Link to related issues
- Testing notes
- Any breaking changes

## Code Standards

### Rust Code

- **Formatting**: Use `rustfmt` - run `cargo fmt`
- **Linting**: Use `clippy` - run `cargo clippy`
- **Line length**: Maximum 100 characters
- **Documentation**: Use `///` for public APIs, include examples
- **Testing**: Place unit tests in the same file with `#[cfg(test)]`

### Error Handling

- Define custom error types using `thiserror`
- Use domain-specific errors internally
- Convert to boundary errors at public API boundaries
- Include context in error messages

Example:
```rust
#[derive(Error, Debug)]
pub enum MyError {
    #[error("Connection failed to {host}")]
    ConnectionFailed { host: String },
}
```

### Logging and Tracing

- Use `log` for detailed debugging (state changes, variables)
- Use `fastrace` for high-level execution structure (workflow lifecycle)
- Trace context is automatically injected into logs

## Testing

```bash
# Fast unit tests
cargo test

# Test specific crate
cargo test -p stepflow-execution

# Test specific function
cargo test -p stepflow-execution -- execute_flows

# Integration tests (requires Python)
cd ..
./scripts/test-integration.sh

# Complete test suite
./scripts/test-all.sh
```

## Pull Request Requirements

Your PR should:

- ✅ **ICLA signed** - Required for all contributions
- ✅ Pass all existing tests
- ✅ Include tests for new functionality
- ✅ Pass `cargo clippy` without warnings
- ✅ Be formatted with `cargo fmt`
- ✅ Include appropriate documentation
- ✅ Have a clear commit history

## Project Structure

The repository contains:

- **`stepflow-rs/`** - Main Rust execution engine and runtime
- **`stepflow-ui/`** - Web-based frontend for workflow management
- **`sdks/python/`** - Python SDK for building components
- **`sdks/typescript/`** - TypeScript SDK for building components
- **`docs/`** - Documentation site (Docusaurus)
- **`examples/`** - Example workflows and use cases

### Key Rust Crates

- **`stepflow-core`** - Core types and workflow definitions
- **`stepflow-execution`** - Workflow execution engine
- **`stepflow-plugin`** - Plugin system and trait definitions
- **`stepflow-protocol`** - JSON-RPC communication protocol
- **`stepflow-builtins`** - Built-in component implementations
- **`stepflow-components-mcp`** - MCP integration

## Getting Help

- 💬 **Questions?** Ask in [GitHub Discussions](https://github.com/stepflow-ai/stepflow/discussions)
- 🐛 **Found a bug?** Report it in [GitHub Issues](https://github.com/stepflow-ai/stepflow/issues)
- 📖 **Need more details?** See the full [CONTRIBUTING.md](https://github.com/stepflow-ai/stepflow/blob/main/CONTRIBUTING.md)
- 🌐 **Community resources** - Visit our [Community page](./community.md)

## Development Tips

### Common Commands

```bash
# Quick development cycle
cargo check          # Fast syntax check
cargo test           # Run tests
cargo clippy         # Check for issues
cargo fmt            # Format code

# Working with examples
cd stepflow-rs
cargo run -- run --flow=examples/python/basic.yaml \
  --input=examples/python/input1.json

# Debugging
RUST_LOG=debug cargo run -- run --flow=your-flow.yaml \
  --input=your-input.json
```

### IDE Setup

For VS Code users, we recommend:

- `rust-analyzer` extension
- `Better TOML` extension
- Configure format-on-save for consistent formatting

## Code of Conduct

All contributors are expected to follow our code of conduct principles:

- Be respectful and inclusive
- Welcome newcomers and help them learn
- Focus on constructive feedback
- Assume good intentions

Thank you for contributing to Stepflow! 🚀