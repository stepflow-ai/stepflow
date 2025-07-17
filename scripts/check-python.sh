#!/bin/bash
# Licensed to the Apache Software Foundation (ASF) under one or more contributor
# license agreements.  See the NOTICE file distributed with this work for
# additional information regarding copyright ownership.  The ASF licenses this
# file to you under the Apache License, Version 2.0 (the "License"); you may not
# use this file except in compliance with the License.  You may obtain a copy of
# the License at
#
#   http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.  See the
# License for the specific language governing permissions and limitations under
# the License.

# CI check script for Python SDK - mirrors .github/actions/python-checks behavior
# This script runs all Python-related checks that are performed in CI

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
echo "🐍 Running Python SDK CI checks..."

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT/sdks/python"

# Track results of all checks
FAILED_CHECKS=()

# =============================================================================
# PYTHON SDK CHECKS (python-checks action)
# =============================================================================

output "🔧 Setting up Python environment..."
if ! command -v uv &> /dev/null; then
    echo "❌ uv not found. Please install uv first:"
    echo "   curl -LsSf https://astral.sh/uv/install.sh | sh"
    exit 1
fi

output "🐍 Installing Python and dependencies..."
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

if [ "$QUIET" = true ]; then
    if ! uv sync --extra http >/dev/null 2>&1; then
        echo "❌ Failed to install dependencies"
        FAILED_CHECKS+=("dependencies")
    fi
else
    if ! uv sync --extra http; then
        echo "❌ Failed to install dependencies"
        FAILED_CHECKS+=("dependencies")
    fi
fi

output "🔄 Regenerating Python types (ensure up-to-date)..."
if [ "$QUIET" = true ]; then
    if ! uv run poe codegen-fix >/dev/null 2>&1; then
        echo "❌ Code generation failed"
        FAILED_CHECKS+=("codegen")
    fi
else
    if ! uv run poe codegen-fix; then
        echo "❌ Code generation failed"
        FAILED_CHECKS+=("codegen")
    fi
fi

output "🎨 Checking Python formatting..."
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

output "📝 Running Python linting..."
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

output "🔍 Running Python type checking..."
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

output "📦 Checking Python dependencies..."
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

output "🧪 Running Python tests..."
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
# ADDITIONAL CHECKS (not in CI but useful for local development)
# =============================================================================

output "✅ Verifying generated types are up-to-date..."
if [ "$QUIET" = true ]; then
    if ! uv run poe codegen-check >/dev/null 2>&1; then
        echo "❌ Generated types are out of date. Run 'uv run poe codegen-fix' to fix."
        FAILED_CHECKS+=("codegen-check")
    fi
else
    if ! uv run poe codegen-check; then
        echo "❌ Generated types are out of date. Run 'uv run poe codegen-fix' to fix."
        FAILED_CHECKS+=("codegen-check")
    fi
fi

# =============================================================================
# RESULTS SUMMARY
# =============================================================================

echo ""
echo "=== Python SDK CI Check Results ==="

if [ ${#FAILED_CHECKS[@]} -eq 0 ]; then
    echo "✅ All Python checks passed!"
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
                echo "  - Run: uv sync --extra http"
                ;;
            "codegen")
                echo "  - Run: uv run poe codegen-fix"
                ;;
            "formatting")
                echo "  - Run: uv run poe fmt-fix"
                ;;
            "linting")
                echo "  - Run: uv run poe lint-fix"
                ;;
            "type-check")
                echo "  - Fix type errors and run: uv run poe type-check"
                ;;
            "dep-check")
                echo "  - Fix dependency issues and run: uv run poe dep-check"
                ;;
            "tests")
                echo "  - Fix failing tests and run: uv run poe test"
                ;;
            "codegen-check")
                echo "  - Run: uv run poe codegen-fix"
                ;;
        esac
    done
    exit 1
fi