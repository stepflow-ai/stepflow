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


# Run k6 load test against Stepflow service
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
    echo "Please start the service first using ./start-service.sh"
    exit 1
fi

echo "Stepflow service is healthy, starting load test..."

# Navigate to the scripts directory to access test files
cd "$(dirname "$0")"

# Run k6 load test with custom output
k6 run \
    --env STEPFLOW_URL="$STEPFLOW_URL" \
    --out json="$RESULTS_DIR/results_${TIMESTAMP}.json" \
    --summary-export="$RESULTS_DIR/summary_${TIMESTAMP}.json" \
    load-test.js

echo ""
echo "Load test completed!"
echo "Results saved to:"
echo "  - Raw data: $RESULTS_DIR/results_${TIMESTAMP}.json"
echo "  - Summary: $RESULTS_DIR/summary_${TIMESTAMP}.json"
echo ""
echo "To analyze results, you can use k6's built-in tools or import the JSON files into your preferred analysis tool."