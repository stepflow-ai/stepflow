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

set -e

# Parse command line arguments
QUIET=false
while [[ $# -gt 0 ]]; do
    case $1 in
        --quiet|-q)
            QUIET=true
            shift
            ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: $0 [--quiet|-q]"
            exit 1
            ;;
    esac
done

# Output function that respects quiet mode
output() {
    if [ "$QUIET" = false ]; then
        echo "$@"
    fi
}

# Always show this header
echo "🔥 Running Langflow Integration CI checks..."

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT/integrations/langflow"

# Track results of all checks
FAILED_CHECKS=()

# =============================================================================
# LANGFLOW INTEGRATION CHECKS (langflow-checks action)
# =============================================================================

output "🔧 Setting up Python environment..."
if ! command -v uv &> /dev/null; then
    echo "❌ uv not found. Please install uv first:"
    echo "   curl -LsSf https://astral.sh/uv/install.sh | sh"
    exit 1
fi

output "🐍 Installing Python and dependencies..."
# Skip Python installation if Python is already available and working
if ! python3 --version >/dev/null 2>&1; then
    if [ "$QUIET" = true ]; then
        if ! uv python install >/dev/null 2>&1; then
            echo "❌ Failed to install Python"
            FAILED_CHECKS+=("python-install")
        fi
    else
        if ! uv python install; then
            echo "❌ Failed to install Python"
            FAILED_CHECKS+=("python-install")
        fi
    fi
else
    output "Python already available, skipping installation"
fi

if [ "$QUIET" = true ]; then
    if ! uv sync >/dev/null 2>&1; then
        echo "❌ Failed to install dependencies"
        FAILED_CHECKS+=("dependencies")
    fi
else
    if ! uv sync; then
        echo "❌ Failed to install dependencies"
        FAILED_CHECKS+=("dependencies")
    fi
fi

output "🎨 Checking Langflow formatting..."
if [ "$QUIET" = true ]; then
    if ! uv run poe fmt-check >/dev/null 2>&1; then
        echo "❌ Formatting check failed. Run 'uv run poe fmt-fix' to fix."
        FAILED_CHECKS+=("formatting")
    fi
else
    if ! uv run poe fmt-check; then
        echo "❌ Formatting check failed. Run 'uv run poe fmt-fix' to fix."
        FAILED_CHECKS+=("formatting")
    fi
fi

output "📝 Running Langflow linting..."
if [ "$QUIET" = true ]; then
    if ! uv run poe lint-check >/dev/null 2>&1; then
        echo "❌ Linting check failed. Run 'uv run poe lint-fix' to fix."
        FAILED_CHECKS+=("linting")
    fi
else
    if ! uv run poe lint-check; then
        echo "❌ Linting check failed. Run 'uv run poe lint-fix' to fix."
        FAILED_CHECKS+=("linting")
    fi
fi

output "🔍 Running Langflow type checking..."
if [ "$QUIET" = true ]; then
    if ! uv run poe type-check >/dev/null 2>&1; then
        echo "❌ Type checking failed"
        FAILED_CHECKS+=("type-check")
    fi
else
    if ! uv run poe type-check; then
        echo "❌ Type checking failed"
        FAILED_CHECKS+=("type-check")
    fi
fi

output "📦 Checking Langflow dependencies..."
if [ "$QUIET" = true ]; then
    if ! uv run poe dep-check >/dev/null 2>&1; then
        echo "❌ Dependency check failed"
        FAILED_CHECKS+=("dep-check")
    fi
else
    if ! uv run poe dep-check; then
        echo "❌ Dependency check failed"
        FAILED_CHECKS+=("dep-check")
    fi
fi

output "🧪 Running Langflow tests..."
if [ "$QUIET" = true ]; then
    if ! uv run poe test >/dev/null 2>&1; then
        echo "❌ Tests failed"
        FAILED_CHECKS+=("tests")
    fi
else
    if ! uv run poe test; then
        echo "❌ Tests failed"
        FAILED_CHECKS+=("tests")
    fi
fi

# =============================================================================
# INTEGRATION TEST (requires stepflow binary)
# =============================================================================

output "🚀 Running Langflow integration tests..."

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
    output "Using stepflow binary: $STEPFLOW_BINARY"
    export STEPFLOW_BINARY_PATH="$STEPFLOW_BINARY"
    
    # Run the langflow integration tests directly (skip slow tests for CI)
    if [ "$QUIET" = true ]; then
        if ! uv run python -m pytest tests/integration/ -v -m "not slow" -x >/dev/null 2>&1; then
            echo "❌ Langflow integration tests failed"
            FAILED_CHECKS+=("integration-tests")
        fi
    else
        if ! uv run python -m pytest tests/integration/ -v -m "not slow" -x; then
            echo "❌ Langflow integration tests failed"
            FAILED_CHECKS+=("integration-tests")
        fi
    fi
else
    echo "⚠️  Stepflow binary not found, skipping integration tests"
    echo "   To run integration tests, build stepflow binary first:"
    echo "   cd stepflow-rs && cargo build"
fi

# =============================================================================
# RESULTS SUMMARY
# =============================================================================

echo ""
echo "=== Langflow Integration CI Check Results ==="

if [ ${#FAILED_CHECKS[@]} -eq 0 ]; then
    echo "✅ All Langflow checks passed!"
    exit 0
else
    echo "❌ Failed checks: ${FAILED_CHECKS[*]}"
    echo ""
    echo "To fix these issues:"
    for check in "${FAILED_CHECKS[@]}"; do
        case "$check" in
            "python-install")
                echo "  - Ensure uv is installed and working"
                ;;
            "dependencies")
                echo "  - Run: cd integrations/langflow && uv sync"
                ;;
            "formatting")
                echo "  - Run: cd integrations/langflow && uv run poe fmt-fix"
                ;;
            "linting")
                echo "  - Run: cd integrations/langflow && uv run poe lint-fix"
                ;;
            "type-check")
                echo "  - Fix type errors and run: cd integrations/langflow && uv run poe type-check"
                ;;
            "dep-check")
                echo "  - Fix dependency issues and run: cd integrations/langflow && uv run poe dep-check"
                ;;
            "tests")
                echo "  - Fix failing tests and run: cd integrations/langflow && uv run poe test"
                ;;
            "integration-tests")
                echo "  - Build stepflow binary: cd stepflow-rs && cargo build"
                echo "  - Run: cd integrations/langflow && uv run python -m pytest tests/integration/ -v"
                ;;
        esac
    done
    exit 1
fi