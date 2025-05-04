# Step Flow Conventions

This document outlines the conventions used in the Step Flow project to maintain consistency and quality across the codebase.

## Code Style

### Rust Code
- Use `rustfmt` for consistent code formatting
- Follow the Rust API Guidelines (https://rust-lang.github.io/api-guidelines/)
- Use `clippy` for linting
- Maximum line length: 100 characters

## Documentation

### Code Documentation
- Use `///` for public API documentation
- Use `//!` for module-level documentation
- Include examples in documentation where appropriate
- Document all public types, functions, and traits
- Use markdown in documentation comments

### Error Handling
- Define custom error types in `error.rs` at the crate root (or in appropriate modules)
- Include a `type Result<T, E = TheErrorType> = std::result::Result<T, E>` alias
- Use `thiserror` for defining error types
- Include context in error messages
- Document error variants and their meanings

## Git Workflow

### Commit Messages
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
- Use the body to explain what and why, not how
- Reference issues using #<issue_number>

## Testing

### Unit Tests
- Place tests in the same file as the code they test
- Use `#[test]` for unit tests
- Use `#[cfg(test)]` for test modules
- Follow the pattern: `mod tests { ... }`

### Integration Tests
- Place integration tests in the `tests/` directory
- Use descriptive test names
- Test both success and failure cases

## Project Structure

### Crate Organization
- Keep related functionality in the same crate
- Use workspaces for managing multiple crates
- Follow the established directory structure:
  - `crates/` for all project crates
  - `examples/` for example code

### Dependencies
- Keep dependencies minimal and well-documented
- Use workspace dependencies where appropriate
- Document the purpose of each dependency
- Keep dependencies up to date

## Error Messages

### User-Facing Messages
- Be clear and concise
- Provide actionable information
- Use consistent formatting
- Include relevant context

### Logging and Tracing
- Use the `tracing` package for all logging and instrumentation
- Use appropriate log levels (error, warn, info, debug, trace)
- Include relevant context in log messages
- Use structured logging where appropriate
- Use spans for tracking operation context
- Use events for discrete log messages
