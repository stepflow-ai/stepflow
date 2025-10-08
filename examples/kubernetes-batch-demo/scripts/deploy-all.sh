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

# Complete deployment script for Kubernetes Batch Demo
# Builds all images and deploys all services to k8s

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

print_status() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

# Helper function to wait for pods
wait_for_pods() {
    local label=$1
    local description=$2
    local timeout=${3:-120}

    print_info "Waiting for $description..."
    if kubectl wait --for=condition=Ready pods -l "$label" -n stepflow-demo --timeout="${timeout}s" 2>&1; then
        echo ""
    else
        echo "❌ Error: Timeout waiting for $description"
        return 1
    fi
}

echo ""
print_status "🚀 Deploying Kubernetes Batch Demo..."
echo ""

# Setup kubectl
export KUBECONFIG="$PROJECT_DIR/kubeconfig"

# Step 1: Build images
print_status "Step 1/4: Building Docker images..."
echo ""

print_info "Building component server..."
if bash "$SCRIPT_DIR/build-component-server.sh"; then
    print_info "✅ Component server build complete"
else
    echo "❌ Error: Component server build failed"
    exit 1
fi
echo ""

print_info "Building Stepflow load balancer..."
if bash "$SCRIPT_DIR/build-load-balancer.sh"; then
    print_info "✅ Load balancer build complete"
else
    echo "❌ Error: Load balancer build failed"
    exit 1
fi
echo ""

print_info "Building Stepflow runtime server..."
if bash "$SCRIPT_DIR/build-stepflow-server.sh"; then
    print_info "✅ Stepflow server build complete"
else
    echo "❌ Error: Stepflow server build failed"
    exit 1
fi
echo ""

print_status "All Docker images built successfully!"
echo ""

# Step 2: Create namespace
print_status "Step 2/4: Creating namespace..."
kubectl apply -f "$PROJECT_DIR/k8s/namespace.yaml"
echo ""

# Step 3: Deploy services
print_status "Step 3/4: Deploying services..."
echo ""

print_info "Deploying component servers..."
kubectl apply -k "$PROJECT_DIR/k8s/component-server/"
echo ""

print_info "Deploying Stepflow load balancer..."
kubectl apply -k "$PROJECT_DIR/k8s/stepflow-load-balancer/"
echo ""

print_info "Deploying Stepflow runtime server..."
kubectl apply -k "$PROJECT_DIR/k8s/stepflow-server/"
echo ""

# Step 4: Wait for readiness
print_status "Step 4/4: Waiting for pods to be ready..."
echo ""

wait_for_pods "app=component-server" "component servers" || exit 1
wait_for_pods "app=stepflow-load-balancer" "Stepflow load balancer" || exit 1
wait_for_pods "app=stepflow-server" "Stepflow runtime server" || exit 1

# Show deployment status
print_status "📊 Deployment Status"
echo ""

kubectl get pods -n stepflow-demo
echo ""

kubectl get services -n stepflow-demo
echo ""

print_status "✅ Deployment complete!"
echo ""
print_info "Next steps:"
print_info "  1. Start port-forward to Stepflow server (in separate terminal):"
print_info "     ./scripts/start-port-forward.sh stepflow"
print_info ""
print_info "  2. Run workflow tests:"
print_info "     cd workflows && ./test-workflows.sh"
echo ""
