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

# Comprehensive CI check script - mirrors complete CI pipeline behavior
# This script runs all checks that are performed in CI, organized by component

set -e

# Parse command line arguments
VERBOSE=false
while [[ $# -gt 0 ]]; do
    case $1 in
        --verbose|-v)
            VERBOSE=true
            shift
            ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: $0 [--verbose|-v]"
            echo "  --verbose, -v: Show detailed output from individual checks"
            exit 1
            ;;
    esac
done

echo "🚀 Running complete CI check suite..."

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

# Track results of all check categories
FAILED_CATEGORIES=()

# =============================================================================
# RUST CHECKS
# =============================================================================

echo ""
echo "═══════════════════════════════════════════════════════════════════════════════"
echo "🦀 RUST CHECKS"
echo "═══════════════════════════════════════════════════════════════════════════════"

if [ "$VERBOSE" = true ]; then
    if ! "$SCRIPT_DIR/check-rust.sh"; then
        echo "❌ Rust checks failed"
        FAILED_CATEGORIES+=("rust")
    else
        echo "✅ Rust checks passed"
    fi
else
    if ! "$SCRIPT_DIR/check-rust.sh" --quiet; then
        echo "❌ Rust checks failed"
        FAILED_CATEGORIES+=("rust")
    else
        echo "✅ Rust checks passed"
    fi
fi

# =============================================================================
# PYTHON CHECKS
# =============================================================================

echo ""
echo "═══════════════════════════════════════════════════════════════════════════════"
echo "🐍 PYTHON CHECKS"
echo "═══════════════════════════════════════════════════════════════════════════════"

if [ "$VERBOSE" = true ]; then
    if ! "$SCRIPT_DIR/check-python.sh"; then
        echo "❌ Python checks failed"
        FAILED_CATEGORIES+=("python")
    else
        echo "✅ Python checks passed"
    fi
else
    if ! "$SCRIPT_DIR/check-python.sh" --quiet; then
        echo "❌ Python checks failed"
        FAILED_CATEGORIES+=("python")
    else
        echo "✅ Python checks passed"
    fi
fi

# =============================================================================
# DOCUMENTATION CHECKS
# =============================================================================

echo ""
echo "═══════════════════════════════════════════════════════════════════════════════"
echo "📚 DOCUMENTATION CHECKS"
echo "═══════════════════════════════════════════════════════════════════════════════"

if [ "$VERBOSE" = true ]; then
    if ! "$SCRIPT_DIR/check-docs.sh"; then
        echo "❌ Documentation checks failed"
        FAILED_CATEGORIES+=("docs")
    else
        echo "✅ Documentation checks passed"
    fi
else
    if ! "$SCRIPT_DIR/check-docs.sh" --quiet; then
        echo "❌ Documentation checks failed"
        FAILED_CATEGORIES+=("docs")
    else
        echo "✅ Documentation checks passed"
    fi
fi

# =============================================================================
# LICENSE CHECKS
# =============================================================================

echo ""
echo "═══════════════════════════════════════════════════════════════════════════════"
echo "🔒 LICENSE CHECKS"
echo "═══════════════════════════════════════════════════════════════════════════════"

if [ "$VERBOSE" = true ]; then
    if ! "$SCRIPT_DIR/check-licenses.sh"; then
        echo "❌ License checks failed"
        FAILED_CATEGORIES+=("licenses")
    else
        echo "✅ License checks passed"
    fi
else
    if ! "$SCRIPT_DIR/check-licenses.sh" --quiet; then
        echo "❌ License checks failed"
        FAILED_CATEGORIES+=("licenses")
    else
        echo "✅ License checks passed"
    fi
fi

# =============================================================================
# INTEGRATION TESTS
# =============================================================================

echo ""
echo "═══════════════════════════════════════════════════════════════════════════════"
echo "🔗 INTEGRATION TESTS"
echo "═══════════════════════════════════════════════════════════════════════════════"

if [ "$VERBOSE" = true ]; then
    if ! "$SCRIPT_DIR/test-integration.sh"; then
        echo "❌ Integration tests failed"
        FAILED_CATEGORIES+=("integration")
    else
        echo "✅ Integration tests passed"
    fi
else
    # Note: test-integration.sh may not support --quiet flag yet
    if ! "$SCRIPT_DIR/test-integration.sh" >/dev/null 2>&1; then
        echo "❌ Integration tests failed"
        FAILED_CATEGORIES+=("integration")
    else
        echo "✅ Integration tests passed"
    fi
fi

# =============================================================================
# OVERALL RESULTS SUMMARY
# =============================================================================

echo ""
echo "═══════════════════════════════════════════════════════════════════════════════"
echo "📊 OVERALL CI CHECK RESULTS"
echo "═══════════════════════════════════════════════════════════════════════════════"

if [ ${#FAILED_CATEGORIES[@]} -eq 0 ]; then
    echo "🎉 All CI checks passed!"
    echo ""
    echo "✅ Rust checks: PASSED"
    echo "✅ Python checks: PASSED"  
    echo "✅ Documentation checks: PASSED"
    echo "✅ License checks: PASSED"
    echo "✅ Integration tests: PASSED"
    echo ""
    echo "🚀 Ready for CI and deployment!"
    exit 0
else
    echo "❌ Some CI checks failed!"
    echo ""
    echo "Failed categories: ${FAILED_CATEGORIES[*]}"
    echo ""
    echo "Status summary:"
    if [[ " ${FAILED_CATEGORIES[*]} " =~ " rust " ]]; then
        echo "❌ Rust checks: FAILED"
    else
        echo "✅ Rust checks: PASSED"
    fi
    if [[ " ${FAILED_CATEGORIES[*]} " =~ " python " ]]; then
        echo "❌ Python checks: FAILED"
    else
        echo "✅ Python checks: PASSED"
    fi
    if [[ " ${FAILED_CATEGORIES[*]} " =~ " docs " ]]; then
        echo "❌ Documentation checks: FAILED"
    else
        echo "✅ Documentation checks: PASSED"
    fi
    if [[ " ${FAILED_CATEGORIES[*]} " =~ " licenses " ]]; then
        echo "❌ License checks: FAILED"
    else
        echo "✅ License checks: PASSED"
    fi
    if [[ " ${FAILED_CATEGORIES[*]} " =~ " integration " ]]; then
        echo "❌ Integration tests: FAILED"
    else
        echo "✅ Integration tests: PASSED"
    fi
    echo ""
    echo "🔧 Run individual check scripts to see detailed error information:"
    for category in "${FAILED_CATEGORIES[@]}"; do
        case "$category" in
            "rust")
                echo "  - ./scripts/check-rust.sh"
                ;;
            "python")
                echo "  - ./scripts/check-python.sh"
                ;;
            "docs")
                echo "  - ./scripts/check-docs.sh"
                ;;
            "licenses")
                echo "  - ./scripts/check-licenses.sh"
                ;;
            "integration")
                echo "  - ./scripts/test-integration.sh"
                ;;
        esac
    done
    exit 1
fi