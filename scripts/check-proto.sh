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

# CI check script for Protocol Buffer definitions.
#
# Checks:
#   1. buf lint — validate proto style and conventions
#   2. buf breaking — detect backwards-incompatible changes (against main branch)
#   3. OpenAPI spec freshness — verify generated spec matches committed version
#   4. OpenAPI lint — validate OpenAPI spec with Redocly CLI
#
# Usage: ./scripts/check-proto.sh [-v|--verbose]
#   -v, --verbose  Show full command output (default: quiet, shows only pass/fail)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Source shared helpers
source "$SCRIPT_DIR/_lib.sh"
_LIB_PROJECT_ROOT="$PROJECT_ROOT"

# Parse command line arguments
parse_flags "$@"

echo "📋 Proto"

# Check for required tool
require_tool "buf" "brew install bufbuild/buf/buf"

cd "$PROJECT_ROOT/proto"

# =============================================================================
# PROTO CHECKS
# =============================================================================

run_check "Lint" buf lint || true

# Detect backwards-incompatible proto changes against the latest release tag.
# Skips gracefully if the latest release has no proto files.
check_breaking() {
    local git_toplevel
    git_toplevel="$(git -C "$PROJECT_ROOT" rev-parse --show-toplevel)"

    # Find the latest release tag to compare against.
    # Breaking changes are measured from the last release, not from main.
    local latest_tag
    latest_tag=$(git -C "$git_toplevel" tag --sort=-creatordate \
        --list 'stepflow-rs-*' \
        | head -n1)

    # In shallow CI checkouts, local tags may be missing. Fall back to the remote.
    if [ -z "$latest_tag" ]; then
        latest_tag=$(git -C "$git_toplevel" ls-remote --tags --refs origin 'refs/tags/stepflow-rs-*' 2>/dev/null \
            | awk '{print $2}' \
            | sed 's#refs/tags/##' \
            | sort -V \
            | tail -n1)

        if [ -z "$latest_tag" ]; then
            echo "No release tags found — skipping breaking change detection"
            return 0
        fi

        # Fetch only the single tag we need.
        if ! git -C "$git_toplevel" fetch --depth=1 origin "refs/tags/${latest_tag}:refs/tags/${latest_tag}" --no-recurse-submodules 2>/dev/null; then
            echo "Warning: failed to fetch release tag '$latest_tag' — skipping breaking change detection"
            return 0
        fi
    fi

    # Check if the release tag actually has any .proto files
    if ! git -C "$git_toplevel" ls-tree -r --name-only "$latest_tag" proto 2>/dev/null | grep -q '\.proto$'; then
        echo "No proto files in latest release ($latest_tag) — skipping breaking change detection"
        return 0
    fi

    buf breaking --against "${git_toplevel}/.git#tag=${latest_tag},subdir=proto" 2>&1
}

run_check "Breaking changes" \
    --fix "Review breaking changes and update proto files" \
    check_breaking || true

# Return to project root so reproduce/fix commands aren't prefixed with "cd proto &&"
cd "$PROJECT_ROOT"

# Check OpenAPI spec freshness by regenerating to a temp file and diffing.
check_openapi_freshness() {
    local committed="$PROJECT_ROOT/schemas/openapi.yaml"

    if [ ! -f "$committed" ]; then
        echo "No committed openapi.yaml found — skipping freshness check"
        return 0
    fi

    local tmpfile
    tmpfile=$(mktemp)
    trap "rm -f $tmpfile" RETURN

    "$SCRIPT_DIR/generate-openapi-proto.sh" "$tmpfile" > /dev/null 2>&1

    if diff -q "$committed" "$tmpfile" > /dev/null 2>&1; then
        return 0
    else
        echo "OpenAPI spec is out of date. Regenerate with: ./scripts/generate-openapi-proto.sh"
        return 1
    fi
}

run_check "OpenAPI freshness" \
    --fix "./scripts/generate-openapi-proto.sh" \
    check_openapi_freshness || true

# Lint the OpenAPI spec with Redocly CLI (if npx is available).
check_openapi_lint() {
    if ! command -v npx &>/dev/null; then
        echo "npx not found — skipping OpenAPI lint"
        return 0
    fi

    local spec="$PROJECT_ROOT/schemas/openapi.yaml"
    if [ ! -f "$spec" ]; then
        echo "No openapi.yaml found — skipping lint"
        return 0
    fi

    npx @redocly/cli lint "$spec" --config "$PROJECT_ROOT/.redocly.yaml" 2>&1
}

run_check "OpenAPI lint" check_openapi_lint || true

# Check Python proto stubs freshness by regenerating to a temp dir and diffing.
check_python_proto_freshness() {
    local committed="$PROJECT_ROOT/sdks/python/stepflow-py/src/stepflow_py/proto"

    if [ ! -d "$committed" ]; then
        echo "No committed Python proto stubs found — skipping freshness check"
        return 0
    fi

    require_tool "uv" "curl -LsSf https://astral.sh/uv/install.sh | sh"

    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" RETURN

    # Copy committed stubs to temp dir for comparison
    cp -r "$committed" "$tmpdir/before"

    # Regenerate stubs
    "$SCRIPT_DIR/generate-python-proto.sh" > /dev/null 2>&1

    if diff -rq --exclude='__pycache__' "$tmpdir/before" "$committed" > /dev/null 2>&1; then
        return 0
    else
        echo "Python proto stubs are out of date. Regenerate with: ./scripts/generate-python-proto.sh"
        # Restore the committed version so the working tree stays clean
        rm -rf "$committed"
        cp -r "$tmpdir/before" "$committed"
        return 1
    fi
}

run_check "Python proto freshness" \
    --fix "./scripts/generate-python-proto.sh" \
    check_python_proto_freshness || true

# =============================================================================
# RESULTS SUMMARY
# =============================================================================

print_summary "Proto" "./scripts/check-proto.sh"
