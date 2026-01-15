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

# CI check script for Langflow Integration - mirrors .github/actions/langflow-checks behavior
# This script runs all Langflow integration checks that are performed in CI
#
# Usage: ./scripts/check-langflow.sh [-v|--verbose]
#   -v, --verbose  Show full command output (default: quiet, shows only pass/fail)

set -e

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Source shared helpers
source "$SCRIPT_DIR/_lib.sh"

# Parse command line arguments
parse_flags "$@"

echo "⚡ Langflow"

cd "$PROJECT_ROOT/integrations/langflow"

# Check for required tool
require_tool "uv" "curl -LsSf https://astral.sh/uv/install.sh | sh"

# =============================================================================
# LANGFLOW SETUP
# =============================================================================

# Skip Python installation if already available
if ! python3 --version >/dev/null 2>&1; then
    run_check "Python install" uv python install
fi

run_check "Dependencies" uv sync

# =============================================================================
# LANGFLOW CHECKS
# =============================================================================

run_check "Formatting" uv run poe fmt-check
if [ $? -ne 0 ]; then
    print_fix "uv run poe fmt-fix"
fi

run_check "Linting" uv run poe lint-check
if [ $? -ne 0 ]; then
    print_fix "uv run poe lint-fix"
fi

run_check "Type checking" uv run poe type-check
if [ $? -ne 0 ]; then
    print_fix "Fix type errors"
fi

run_check "Dep check" uv run poe dep-check

run_check "Tests" uv run poe test
if [ $? -ne 0 ]; then
    print_fix "Fix failing tests"
    print_rerun "cd integrations/langflow && uv run poe test"
fi

# =============================================================================
# INTEGRATION TESTS (requires stepflow binary)
# =============================================================================

# Check if we have the stepflow binary available
STEPFLOW_BINARY=""
if [ -n "$STEPFLOW_BINARY_PATH" ] && [ -f "$STEPFLOW_BINARY_PATH" ]; then
    STEPFLOW_BINARY="$STEPFLOW_BINARY_PATH"
elif [ -f "$PROJECT_ROOT/stepflow-rs/target/debug/stepflow" ]; then
    STEPFLOW_BINARY="$PROJECT_ROOT/stepflow-rs/target/debug/stepflow"
elif [ -f "$PROJECT_ROOT/stepflow-rs/target/release/stepflow" ]; then
    STEPFLOW_BINARY="$PROJECT_ROOT/stepflow-rs/target/release/stepflow"
fi

if [ -n "$STEPFLOW_BINARY" ]; then
    export STEPFLOW_BINARY_PATH="$STEPFLOW_BINARY"
    run_check "Integration tests" uv run python -m pytest tests/integration/ -v -m "not slow" -x
    if [ $? -ne 0 ]; then
        print_fix "Fix integration test failures"
        print_rerun "cd integrations/langflow && uv run python -m pytest tests/integration/ -v"
    fi
else
    print_step "Integration tests"
    print_skip "stepflow binary not found"
    echo "    Build with: cd stepflow-rs && cargo build"
fi

# =============================================================================
# RESULTS SUMMARY
# =============================================================================

print_summary "Langflow" "./scripts/check-langflow.sh"
