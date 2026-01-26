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
#
# Usage: ./scripts/check-licenses.sh [-v|--verbose]
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

echo "🔒 Licenses"

cd "$PROJECT_ROOT"

# Check for required tool
require_tool "licensure" "cargo install licensure"

# =============================================================================
# LICENSE HEADER CHECKS
# =============================================================================

if ! run_check "License headers" licensure -c -p; then
    print_fix "licensure -p --in-place"
fi

# =============================================================================
# RESULTS SUMMARY
# =============================================================================

print_summary "License" "./scripts/check-licenses.sh"
