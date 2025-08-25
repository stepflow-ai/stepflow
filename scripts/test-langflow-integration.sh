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

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Emojis for status
CHECK_MARK="✅"
CROSS_MARK="❌"
ROCKET="🚀"
WRENCH="🔧"
TEST_TUBE="🧪"
TARGET="🎯"
CELEBRATION="🎉"

echo -e "${ROCKET} Stepflow Langflow Integration Test Runner"
echo "=================================================="

# Get the root directory (assuming script is in scripts/)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
LANGFLOW_DIR="$ROOT_DIR/integrations/langflow"
STEPFLOW_RS_DIR="$ROOT_DIR/stepflow-rs"

# Function to print status messages
print_status() {
    local emoji="$1"
    local message="$2"
    echo -e "${emoji} ${message}"
}

# Function to print error and exit
die() {
    print_status "$CROSS_MARK" "$1"
    exit 1
}

# Function to check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Function to run command with status reporting
run_with_status() {
    local description="$1"
    shift

    echo -e "\n${WRENCH} ${description}..."

    if "$@"; then
        print_status "$CHECK_MARK" "$description completed"
        return 0
    else
        print_status "$CROSS_MARK" "$description failed"
        return 1
    fi
}

# Check required tools
check_prerequisites() {
    print_status "$WRENCH" "Checking prerequisites..."

    command_exists "cargo" || die "cargo not found. Please install Rust."
    command_exists "uv" || die "uv not found. Please install uv package manager."

    # Check if we're in the right directory
    [ -d "$LANGFLOW_DIR" ] || die "Langflow integration directory not found at $LANGFLOW_DIR"
    [ -d "$STEPFLOW_RS_DIR" ] || die "Stepflow Rust directory not found at $STEPFLOW_RS_DIR"

    print_status "$CHECK_MARK" "Prerequisites check passed"
}

# Build Stepflow binary if needed
ensure_stepflow_binary() {
    local binary_path="$STEPFLOW_RS_DIR/target/debug/stepflow"

    if [ -n "$STEPFLOW_BINARY_PATH" ] && [ -f "$STEPFLOW_BINARY_PATH" ]; then
        print_status "$CHECK_MARK" "Using Stepflow binary from STEPFLOW_BINARY_PATH: $STEPFLOW_BINARY_PATH"
        export STEPFLOW_BINARY_PATH
        return 0
    fi

    if [ ! -f "$binary_path" ]; then
        run_with_status "Building Stepflow binary" \
            cargo build --manifest-path "$STEPFLOW_RS_DIR/Cargo.toml"
    else
        print_status "$CHECK_MARK" "Stepflow binary already exists"
    fi

    export STEPFLOW_BINARY_PATH="$binary_path"
    print_status "$CHECK_MARK" "Using Stepflow binary: $STEPFLOW_BINARY_PATH"
}

# Test mock server functionality
test_mock_server() {
    print_status "$WRENCH" "Testing mock Langflow server..."

    cd "$LANGFLOW_DIR"

    # Test that the mock server responds correctly
    local test_request='{"jsonrpc":"2.0","id":"test","method":"initialize","params":{}}'
    local response

    response=$(echo "$test_request" | timeout 5 uv run python tests/mock_langflow_server.py 2>/dev/null) || {
        die "Mock server failed to respond to test request"
    }

    # Check if response contains expected fields
    if echo "$response" | grep -q '"status": "initialized"' && echo "$response" | grep -q '"server_protocol_version": 1'; then
        print_status "$CHECK_MARK" "Mock server working correctly"
    else
        die "Mock server returned unexpected response: $response"
    fi
}

# Run basic validation test
run_validation_test() {
    print_status "$WRENCH" "Running basic validation test..."

    cd "$LANGFLOW_DIR"

    # Test basic conversion and configuration
    local test_result
    test_result=$(uv run python -c "
import sys
sys.path.append('src')
from stepflow_langflow_integration.converter.translator import LangflowConverter
from stepflow_langflow_integration.testing.stepflow_binary import get_default_stepflow_config

# Test basic conversion
simple_workflow = {
    'data': {
        'nodes': [
            {
                'data': {'type': 'ChatInput', 'node': {'template': {}}},
                'id': 'test-input',
                'type': 'genericNode'
            }
        ],
        'edges': []
    }
}

converter = LangflowConverter()
workflow = converter.convert(simple_workflow)
print(f'STEPS:{len(workflow.steps)}')

# Test configuration
config = get_default_stepflow_config()
if '/langflow/' in config and 'mock_langflow' in config:
    print('CONFIG:OK')
else:
    print('CONFIG:BAD')
" 2>/dev/null) || die "Basic validation test failed"

    if echo "$test_result" | grep -q "STEPS:1" && echo "$test_result" | grep -q "CONFIG:OK"; then
        print_status "$CHECK_MARK" "Basic validation test passed"
    else
        die "Basic validation test failed with result: $test_result"
    fi
}

# Run unit tests
run_unit_tests() {
    print_status "$TEST_TUBE" "Running unit tests..."

    cd "$LANGFLOW_DIR"

    if uv run python -m pytest tests/unit/ -v -x; then
        print_status "$CHECK_MARK" "Unit tests passed"
    else
        die "Unit tests failed"
    fi
}

# Run new conversion tests
run_conversion_tests() {
    print_status "$TEST_TUBE" "Running conversion tests..."

    cd "$LANGFLOW_DIR"

    if uv run python -m pytest tests/integration/test_conversion.py -v -x; then
        print_status "$CHECK_MARK" "Conversion tests passed"
    else
        print_status "$CROSS_MARK" "Some conversion tests failed"
        return 1
    fi
}

# Run new validation tests
run_validation_tests() {
    print_status "$TEST_TUBE" "Running validation tests..."

    cd "$LANGFLOW_DIR"

    if uv run python -m pytest tests/integration/test_validation.py -v -x -m "not slow"; then
        print_status "$CHECK_MARK" "Validation tests passed"
    else
        print_status "$CROSS_MARK" "Some validation tests failed"
        return 1
    fi
}

# Run execution tests (with mocking, non-slow)
run_execution_tests() {
    print_status "$TEST_TUBE" "Running execution tests..."

    cd "$LANGFLOW_DIR"

    if uv run python -m pytest tests/integration/test_execution.py -v -m "not slow" -x; then
        print_status "$CHECK_MARK" "Execution tests passed"
    else
        print_status "$CROSS_MARK" "Some execution tests failed"
        return 1
    fi
}

# Run slow tests (execution and validation)
run_slow_tests() {
    print_status "$TEST_TUBE" "Running slow tests..."

    cd "$LANGFLOW_DIR"

    # Run slow execution tests
    local exit_code=0
    if ! uv run python -m pytest tests/integration/test_execution.py -v -m "slow" -x; then
        print_status "$CROSS_MARK" "Some slow execution tests failed"
        exit_code=1
    fi

    # Run slow validation tests
    if ! uv run python -m pytest tests/integration/test_validation.py -v -m "slow" -x; then
        print_status "$CROSS_MARK" "Some slow validation tests failed"
        exit_code=1
    fi

    if [ $exit_code -eq 0 ]; then
        print_status "$CHECK_MARK" "Slow tests passed"
    fi

    return $exit_code
}

# Test key execution scenarios (previously failing workflows)
test_failing_scenarios() {
    print_status "$TARGET" "Testing key execution scenarios..."

    cd "$LANGFLOW_DIR"

    local key_scenarios=(
        "basic_prompting"
        "memory_chatbot"
        "document_qa"
        "simple_agent"
    )

    local failed_count=0
    local total_count=${#key_scenarios[@]}

    for scenario in "${key_scenarios[@]}"; do
        echo -e "\n--- Testing execution scenario: $scenario ---"

        # Run both validation and execution for this scenario
        if timeout 60 uv run python -m pytest "tests/integration/test_execution.py::TestWorkflowExecution::test_workflow_execution_with_mocking[$scenario]" -v --tb=short; then
            print_status "$CHECK_MARK" "$scenario execution - PASSED"
        else
            print_status "$CROSS_MARK" "$scenario execution - FAILED"
            ((failed_count++))
        fi
    done

    if [ $failed_count -eq 0 ]; then
        print_status "$CHECK_MARK" "All key execution scenarios now pass!"
        return 0
    else
        print_status "$YELLOW" "$failed_count/$total_count key scenarios still fail"
        return 1
    fi
}

# Run all tests with summary
run_full_test_suite() {
    print_status "$TEST_TUBE" "Running full test suite..."

    cd "$LANGFLOW_DIR"

    local exit_code=0

    # Run unit tests
    if ! run_unit_tests; then
        exit_code=1
    fi

    # Run new conversion tests
    if ! run_conversion_tests; then
        exit_code=1
    fi

    # Run new validation tests
    if ! run_validation_tests; then
        exit_code=1
    fi

    # Run new execution tests
    if ! run_execution_tests; then
        exit_code=1
    fi

    # Run slow tests
    if ! run_slow_tests; then
        exit_code=1
    fi

    return $exit_code
}

# Main execution
main() {
    local skip_slow=false
    local test_only_failing=false

    # Parse command line arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --skip-slow)
                skip_slow=true
                shift
                ;;
            --failing-only)
                test_only_failing=true
                shift
                ;;
            --help|-h)
                echo "Usage: $0 [--skip-slow] [--failing-only] [--help]"
                echo ""
                echo "Options:"
                echo "  --skip-slow      Skip slow integration tests"
                echo "  --failing-only   Only test previously failing scenarios"
                echo "  --help           Show this help message"
                exit 0
                ;;
            *)
                echo "Unknown option: $1"
                echo "Use --help for usage information"
                exit 1
                ;;
        esac
    done

    # Step 1: Prerequisites
    check_prerequisites

    # Step 2: Build binary
    ensure_stepflow_binary

    # Step 3: Basic validation
    run_validation_test

    # Step 4: Mock server test
    test_mock_server

    if [ "$test_only_failing" = true ]; then
        # Only test previously failing scenarios
        if test_failing_scenarios; then
            print_status "$CELEBRATION" "All previously failing tests now pass!"
            exit 0
        else
            print_status "$CROSS_MARK" "Some previously failing tests still fail"
            exit 1
        fi
    fi

    # Step 5: Run tests
    local test_exit_code=0

    if [ "$skip_slow" = false ]; then
        if ! run_full_test_suite; then
            test_exit_code=1
        fi
    else
        # Run without slow tests
        if ! run_unit_tests || ! run_conversion_tests || ! run_validation_tests || ! run_execution_tests; then
            test_exit_code=1
        fi
    fi

    # Step 6: Test failing scenarios
    if ! test_failing_scenarios; then
        test_exit_code=1
    fi

    # Final summary
    echo ""
    echo "=================================================="
    if [ $test_exit_code -eq 0 ]; then
        print_status "$CELEBRATION" "All tests passed successfully!"
    else
        print_status "$CROSS_MARK" "Some tests failed"
    fi
    echo "=================================================="

    exit $test_exit_code
}

# Run main function with all arguments
main "$@"