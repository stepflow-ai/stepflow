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

# Integration test script - builds stepflow and runs workflow tests
#
# Usage: ./scripts/test-integration.sh [-v|--verbose]
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

echo "🔗 Integration"

cd "$PROJECT_ROOT"

# =============================================================================
# SETUP
# =============================================================================

# Initialize temp directory
_lib_init

# Setup Python environment and build stepflow in parallel
if [ "$VERBOSE" = true ]; then
    # In verbose mode, run sequentially to show output
    print_step "Python deps"
    echo ""
    echo "    Running: cd sdks/python && uv sync --all-extras --group dev"
    if (cd "$PROJECT_ROOT/sdks/python" && uv sync --all-extras --group dev); then
        print_step "Python deps"
        print_pass
    else
        print_step "Python deps"
        print_fail
        FAILED_CHECKS+=("Python deps")
        FAILED_CHECK_CMDS+=("cd sdks/python && uv sync --all-extras --group dev")
    fi

    print_step "Build stepflow"
    echo ""
    echo "    Running: cd stepflow-rs && cargo build"
    if (cd "$PROJECT_ROOT/stepflow-rs" && cargo build); then
        print_step "Build stepflow"
        print_pass
    else
        print_step "Build stepflow"
        print_fail
        FAILED_CHECKS+=("Build stepflow")
        FAILED_CHECK_CMDS+=("cd stepflow-rs && cargo build")
        print_fix "Fix build errors"
    fi
else
    # In quiet mode, run in parallel with output capture
    (cd "$PROJECT_ROOT/sdks/python" && uv sync --all-extras --group dev) > "$_LIB_TMPDIR/python.txt" 2>&1 &
    PYTHON_PID=$!

    (cd "$PROJECT_ROOT/stepflow-rs" && cargo build) > "$_LIB_TMPDIR/cargo.txt" 2>&1 &
    CARGO_PID=$!

    # Wait for both to complete
    print_step "Python deps"
    if wait $PYTHON_PID; then
        print_pass
    else
        print_fail
        FAILED_CHECKS+=("Python deps")
        FAILED_CHECK_CMDS+=("cd sdks/python && uv sync --all-extras --group dev")
        echo "    Output:"
        sed 's/^/      /' "$_LIB_TMPDIR/python.txt" | head -50
    fi

    print_step "Build stepflow"
    if wait $CARGO_PID; then
        print_pass
    else
        print_fail
        FAILED_CHECKS+=("Build stepflow")
        FAILED_CHECK_CMDS+=("cd stepflow-rs && cargo build")
        echo "    Output:"
        sed 's/^/      /' "$_LIB_TMPDIR/cargo.txt" | head -50
        print_fix "Fix build errors"
    fi
fi

# =============================================================================
# RUN WORKFLOW TESTS
# =============================================================================

if ! run_check "Workflow tests" "$PROJECT_ROOT/stepflow-rs/target/debug/stepflow" test "$PROJECT_ROOT/tests" "$PROJECT_ROOT/examples"; then
    print_fix "Fix failing workflow tests"
fi

# =============================================================================
# RESULTS SUMMARY
# =============================================================================

print_summary "Integration" "./scripts/test-integration.sh"
