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
# NOTE: Each check uses `|| true` to continue running all checks even when one fails.
# Failures are tracked by run_check in FAILED_CHECKS array and reported via print_summary,
# which returns the appropriate exit code at the end of the script.

run_check "Formatting" --fix "cargo fmt" cargo fmt --check || true

run_optional_check "Security audit" "cargo-deny" cargo deny check || true

run_optional_check "Unused deps" "cargo-machete" --fix "cargo machete --fix --with-metadata" cargo machete --with-metadata || true

# =============================================================================
# RUST BUILD & TEST CHECKS
# =============================================================================

run_check "Tests" cargo test || true

run_check "Clippy" --fix "cargo clippy --fix  # add --allow-dirty if needed" cargo clippy -- -D warnings || true

# =============================================================================
# ADDITIONAL CHECKS (not in CI but useful for local development)
# =============================================================================

run_check "Compilation" cargo check --all-targets --all-features || true

run_check "Documentation" cargo doc --all --no-deps || true

# =============================================================================
# RESULTS SUMMARY
# =============================================================================

print_summary "Rust" "./scripts/check-rust.sh"
