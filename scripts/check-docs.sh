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

# CI check script for documentation - mirrors .github/actions/docs-checks behavior
# This script runs all documentation-related checks that are performed in CI

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
echo "📚 Running documentation CI checks..."

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT/docs"

# Track results of all checks
FAILED_CHECKS=()

# =============================================================================
# DOCUMENTATION CHECKS (docs-checks action)
# =============================================================================

output "🔧 Setting up Node.js environment..."
if ! command -v pnpm &> /dev/null; then
    echo "❌ pnpm not found. Please install pnpm first:"
    echo "   npm install -g pnpm"
    exit 1
fi

output "📦 Installing documentation dependencies..."
if [ "$QUIET" = true ]; then
    if ! pnpm install --frozen-lockfile >/dev/null 2>&1; then
        echo "❌ Failed to install dependencies"
        FAILED_CHECKS+=("dependencies")
    fi
else
    if ! pnpm install --frozen-lockfile; then
        echo "❌ Failed to install dependencies"
        FAILED_CHECKS+=("dependencies")
    fi
fi

output "🏗️  Building documentation..."
if [ "$QUIET" = true ]; then
    if ! pnpm build >/dev/null 2>&1; then
        echo "❌ Documentation build failed"
        FAILED_CHECKS+=("build")
    fi
else
    if ! pnpm build; then
        echo "❌ Documentation build failed"
        FAILED_CHECKS+=("build")
    fi
fi

# =============================================================================
# ADDITIONAL CHECKS (not in CI but useful for local development)
# =============================================================================

output "🔍 Checking documentation linting..."
if [ "$QUIET" = true ]; then
    pnpm lint >/dev/null 2>&1 || true
else
    if ! pnpm lint 2>/dev/null; then
        echo "⚠️  Documentation linting not available or failed"
        echo "   This is optional and may not be configured"
    fi
fi

output "🔗 Checking for broken links..."
if [ "$QUIET" = true ]; then
    pnpm check-links >/dev/null 2>&1 || true
else
    if ! pnpm check-links 2>/dev/null; then
        echo "⚠️  Link checking not available or failed"
        echo "   This is optional and may not be configured"
    fi
fi

# =============================================================================
# RESULTS SUMMARY
# =============================================================================

echo ""
echo "=== Documentation CI Check Results ==="

if [ ${#FAILED_CHECKS[@]} -eq 0 ]; then
    echo "✅ All documentation checks passed!"
    exit 0
else
    echo "❌ Failed checks: ${FAILED_CHECKS[*]}"
    echo ""
    echo "To fix these issues:"
    for check in "${FAILED_CHECKS[@]}"; do
        case "$check" in
            "dependencies")
                echo "  - Run: pnpm install --frozen-lockfile"
                ;;
            "build")
                echo "  - Fix build errors and run: pnpm build"
                ;;
        esac
    done
    exit 1
fi