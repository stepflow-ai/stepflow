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

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
K8S_DIR="$PROJECT_DIR/k8s"

echo "🚀 Deploying Stepflow Kubernetes Batch Demo"
echo ""

# Check if kubectl is configured
if ! kubectl cluster-info &> /dev/null; then
    echo "❌ kubectl not configured. Run ./scripts/setup-kubectl.sh first"
    exit 1
fi

echo "✅ kubectl configured"
echo ""

# Step 1: Create namespace
echo "📦 Creating namespace..."
kubectl apply -f "$K8S_DIR/namespace.yaml"
echo ""

# Step 2: Deploy component servers
echo "🐍 Deploying component servers..."
kubectl apply -k "$K8S_DIR/component-server/"
echo ""

# Step 3: Wait for component servers to be ready
echo "⏳ Waiting for component server pods to be ready (timeout: 60s)..."
if kubectl wait --for=condition=Ready pods -l app=component-server -n stepflow-demo --timeout=60s; then
    echo "✅ Component servers ready"
else
    echo "❌ Component servers failed to become ready"
    echo ""
    echo "Pod status:"
    kubectl get pods -n stepflow-demo -l app=component-server
    echo ""
    echo "Pod logs:"
    kubectl logs -n stepflow-demo -l app=component-server --tail=50
    exit 1
fi
echo ""

# Step 4: Show component server status
echo "📊 Component server status:"
kubectl get pods -n stepflow-demo -l app=component-server -o wide
echo ""

# Step 5: Deploy Pingora load balancer
echo "🔀 Deploying Pingora load balancer..."
kubectl apply -k "$K8S_DIR/pingora-lb/"
echo ""

# Step 6: Wait for Pingora to be ready
echo "⏳ Waiting for Pingora pods to be ready (timeout: 60s)..."
if kubectl wait --for=condition=Ready pods -l app=pingora-lb -n stepflow-demo --timeout=60s; then
    echo "✅ Pingora load balancer ready"
else
    echo "❌ Pingora failed to become ready"
    echo ""
    echo "Pod status:"
    kubectl get pods -n stepflow-demo -l app=pingora-lb
    echo ""
    echo "Pod logs:"
    kubectl logs -n stepflow-demo -l app=pingora-lb --tail=50
    exit 1
fi
echo ""

# Step 7: Show Pingora status
echo "📊 Pingora load balancer status:"
kubectl get pods -n stepflow-demo -l app=pingora-lb -o wide
echo ""

# Step 8: Show all services
echo "🌐 Services:"
kubectl get svc -n stepflow-demo
echo ""

# Step 9: Wait a moment for backend discovery
echo "⏳ Waiting 15 seconds for backend discovery..."
sleep 15
echo ""

# Step 10: Show Pingora logs to verify backend discovery
echo "📋 Pingora backend discovery logs:"
kubectl logs -n stepflow-demo -l app=pingora-lb --tail=30 | grep -E "(INFO|Backend|healthy|Discovered)" || true
echo ""

echo "✅ Deployment complete!"
echo ""
echo "Next steps:"
echo "  1. Port-forward to test: kubectl port-forward -n stepflow-demo service/pingora-lb 8080:8080"
echo "  2. Test health endpoint: curl http://localhost:8080/health"
echo "  3. Run workflows from workflows/ directory"
echo ""
echo "To view logs:"
echo "  Component servers: kubectl logs -n stepflow-demo -l app=component-server -f"
echo "  Pingora LB:        kubectl logs -n stepflow-demo -l app=pingora-lb -f"
