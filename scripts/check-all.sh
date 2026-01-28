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

# Complete CI check suite - runs all checks (Rust, Python, Docs, Licenses, Langflow, Integration)
#
# Usage: ./scripts/check-all.sh [-v|--verbose]
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

echo "🚀 Running CI checks..."

cd "$PROJECT_ROOT"

# Track results of all check categories
FAILED_CATEGORIES=()

# Build verbose flag to pass to sub-scripts
VERBOSE_FLAG=""
if [ "$VERBOSE" = true ]; then
    VERBOSE_FLAG="-v"
fi

# =============================================================================
# RUN ALL CHECKS
# =============================================================================

run_category() {
    local name="$1"
    local script="$2"

    echo ""
    if "$SCRIPT_DIR/$script" $VERBOSE_FLAG; then
        return 0
    else
        FAILED_CATEGORIES+=("$name")
        return 1
    fi
}

# Run each category - continue even if one fails
run_category "rust" "check-rust.sh" || true
run_category "python" "check-python.sh" || true
run_category "docs" "check-docs.sh" || true
run_category "licenses" "check-licenses.sh" || true
run_category "langflow" "check-langflow.sh" || true
run_category "integration" "test-integration.sh" || true

# =============================================================================
# OVERALL RESULTS SUMMARY
# =============================================================================

echo ""
echo "═══════════════════════════════════════════════════════════════════════════════"
echo "📊 OVERALL CI CHECK RESULTS"
echo "═══════════════════════════════════════════════════════════════════════════════"

if [ ${#FAILED_CATEGORIES[@]} -eq 0 ]; then
    echo "✅ All CI checks passed!"
    exit 0
else
    echo "❌ ${#FAILED_CATEGORIES[@]} check(s) failed: ${FAILED_CATEGORIES[*]}"
    echo ""
    echo "To debug failures, run with verbose flag:"
    for category in "${FAILED_CATEGORIES[@]}"; do
        case "$category" in
            "rust")
                echo "  ./scripts/check-rust.sh -v"
                ;;
            "python")
                echo "  ./scripts/check-python.sh -v"
                ;;
            "docs")
                echo "  ./scripts/check-docs.sh -v"
                ;;
            "licenses")
                echo "  ./scripts/check-licenses.sh -v"
                ;;
            "langflow")
                echo "  ./scripts/check-langflow.sh -v"
                ;;
            "integration")
                echo "  ./scripts/test-integration.sh -v"
                ;;
        esac
    done
    exit 1
fi
