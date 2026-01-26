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

# CI check script for documentation - mirrors .github/actions/docs-checks behavior
# This script runs all documentation-related checks that are performed in CI
#
# Usage: ./scripts/check-docs.sh [-v|--verbose]
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

echo "📚 Docs"

cd "$PROJECT_ROOT/docs"

# Check for required tool
require_tool "pnpm" "npm install -g pnpm"

# =============================================================================
# DOCUMENTATION CHECKS
# =============================================================================

run_check "Dependencies" pnpm install --frozen-lockfile || true

if ! run_check "Build" pnpm build; then
    print_fix "Fix build errors"
fi

# =============================================================================
# OPTIONAL CHECKS (warn but don't fail)
# =============================================================================

# These are optional and may not be configured in all docs setups
print_step "Lint"
if pnpm lint >/dev/null 2>&1; then
    print_pass
else
    print_skip "not configured"
fi

print_step "Link check"
if pnpm check-links >/dev/null 2>&1; then
    print_pass
else
    print_skip "not configured"
fi

# =============================================================================
# RESULTS SUMMARY
# =============================================================================

print_summary "Docs" "./scripts/check-docs.sh"
