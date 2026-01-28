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
_LIB_PROJECT_ROOT="$PROJECT_ROOT"

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
    run_check "Python install" uv python install || true
fi

run_check "Dependencies" uv sync || true

# =============================================================================
# LANGFLOW CHECKS
# =============================================================================

if ! run_check "Formatting" uv run poe fmt-check; then
    print_fix "uv run poe fmt-fix"
fi

if ! run_check "Linting" uv run poe lint-check; then
    print_fix "uv run poe lint-fix"
fi

# Clear mypy cache before type checking to avoid cross-project cache corruption.
# This is needed because langflow imports stepflow_py via editable install, and
# the mypy caches can become inconsistent when both projects are checked together.
# Note: Switching to non-editable installs would eliminate this issue, but would
# require reinstalling packages after every change to stepflow_py or stepflow_orchestrator.
rm -rf .mypy_cache

if ! run_check "Type checking" uv run poe type-check; then
    print_fix "Fix type errors"
fi

run_check "Dep check" uv run poe dep-check || true

if ! run_check "Tests" uv run poe test; then
    print_fix "Fix failing tests"
fi

# =============================================================================
# INTEGRATION TESTS (requires stepflow-server binary)
# =============================================================================

# Check if we have the stepflow-server binary available
# If STEPFLOW_DEV_BINARY is set, use it (allows testing with custom binaries)
# Otherwise, build the debug binary automatically to ensure we test the latest code
if [ -n "$STEPFLOW_DEV_BINARY" ] && [ -f "$STEPFLOW_DEV_BINARY" ]; then
    STEPFLOW_BINARY="$STEPFLOW_DEV_BINARY"
    print_verbose "Using STEPFLOW_DEV_BINARY: $STEPFLOW_BINARY"
else
    # Build the debug binary to ensure we're testing the latest code
    print_verbose "Building stepflow-server (debug)..."
    if (cd "$PROJECT_ROOT/stepflow-rs" && cargo build -p stepflow-server --quiet); then
        STEPFLOW_BINARY="$PROJECT_ROOT/stepflow-rs/target/debug/stepflow-server"
        print_verbose "Built: $STEPFLOW_BINARY"
    else
        STEPFLOW_BINARY=""
        print_verbose "⚠️  Build failed"
    fi
fi

if [ -n "$STEPFLOW_BINARY" ]; then
    export STEPFLOW_DEV_BINARY="$STEPFLOW_BINARY"
    if ! run_check "Integration tests" uv run python -m pytest tests/integration/ -v -m "not slow" -x; then
        print_fix "Fix integration test failures"
    fi
else
    print_step "Integration tests"
    print_skip "stepflow-server binary build failed"
    print_verbose "Check cargo build errors above"
fi

# =============================================================================
# RESULTS SUMMARY
# =============================================================================

print_summary "Langflow" "./scripts/check-langflow.sh"
