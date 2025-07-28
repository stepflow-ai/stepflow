#!/bin/bash

# Run individual standalone workflow load tests (no OpenAI API calls)
# This script should be run from the load-tests directory

set -e

# Default values
STEPFLOW_URL="http://localhost:7837"
RESULTS_DIR="results"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --url)
            STEPFLOW_URL="$2"
            shift 2
            ;;
        --results-dir)
            RESULTS_DIR="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 [options]"
            echo "Options:"
            echo "  --url URL           Stepflow service URL (default: http://localhost:7837)"
            echo "  --results-dir DIR   Results directory (default: results)"
            echo "  -h, --help         Show this help message"
            echo ""
            echo "This script runs standalone load tests that don't require OpenAI API calls."
            echo "Tests basic message creation/manipulation functionality across three workflow types:"
            echo "  1. rust-builtin-messages - Direct Rust built-in component"
            echo "  2. python-custom-messages - Python custom component server"
            echo "  3. python-udf-messages - Python UDF with blob storage"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Check if k6 is installed
if ! command -v k6 &> /dev/null; then
    echo "Error: k6 is not installed"
    echo "Please install k6: https://k6.io/docs/getting-started/installation/"
    exit 1
fi

# Create results directory
mkdir -p "$RESULTS_DIR"
RESULTS_DIR="$PWD/$RESULTS_DIR"

# Check if Stepflow service is running
echo "Checking Stepflow service at $STEPFLOW_URL..."
if ! curl -sf "$STEPFLOW_URL/api/v1/health" > /dev/null; then
    echo "Error: Stepflow service is not responding at $STEPFLOW_URL"
    echo "Please start the service first."
    echo "Use: ./scripts/start-standalone-service.sh"
    exit 1
fi

echo "Stepflow service is healthy, starting individual standalone workflow load tests..."

# Navigate to the scripts directory to access test files
cd "$(dirname "$0")"

# Array of workflow types to test
workflows=("rust-builtin-messages" "python-custom-messages" "python-udf-messages")

# Run load test for each workflow type
for workflow in "${workflows[@]}"; do
    echo ""
    echo "=========================================="
    echo "Testing workflow: $workflow"
    echo "=========================================="
    
    # Run k6 load test for this specific workflow
    k6 run \
        --env STEPFLOW_URL="$STEPFLOW_URL" \
        --env WORKFLOW_TYPE="$workflow" \
        --out json="$RESULTS_DIR/${workflow}_${TIMESTAMP}.json" \
        --summary-export="$RESULTS_DIR/${workflow}_summary_${TIMESTAMP}.json" \
        single-standalone-test.js
    
    echo ""
    echo "✓ Completed test for $workflow"
    echo "  Results: $RESULTS_DIR/${workflow}_${TIMESTAMP}.json"
    echo "  Summary: $RESULTS_DIR/${workflow}_summary_${TIMESTAMP}.json"
    
    # Brief pause between workflow tests
    echo "Waiting 10 seconds before next test..."
    sleep 10
done

echo ""
echo "=========================================="
echo "All standalone workflow load tests completed!"
echo "=========================================="
echo ""
echo "Results saved to $RESULTS_DIR/ with timestamp $TIMESTAMP"
echo ""
echo "To analyze and compare results:"
echo "  1. Check summary files for key metrics per workflow"
echo "  2. Look for threshold violations indicating load limits"
echo "  3. Compare p95 response times across workflow types"
echo "  4. Identify failure rates and performance bottlenecks"
echo ""
echo "Expected performance order (fastest to slowest):"
echo "  1. rust-builtin-messages    (direct Rust integration)"
echo "  2. python-custom-messages   (dedicated Python server)"
echo "  3. python-udf-messages      (blob creation + Python execution)"
echo ""
echo "These standalone tests should be much faster than OpenAI tests since there are no external API calls."