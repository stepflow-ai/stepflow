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

# Shared helper library for check scripts
# Source this file at the start of each check script:
#   source "$(dirname "${BASH_SOURCE[0]}")/_lib.sh"

# Global state
VERBOSE=${VERBOSE:-false}
FAILED_CHECKS=()
_LIB_TMPDIR=""

# Cleanup temp files on exit
_lib_cleanup() {
    if [ -n "$_LIB_TMPDIR" ] && [ -d "$_LIB_TMPDIR" ]; then
        rm -rf "$_LIB_TMPDIR"
    fi
}
trap _lib_cleanup EXIT

# Initialize temp directory
_lib_init() {
    _LIB_TMPDIR=$(mktemp -d)
}

# Parse common flags (-v/--verbose)
# Usage: parse_flags "$@"
# Sets VERBOSE=true if -v or --verbose is passed
parse_flags() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --verbose|-v)
                VERBOSE=true
                shift
                ;;
            *)
                echo "Unknown option: $1"
                echo "Usage: $0 [--verbose|-v]"
                exit 1
                ;;
        esac
    done
}

# Print a step name with padding (before running)
# Usage: print_step "Step name"
print_step() {
    local name="$1"
    local padded
    padded=$(printf "%-20s" "$name")
    echo -n "  $padded "
}

# Print success marker
print_pass() {
    echo "✓"
}

# Print failure marker
print_fail() {
    echo "✗"
}

# Print fix instruction
# Usage: print_fix "cargo fmt"
print_fix() {
    echo "    Fix: $1"
}

# Print rerun instruction
# Usage: print_rerun "./scripts/check-rust.sh -v"
print_rerun() {
    echo "    Rerun: $1"
}

# Print skip message
# Usage: print_skip "reason"
print_skip() {
    echo "(skipped: $1)"
}

# Print warning (for optional tools)
# Usage: print_warn "cargo-deny not found"
print_warn() {
    echo "⚠️  $1"
}

# Run a check command with output capture
# Usage: run_check "Step name" command [args...]
# Returns: 0 on success, 1 on failure
# On failure, adds step name to FAILED_CHECKS array
run_check() {
    local name="$1"
    shift

    # Ensure temp dir exists
    if [ -z "$_LIB_TMPDIR" ]; then
        _lib_init
    fi

    local outfile="$_LIB_TMPDIR/output.txt"

    print_step "$name"

    if [ "$VERBOSE" = true ]; then
        echo ""
        echo "    Running: $*"
        if "$@" 2>&1 | tee "$outfile"; then
            print_step "$name"
            print_pass
            return 0
        else
            print_step "$name"
            print_fail
            FAILED_CHECKS+=("$name")
            return 1
        fi
    else
        if "$@" > "$outfile" 2>&1; then
            print_pass
            return 0
        else
            print_fail
            FAILED_CHECKS+=("$name")
            # Show captured output on failure
            echo "    Output:"
            sed 's/^/      /' "$outfile" | head -50
            if [ "$(wc -l < "$outfile")" -gt 50 ]; then
                echo "      ... (truncated, run with -v for full output)"
            fi
            return 1
        fi
    fi
}

# Run an optional check (warns but doesn't fail if tool missing)
# Usage: run_optional_check "Step name" "tool_name" command [args...]
# Returns: 0 on success or tool missing, 1 on check failure
run_optional_check() {
    local name="$1"
    local tool="$2"
    shift 2

    if ! command -v "$tool" &> /dev/null; then
        print_step "$name"
        print_skip "$tool not installed"
        return 0
    fi

    run_check "$name" "$@"
}

# Check if a required tool is available
# Usage: require_tool "uv" "curl -LsSf https://astral.sh/uv/install.sh | sh"
# Exits with error if tool not found
require_tool() {
    local tool="$1"
    local install_cmd="$2"

    if ! command -v "$tool" &> /dev/null; then
        echo "❌ $tool not found. Please install it first:"
        echo "   $install_cmd"
        exit 1
    fi
}

# Print summary of failed checks
# Usage: print_summary "Rust" "./scripts/check-rust.sh"
# Returns: 0 if all passed, 1 if any failed
print_summary() {
    local category="$1"
    local script="$2"

    echo ""
    if [ ${#FAILED_CHECKS[@]} -eq 0 ]; then
        echo "✅ All $category checks passed!"
        return 0
    else
        echo "❌ Failed checks: ${FAILED_CHECKS[*]}"
        echo ""
        echo "To debug:"
        echo "  $script -v"
        return 1
    fi
}
