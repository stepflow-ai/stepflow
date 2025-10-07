#!/bin/bash
set -e

# Build and push component server image to k3s local registry

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
LIMA_INSTANCE="${LIMA_INSTANCE:-stepflow-k3s}"

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

# Check if Lima instance is running
if ! limactl list | grep -q "$LIMA_INSTANCE.*Running"; then
    print_error "Lima instance '$LIMA_INSTANCE' is not running"
    echo "Start it with: ./start-lima-k3s.sh"
    exit 1
fi

print_status "Building component server Docker image..."

# Build image inside Lima VM (use repo root as context to access sdks/python)
limactl shell --workdir /home/lima.linux/stepflow "$LIMA_INSTANCE" bash << EOF
set -e

# Build the image with repo root as context
echo "Building Docker image..."
sudo k3s ctr images pull docker.io/library/python:3.12-slim
sudo docker build -f examples/kubernetes-batch-demo/docker/Dockerfile.component-server \
  -t localhost:5000/stepflow-component-server:latest \
  .

# Tag and push to local registry
echo "Pushing to local registry..."
sudo docker push localhost:5000/stepflow-component-server:latest

echo "✅ Image built and pushed successfully!"
sudo k3s ctr images list | grep stepflow-component-server || true
EOF

print_status "✅ Component server image ready!"
print_info "Image: localhost:5000/stepflow-component-server:latest"

echo ""
echo "Next steps:"
echo "  kubectl apply -k k8s/component-server/"
echo "  kubectl get pods -n stepflow-demo"
