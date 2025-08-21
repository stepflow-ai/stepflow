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

# CI check script for license headers - mirrors CI license check behavior
# This script runs license header validation that is performed in CI

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
echo "🔒 Running license header checks..."

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

# Track results of all checks
FAILED_CHECKS=()

# =============================================================================
# LICENSE HEADER CHECKS (licensure)
# =============================================================================

output "🔍 Checking license headers..."
if ! command -v licensure &> /dev/null; then
    echo "❌ licensure not found. Please install licensure first:"
    echo "   cargo install licensure"
    exit 1
fi

if [ "$QUIET" = true ]; then
    if ! licensure -c -p >/dev/null 2>&1; then
        echo "❌ License header check failed. Run 'licensure -p --in-place' to fix."
        FAILED_CHECKS+=("license-headers")
    fi
else
    if ! licensure -c -p; then
        echo "❌ License header check failed. Run 'licensure -p --in-place' to fix."
        FAILED_CHECKS+=("license-headers")
    fi
fi

# =============================================================================
# RESULTS SUMMARY
# =============================================================================

echo ""
echo "=== License Header Check Results ==="

if [ ${#FAILED_CHECKS[@]} -eq 0 ]; then
    echo "✅ All license header checks passed!"
    exit 0
else
    echo "❌ Failed checks: ${FAILED_CHECKS[*]}"
    echo ""
    echo "To fix these issues:"
    for check in "${FAILED_CHECKS[@]}"; do
        case "$check" in
            "license-headers")
                echo "  - Run: licensure -p --in-place"
                ;;
        esac
    done
    exit 1
fi