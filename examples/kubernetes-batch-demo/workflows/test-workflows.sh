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

# Unified test script using submit command to k8s Stepflow server
# Tests simple, bidirectional, and parallel workflows

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

print_status() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Setup kubectl
export KUBECONFIG="$PROJECT_DIR/kubeconfig"

# Stepflow server URL (via port-forward or service)
STEPFLOW_URL=${STEPFLOW_URL:-"http://localhost:7840/api/v1"}

echo ""
print_status "🚀 Testing workflows via Stepflow server..."
print_info "Server URL: $STEPFLOW_URL"
echo ""

# Check if Stepflow server is ready
print_info "Checking Stepflow server health..."
if ! curl -s -f "$STEPFLOW_URL/health" > /dev/null 2>&1; then
    print_error "Cannot reach Stepflow server at $STEPFLOW_URL/health"
    echo ""
    echo "Please ensure:"
    echo "  1. Stepflow server pod is running:"
    echo "     kubectl get pods -n stepflow-demo -l app=stepflow-server"
    echo ""
    echo "  2. Port-forward is active (in another terminal):"
    echo "     cd $PROJECT_DIR"
    echo "     ./scripts/start-stepflow-port-forward.sh"
    echo ""
    echo "Or set STEPFLOW_URL environment variable to point to your server"
    exit 1
fi

print_info "✓ Stepflow server is healthy and ready"
echo ""

# Function to submit workflow
submit_workflow() {
    local workflow_file=$1
    local input_json=$2
    local description=$3

    print_info "Submitting: $description"

    local output
    local exit_code
    output=$(cargo run --quiet --manifest-path "$PROJECT_DIR/../../stepflow-rs/Cargo.toml" -- submit \
        --url "$STEPFLOW_URL" \
        --flow "$SCRIPT_DIR/$workflow_file" \
        --input-json "$input_json" 2>&1)
    exit_code=$?

    if [ $exit_code -eq 0 ]; then
        echo "  ✅ Success"
        echo "$output" | grep -E "(execution_id|output)" | head -5 | sed 's/^/    /'
    else
        print_error "  ❌ Failed (exit code: $exit_code)"
        echo ""
        echo "Output:"
        echo "$output" | sed 's/^/  /'
        echo ""
        return 1
    fi

    echo ""
    sleep 0.5
}

# Test 1: Simple workflows
print_status "📝 Test 1: Simple workflows (10 sequential submissions)"
echo ""

for i in {1..10}; do
    submit_workflow \
        "simple-test-workflow.yaml" \
        "{\"m\": $i, \"n\": $((i+1))}" \
        "Simple workflow $i/10"
done

# Test 2: Bidirectional workflows
print_status "📝 Test 2: Bidirectional workflows (5 sequential submissions)"
echo ""

TIMESTAMP=$(date +%s)
for i in {1..5}; do
    submit_workflow \
        "bidirectional-test-workflow.yaml" \
        "{\"data\": {\"test\": \"value$i\", \"number\": $i, \"timestamp\": \"$TIMESTAMP\"}}" \
        "Bidirectional workflow $i/5"
done

# Test 3: Parallel bidirectional workflows
print_status "📝 Test 3: Parallel bidirectional workflows (5 workflows, 3 parallel components each)"
echo ""

for i in {1..5}; do
    submit_workflow \
        "parallel-bidirectional-test.yaml" \
        "{\"workflow_id\": $i}" \
        "Parallel workflow $i/5 (3 parallel components)"
done

# Test 4: Batch execution
print_status "📝 Test 4: Batch execution (20 workflows, 5 concurrent)"
echo ""

# Generate batch inputs as JSONL (using 'value' field for simple-test-workflow)
BATCH_INPUTS_FILE="/tmp/stepflow-batch-inputs-$$.jsonl"
print_info "Generating batch inputs..."
> "$BATCH_INPUTS_FILE"  # Clear file first
for i in {1..20}; do
    echo "{\"value\": $i}" >> "$BATCH_INPUTS_FILE"
done
print_info "Generated 20 inputs in $BATCH_INPUTS_FILE"

# Submit batch
print_info "Submitting batch execution..."
output=$(cargo run --quiet --manifest-path "$PROJECT_DIR/../../stepflow-rs/Cargo.toml" -- submit-batch \
    --url "$STEPFLOW_URL" \
    --flow "$SCRIPT_DIR/simple-test-workflow.yaml" \
    --inputs "$BATCH_INPUTS_FILE" \
    --max-concurrent 5 2>&1)
exit_code=$?

if [ $exit_code -eq 0 ]; then
    echo "  ✅ Batch execution completed successfully"
    # Extract and display summary stats (last 15 lines contain statistics)
    echo "$output" | tail -15 | sed 's/^/    /'
else
    print_error "  ❌ Batch execution had failures (exit code: $exit_code)"
    echo ""
    echo "Last 30 lines of output:"
    echo "$output" | tail -30 | sed 's/^/  /'
    echo ""
    print_info "Note: Some failures are expected if component servers are under load"
fi

# Cleanup temp file
rm -f "$BATCH_INPUTS_FILE"

echo ""

# Give workflows time to complete
print_info "Waiting for all workflows to complete..."
sleep 5

echo ""
print_status "📊 Analyzing execution distribution..."
echo ""

# Component server instances
print_info "Component server instances:"
kubectl get pods -n stepflow-demo -l app=component-server -o wide 2>/dev/null | tail -n +2 | awk '{print "  " $1 " (IP: " $6 ")"}'

echo ""

# Component execution distribution
print_info "Component execution distribution (last 100 executions):"
kubectl logs -n stepflow-demo -l app=component-server --tail=500 --prefix 2>/dev/null | \
    grep -E "(double|store_and_retrieve_blob)" | \
    grep -o "^\\[pod/[^]]*\\]" | \
    sed 's/\\[pod\\/\\(.*\\)\\]/\\1/' | \
    sort | uniq -c | \
    awk '{print "  " $2 ": " $1 " executions"}'

echo ""

# Pingora routing decisions
print_info "Pingora routing decisions (last 50 requests):"
kubectl logs -n stepflow-demo -l app=pingora-lb --tail=1000 --since=60s 2>/dev/null | \
    grep "Routing POST /" | \
    tail -50 | \
    awk '{print $NF}' | \
    sort | uniq -c | \
    awk '{print "  " $2 ": " $1 " requests"}'

echo ""
print_status "✅ Workflow tests complete!"
echo ""
print_info "Expected behavior:"
print_info "  • Simple workflows: Load distributed across component servers"
print_info "  • Bidirectional workflows: Instance affinity maintained for blob operations"
print_info "  • Parallel workflows: Concurrent component execution distributed across instances"
print_info "  • Batch execution: 20 workflows executed with max 5 concurrent, distributed load"
echo ""
