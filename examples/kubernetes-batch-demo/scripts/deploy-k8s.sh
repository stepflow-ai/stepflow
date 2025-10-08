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

# Step 5: Deploy load balancer
echo "🔀 Deploying Stepflow load balancer..."
kubectl apply -k "$K8S_DIR/load-balancer/"
echo ""

# Step 6: Wait for load balancer to be ready
echo "⏳ Waiting for load balancer pods to be ready (timeout: 60s)..."
if kubectl wait --for=condition=Ready pods -l app=stepflow-load-balancer -n stepflow-demo --timeout=60s; then
    echo "✅ Stepflow load balancer ready"
else
    echo "❌ Load balancer failed to become ready"
    echo ""
    echo "Pod status:"
    kubectl get pods -n stepflow-demo -l app=stepflow-load-balancer
    echo ""
    echo "Pod logs:"
    kubectl logs -n stepflow-demo -l app=stepflow-load-balancer --tail=50
    exit 1
fi
echo ""

# Step 7: Show load balancer status
echo "📊 Load balancer status:"
kubectl get pods -n stepflow-demo -l app=stepflow-load-balancer -o wide
echo ""

# Step 8: Show all services
echo "🌐 Services:"
kubectl get svc -n stepflow-demo
echo ""

# Step 9: Wait a moment for backend discovery
echo "⏳ Waiting 15 seconds for backend discovery..."
sleep 15
echo ""

# Step 10: Show load balancer logs to verify backend discovery
echo "📋 Load balancer backend discovery logs:"
kubectl logs -n stepflow-demo -l app=stepflow-load-balancer --tail=30 | grep -E "(INFO|Backend|healthy|Discovered)" || true
echo ""

echo "✅ Deployment complete!"
echo ""
echo "Next steps:"
echo "  1. Port-forward to test: kubectl port-forward -n stepflow-demo service/stepflow-load-balancer 8080:8080"
echo "  2. Test health endpoint: curl http://localhost:8080/health"
echo "  3. Run workflows from workflows/ directory"
echo ""
echo "To view logs:"
echo "  Component servers: kubectl logs -n stepflow-demo -l app=component-server -f"
echo "  Load Balancer:        kubectl logs -n stepflow-demo -l app=stepflow-load-balancer -f"
