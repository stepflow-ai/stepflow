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

# Copyright {{ year }} {{ authors }}
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

# CI check script for Rust code - mirrors .github/actions/rust-* behavior
# This script runs all Rust-related checks that are performed in CI

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
echo "🦀 Running Rust CI checks..."

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT/stepflow-rs"

# Track results of all checks
FAILED_CHECKS=()

# =============================================================================
# RUST STYLE & QUALITY CHECKS (rust-style action)
# =============================================================================

output "📝 Checking Rust formatting..."
if [ "$QUIET" = true ]; then
    if ! cargo fmt --check >/dev/null 2>&1; then
        echo "❌ Formatting check failed. Run 'cargo fmt' to fix."
        FAILED_CHECKS+=("formatting")
    fi
else
    if ! cargo fmt --check; then
        echo "❌ Formatting check failed. Run 'cargo fmt' to fix."
        FAILED_CHECKS+=("formatting")
    fi
fi

output "🔒 Running dependency security audit..."
if [ "$QUIET" = true ]; then
    if ! cargo deny check >/dev/null 2>&1; then
        echo "⚠️  cargo-deny not found or check failed"
        echo "   Install with: cargo install cargo-deny"
        echo "   Or run: cargo deny check"
        FAILED_CHECKS+=("security-audit")
    fi
else
    if ! cargo deny check 2>/dev/null; then
        echo "⚠️  cargo-deny not found or check failed"
        echo "   Install with: cargo install cargo-deny"
        echo "   Or run: cargo deny check"
        FAILED_CHECKS+=("security-audit")
    fi
fi

output "🧹 Checking for unused dependencies..."
if command -v cargo-machete &> /dev/null; then
    if [ "$QUIET" = true ]; then
        if ! cargo machete --with-metadata >/dev/null 2>&1; then
            echo "❌ Unused dependencies found. Run 'cargo machete --fix --with-metadata' to fix."
            FAILED_CHECKS+=("unused-deps")
        fi
    else
        if ! cargo machete --with-metadata; then
            echo "❌ Unused dependencies found. Run 'cargo machete --fix --with-metadata' to fix."
            FAILED_CHECKS+=("unused-deps")
        fi
    fi
else
    if [ "$QUIET" = false ]; then
        echo "⚠️  cargo-machete not found, skipping unused dependency check"
        echo "   Install with: cargo install cargo-machete"
    fi
fi

# =============================================================================
# RUST BUILD & TEST CHECKS (rust-build action)
# =============================================================================

output "🧪 Running Rust tests..."
if [ "$QUIET" = true ]; then
    if ! cargo test >/dev/null 2>&1; then
        echo "❌ Tests failed"
        FAILED_CHECKS+=("tests")
    fi
else
    if ! cargo test; then
        echo "❌ Tests failed"
        FAILED_CHECKS+=("tests")
    fi
fi

output "📎 Running Rust linting (clippy)..."
if [ "$QUIET" = true ]; then
    if ! cargo clippy -- -D warnings >/dev/null 2>&1; then
        echo "❌ Clippy linting failed"
        FAILED_CHECKS+=("clippy")
    fi
else
    if ! cargo clippy -- -D warnings; then
        echo "❌ Clippy linting failed"
        FAILED_CHECKS+=("clippy")
    fi
fi

# =============================================================================
# ADDITIONAL CHECKS (not in CI but useful for local development)
# =============================================================================

output "🔍 Checking compilation..."
if [ "$QUIET" = true ]; then
    if ! cargo check --all-targets --all-features >/dev/null 2>&1; then
        echo "❌ Compilation check failed"
        FAILED_CHECKS+=("compilation")
    fi
else
    if ! cargo check --all-targets --all-features; then
        echo "❌ Compilation check failed"
        FAILED_CHECKS+=("compilation")
    fi
fi

output "📚 Checking documentation..."
if [ "$QUIET" = true ]; then
    if ! cargo doc --all --no-deps >/dev/null 2>&1; then
        echo "❌ Documentation check failed"
        FAILED_CHECKS+=("documentation")
    fi
else
    if ! cargo doc --all --no-deps; then
        echo "❌ Documentation check failed"
        FAILED_CHECKS+=("documentation")
    fi
fi

# =============================================================================
# RESULTS SUMMARY
# =============================================================================

echo ""
echo "=== Rust CI Check Results ==="

if [ ${#FAILED_CHECKS[@]} -eq 0 ]; then
    echo "✅ All Rust checks passed!"
    exit 0
else
    echo "❌ Failed checks: ${FAILED_CHECKS[*]}"
    echo ""
    echo "To fix these issues:"
    for check in "${FAILED_CHECKS[@]}"; do
        case "$check" in
            "formatting")
                echo "  - Run: cargo fmt"
                ;;
            "security-audit")
                echo "  - Install cargo-deny: cargo install cargo-deny"
                echo "  - Run: cargo deny check"
                ;;
            "unused-deps")
                echo "  - Install cargo-machete: cargo install cargo-machete"
                echo "  - Run: cargo machete --fix --with-metadata"
                ;;
            "tests")
                echo "  - Fix failing tests and run: cargo test"
                ;;
            "clippy")
                echo "  - Fix linting issues and run: cargo clippy"
                ;;
            "compilation")
                echo "  - Fix compilation errors and run: cargo check"
                ;;
            "documentation")
                echo "  - Fix documentation errors and run: cargo doc"
                ;;
        esac
    done
    exit 1
fi