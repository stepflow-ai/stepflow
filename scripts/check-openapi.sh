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

# CI check script for OpenAPI schema - validates schemas/openapi.json with Redocly CLI
#
# Usage: ./scripts/check-openapi.sh [-v|--verbose]
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

echo "📐 OpenAPI"

cd "$PROJECT_ROOT"

# Check for required tool
require_tool "npx" "install Node.js (https://nodejs.org/)"

# =============================================================================
# OPENAPI CHECKS
# =============================================================================

run_check "Lint" npx @redocly/cli lint schemas/openapi.json --config .redocly.yaml || true

# =============================================================================
# RESULTS SUMMARY
# =============================================================================

print_summary "OpenAPI" "./scripts/check-openapi.sh"
