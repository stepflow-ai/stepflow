# CI Check Scripts

This directory contains scripts that mirror the CI pipeline checks for local development and testing.

## Available Scripts

### Component-Specific Checks

- **`check-rust.sh`** - Runs all Rust-related CI checks
  - Formatting (`cargo fmt --check`)
  - Security audit (`cargo deny check`)
  - Unused dependencies (`cargo machete`)
  - Tests (`cargo test`)
  - Linting (`cargo clippy`)
  - Compilation (`cargo check`)
  - Documentation (`cargo doc`)

- **`check-python.sh`** - Runs all Python SDK CI checks
  - Code generation (`uv run poe codegen-fix`)
  - Formatting (`uv run poe fmt-check`)
  - Linting (`uv run poe lint-check`)
  - Type checking (`uv run poe type-check`)
  - Dependency checking (`uv run poe dep-check`)
  - Tests (`uv run poe test`)

- **`check-docs.sh`** - Runs documentation CI checks
  - Documentation build (`pnpm build`)
  - Optional linting and link checking

- **`check-licenses.sh`** - Runs license header validation
  - License header checking (`licensure -c -p`)

### Integration Tests

- **`test-integration.sh`** - Runs integration tests (existing)
- **`test-all.sh`** - Runs all tests (existing)
- **`test-python-versions.sh`** - Tests Python SDK across multiple Python versions (3.11, 3.12, 3.13)

### Complete CI Pipeline

- **`check-all.sh`** - Runs all CI checks in sequence
  - Executes all component checks
  - Runs integration tests
  - Provides comprehensive status report

## Usage

### Run All Checks (Recommended)
```bash
./scripts/check-all.sh
```

### Run Individual Component Checks
```bash
./scripts/check-rust.sh      # Rust checks only
./scripts/check-python.sh    # Python checks only
./scripts/check-docs.sh      # Documentation checks only
./scripts/check-licenses.sh  # License checks only
```

### Quiet Mode
All individual check scripts support a `--quiet` flag to suppress detailed output and only show errors and final results:

```bash
./scripts/check-rust.sh --quiet      # Only show errors and final status
./scripts/check-python.sh --quiet    # Only show errors and final status
./scripts/check-docs.sh --quiet      # Only show errors and final status
./scripts/check-licenses.sh --quiet  # Only show errors and final status
```

The `check-all.sh` script runs individual checks in quiet mode by default for a cleaner summary. Use `--verbose` to see detailed output:

```bash
./scripts/check-all.sh          # Clean summary output (default)
./scripts/check-all.sh --verbose # Detailed output from all checks
```

### Run Tests
```bash
./scripts/test-integration.sh     # Integration tests
./scripts/test-all.sh             # All tests
./scripts/test-python-versions.sh # Test Python SDK across Python 3.11, 3.12, 3.13
```

## CI Integration

These scripts are designed to mirror the behavior of the GitHub Actions CI pipeline. The CI workflow has been updated to call these scripts directly:

- **`check-rust.sh`** ↔ CI rust-checks job
- **`check-python.sh`** ↔ CI python-checks job
- **`check-docs.sh`** ↔ CI docs-checks job
- **`check-licenses.sh`** ↔ CI licensure job
- **`test-integration.sh`** ↔ CI integration-checks job

### CI Implementation

The CI workflow now calls these scripts directly, ensuring perfect consistency between local and CI environments. This means:

**Benefits of this approach:**
- ✅ **Perfect Consistency**: Identical logic runs locally and in CI
- ✅ **Maintainability**: Single source of truth for all checks
- ✅ **Debugging**: Developers can reproduce CI failures exactly
- ✅ **Simplicity**: No duplication of check logic
- ✅ **Reliability**: Same scripts tested locally are used in CI

## Prerequisites

### Rust Checks
- Rust toolchain (stable)
- Optional: `cargo-deny`, `cargo-machete` for additional checks

### Python Checks
- `uv` package manager
- Python 3.11+ (supports 3.11, 3.12, 3.13)

### Documentation Checks
- Node.js
- `pnpm` package manager

### License Checks
- `licensure` tool (`cargo install licensure`)

## Development Workflow

1. **Before committing**: Run `./scripts/check-all.sh` to ensure all checks pass
2. **For specific changes**: Run the relevant component script
3. **For CI debugging**: Run the same script that's failing in CI

## Error Handling

All scripts provide:
- ✅ Clear success/failure indication
- ❌ Detailed error messages
- 🔧 Helpful fix suggestions
- 📊 Comprehensive status summaries

Scripts continue running all checks even if some fail, providing complete feedback in a single run.