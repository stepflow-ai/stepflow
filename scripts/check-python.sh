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

# CI check script for Python SDK - mirrors .github/actions/python-checks behavior
# This script runs all Python-related checks that are performed in CI
#
# Usage: ./scripts/check-python.sh [-v|--verbose]
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

echo "🐍 Python"

cd "$PROJECT_ROOT/sdks/python"

# Check for required tool
require_tool "uv" "curl -LsSf https://astral.sh/uv/install.sh | sh"

# =============================================================================
# PYTHON SDK SETUP
# =============================================================================
# NOTE: Each check uses `|| true` to continue running all checks even when one fails.
# Failures are tracked by run_check in FAILED_CHECKS array and reported via print_summary,
# which returns the appropriate exit code at the end of the script.

run_check "Python install" uv python install || true
run_check "Dependencies" uv sync --all-extras --group dev || true

# =============================================================================
# PYTHON SDK CHECKS
# =============================================================================

run_check "Codegen" uv run poe codegen-fix || true

run_check "Formatting" --fix "uv run poe fmt-fix" uv run poe fmt-check || true

run_check "Linting" --fix "uv run poe lint-fix" uv run poe lint-check || true

run_check "Type checking" uv run poe type-check || true

run_check "Dep check" uv run poe dep-check || true

run_check "Tests" uv run poe test || true

# =============================================================================
# ADDITIONAL CHECKS
# =============================================================================

run_check "Codegen check" --fix "uv run poe codegen-fix" uv run poe codegen-check || true

# =============================================================================
# RESULTS SUMMARY
# =============================================================================

print_summary "Python" "./scripts/check-python.sh"
