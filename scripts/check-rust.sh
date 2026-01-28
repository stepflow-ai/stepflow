#!/bin/bash
# Copyright 2025 DataStax Inc.
#
# Licensed under the Apache License, Version 2.0 (the "License"); you may not
# use this file except in compliance with the License. You may obtain a copy of
# the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
# License for the specific language governing permissions and limitations under
# the License.

# CI check script for Rust code - mirrors .github/actions/rust-* behavior
# This script runs all Rust-related checks that are performed in CI
#
# Usage: ./scripts/check-rust.sh [-v|--verbose]
#   -v, --verbose  Show full command output (default: quiet, shows only pass/fail)

set -e

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Source shared helpers
source "$SCRIPT_DIR/_lib.sh"
_LIB_PROJECT_ROOT="$PROJECT_ROOT"

# Parse command line arguments
parse_flags "$@"

echo "🦀 Rust"

cd "$PROJECT_ROOT/stepflow-rs"

# =============================================================================
# RUST STYLE & QUALITY CHECKS
# =============================================================================

if ! run_check "Formatting" cargo fmt --check; then
    print_fix "cargo fmt"
fi

if ! run_optional_check "Security audit" "cargo-deny" cargo deny check; then
    print_fix "cargo deny check (review and fix security issues)"
fi

if ! run_optional_check "Unused deps" "cargo-machete" cargo machete --with-metadata; then
    print_fix "cargo machete --fix --with-metadata"
fi

# =============================================================================
# RUST BUILD & TEST CHECKS
# =============================================================================

if ! run_check "Tests" cargo test; then
    print_fix "Fix failing tests"
fi

if ! run_check "Clippy" cargo clippy -- -D warnings; then
    print_fix "cargo clippy --fix"
fi

# =============================================================================
# ADDITIONAL CHECKS (not in CI but useful for local development)
# =============================================================================

if ! run_check "Compilation" cargo check --all-targets --all-features; then
    print_fix "Fix compilation errors"
fi

if ! run_check "Documentation" cargo doc --all --no-deps; then
    print_fix "Fix documentation errors"
fi

# =============================================================================
# RESULTS SUMMARY
# =============================================================================

print_summary "Rust" "./scripts/check-rust.sh"
