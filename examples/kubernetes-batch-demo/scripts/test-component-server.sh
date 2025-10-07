#!/bin/bash
set -e

# Test component server deployment in Kubernetes

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
KUBECONFIG_FILE="$SCRIPT_DIR/../kubeconfig"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

print_status() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

# Export KUBECONFIG
export KUBECONFIG="$KUBECONFIG_FILE"

if [ ! -f "$KUBECONFIG" ]; then
    print_error "kubeconfig not found. Run ./setup-kubectl.sh first"
    exit 1
fi

print_status "Testing component server deployment..."

# Check if namespace exists
if ! kubectl get namespace stepflow-demo > /dev/null 2>&1; then
    print_error "Namespace stepflow-demo not found. Deploy with:"
    echo "  kubectl apply -k k8s/component-server/"
    exit 1
fi

# Check deployment
print_info "Checking deployment..."
kubectl get deployment -n stepflow-demo component-server

# Check pods
print_info "Checking pods..."
kubectl get pods -n stepflow-demo -l app=component-server

# Wait for pods to be ready
print_status "Waiting for pods to be ready..."
kubectl wait --for=condition=Ready pods -l app=component-server -n stepflow-demo --timeout=60s

# Check service
print_info "Checking service..."
kubectl get service -n stepflow-demo component-server

# Port forward for testing
print_status "Setting up port forward for testing..."
print_info "Port forwarding 9090 -> component-server:8080"

# Kill any existing port-forward on 9090
pkill -f "kubectl.*port-forward.*9090" || true
sleep 1

# Start port-forward in background
kubectl port-forward -n stepflow-demo service/component-server 9090:8080 > /dev/null 2>&1 &
PF_PID=$!

# Wait for port-forward to be ready
sleep 3

# Test health endpoint
print_status "Testing health endpoint..."
if curl -s -f http://localhost:9090/health > /dev/null 2>&1; then
    print_status "✅ Health check passed!"
else
    print_error "❌ Health check failed"
    kill $PF_PID || true
    exit 1
fi

# Test component listing
print_status "Testing component discovery..."
COMPONENTS=$(curl -s http://localhost:9090/components 2>/dev/null || echo "[]")
if echo "$COMPONENTS" | grep -q "double"; then
    print_status "✅ Component discovery working!"
    echo "Available components:"
    echo "$COMPONENTS" | python3 -m json.tool 2>/dev/null || echo "$COMPONENTS"
else
    print_error "❌ Component discovery failed"
    kill $PF_PID || true
    exit 1
fi

# Test a simple component execution
print_status "Testing component execution (double)..."
RESULT=$(curl -s -X POST http://localhost:9090/execute \
    -H "Content-Type: application/json" \
    -d '{"component": "double", "input": {"value": 5}}' 2>/dev/null || echo "{}")

if echo "$RESULT" | grep -q "result.*10"; then
    print_status "✅ Component execution working!"
    echo "Result: $RESULT"
else
    print_error "❌ Component execution failed"
    echo "Got: $RESULT"
    kill $PF_PID || true
    exit 1
fi

# Clean up port-forward
kill $PF_PID || true

print_status "🎉 All tests passed!"

echo ""
echo "Component server is ready for use!"
echo ""
echo "To access from Stepflow on Mac:"
echo "  kubectl port-forward -n stepflow-demo service/component-server 8080:8080"
echo "  # Then configure Stepflow with: http://localhost:8080"
