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

# Recovery integration test script - builds stepflow-server and runs
# Docker Compose recovery tests.
#
# Usage: ./scripts/check-recovery.sh [-v|--verbose]
#   -v, --verbose  Show full command output (default: quiet, shows only pass/fail)
#
# On Linux: builds the binary on the host and uses a prebuilt Docker image
#           (fast, avoids multi-stage Docker build)
# On macOS: skips host build; Docker Compose builds from source inside the
#           container using the standard multi-stage Dockerfile

set -e

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Source shared helpers
source "$SCRIPT_DIR/_lib.sh"
_LIB_PROJECT_ROOT="$PROJECT_ROOT"

# Parse command line arguments
parse_flags "$@"

echo "🔄 Recovery"

cd "$PROJECT_ROOT"

# =============================================================================
# SETUP
# =============================================================================

_lib_init

RECOVERY_DIR="$PROJECT_ROOT/tests/recovery"

# Always clean up Docker Compose on exit
cleanup() {
    echo ""
    echo "Cleaning up Docker Compose..."
    (cd "$RECOVERY_DIR" && docker compose -f docker-compose.yml down -v --timeout 5 2>/dev/null) || true
}
trap cleanup EXIT

# Detect host OS to decide build strategy
USE_PREBUILT=false
if [ "$(uname -s)" = "Linux" ]; then
    USE_PREBUILT=true
fi

if [ "$USE_PREBUILT" = true ]; then
    # Linux: build binary on host and use prebuilt Docker image
    if [ "$VERBOSE" = true ]; then
        print_step "Python deps"
        echo ""
        echo "    Running: cd tests/recovery && uv sync"
        if (cd "$RECOVERY_DIR" && uv sync); then
            print_step "Python deps"
            print_pass
        else
            print_step "Python deps"
            print_fail
            FAILED_CHECKS+=("Python deps")
            FAILED_CHECK_CMDS+=("cd tests/recovery && uv sync")
        fi

        print_step "Build server"
        echo ""
        echo "    Running: cd stepflow-rs && cargo build -p stepflow-server"
        if (cd "$PROJECT_ROOT/stepflow-rs" && cargo build -p stepflow-server); then
            print_step "Build server"
            print_pass
        else
            print_step "Build server"
            print_fail
            FAILED_CHECKS+=("Build server")
            FAILED_CHECK_CMDS+=("cd stepflow-rs && cargo build -p stepflow-server")
            print_fix "Fix build errors"
        fi
    else
        # Quiet mode: run in parallel
        (cd "$RECOVERY_DIR" && uv sync) > "$_LIB_TMPDIR/python.txt" 2>&1 &
        PYTHON_PID=$!

        (cd "$PROJECT_ROOT/stepflow-rs" && cargo build -p stepflow-server) > "$_LIB_TMPDIR/cargo.txt" 2>&1 &
        CARGO_PID=$!

        print_step "Python deps"
        if wait $PYTHON_PID; then
            print_pass
        else
            print_fail
            FAILED_CHECKS+=("Python deps")
            FAILED_CHECK_CMDS+=("cd tests/recovery && uv sync")
            echo "    Output:"
            sed 's/^/      /' "$_LIB_TMPDIR/python.txt" | head -50
        fi

        print_step "Build server"
        if wait $CARGO_PID; then
            print_pass
        else
            print_fail
            FAILED_CHECKS+=("Build server")
            FAILED_CHECK_CMDS+=("cd stepflow-rs && cargo build -p stepflow-server")
            echo "    Output:"
            sed 's/^/      /' "$_LIB_TMPDIR/cargo.txt" | head -50
            print_fix "Fix build errors"
        fi
    fi

    # Copy binary to test directory for the prebuilt Dockerfile
    cp "$PROJECT_ROOT/stepflow-rs/target/debug/stepflow-server" "$RECOVERY_DIR/stepflow-server"

    export COMPOSE_OVERRIDE=docker-compose.ci.yml
else
    # macOS/other: install Python deps only; Docker builds from source
    if [ "$VERBOSE" = true ]; then
        print_step "Python deps"
        echo ""
        echo "    Running: cd tests/recovery && uv sync"
        if (cd "$RECOVERY_DIR" && uv sync); then
            print_step "Python deps"
            print_pass
        else
            print_step "Python deps"
            print_fail
            FAILED_CHECKS+=("Python deps")
            FAILED_CHECK_CMDS+=("cd tests/recovery && uv sync")
        fi
    else
        print_step "Python deps"
        if (cd "$RECOVERY_DIR" && uv sync) > "$_LIB_TMPDIR/python.txt" 2>&1; then
            print_pass
        else
            print_fail
            FAILED_CHECKS+=("Python deps")
            FAILED_CHECK_CMDS+=("cd tests/recovery && uv sync")
            echo "    Output:"
            sed 's/^/      /' "$_LIB_TMPDIR/python.txt" | head -50
        fi
    fi
fi

# =============================================================================
# DOCKER BUILD (pre-build images so pytest doesn't timeout during build)
# =============================================================================

cd "$RECOVERY_DIR"

COMPOSE_CMD=(docker compose -f docker-compose.yml)
if [ -n "$COMPOSE_OVERRIDE" ]; then
    COMPOSE_CMD+=(-f "$COMPOSE_OVERRIDE")
fi

if ! run_check "Docker build" "${COMPOSE_CMD[@]}" build; then
    print_fix "Fix Docker build errors"
fi

# =============================================================================
# RUN RECOVERY TESTS
# =============================================================================

if ! run_check "Recovery tests" uv run pytest -v --timeout=300; then
    print_fix "Fix failing recovery tests"

    # Dump Docker logs for debugging failed tests
    for svc in orchestrator-1 orchestrator-2 worker; do
        if [ -n "$GITHUB_ACTIONS" ]; then
            echo "::group::Docker logs: $svc"
        else
            echo ""
            echo "--- Docker logs: $svc ---"
        fi
        "${COMPOSE_CMD[@]}" logs --no-log-prefix "$svc" 2>/dev/null || true
        if [ -n "$GITHUB_ACTIONS" ]; then
            echo "::endgroup::"
        fi
    done
fi

# =============================================================================
# RESULTS SUMMARY
# =============================================================================

print_summary "Recovery" "./scripts/check-recovery.sh"
