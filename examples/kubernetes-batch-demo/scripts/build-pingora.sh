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

# Build and push Pingora load balancer image to k3s local registry

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

print_status "Building Pingora load balancer Docker image..."

# Build image inside Lima VM
limactl shell --workdir /home/lima.linux/stepflow/examples/kubernetes-batch-demo "$LIMA_INSTANCE" bash << EOF
set -e

# Build the image (multi-stage build with Rust compilation)
echo "Building Pingora image (this may take a few minutes)..."
sudo docker build -f docker/Dockerfile.pingora -t localhost:5000/pingora-lb:latest .

# Push to local registry
echo "Pushing to local registry..."
sudo docker push localhost:5000/pingora-lb:latest

echo "✅ Pingora image built and pushed successfully!"
sudo k3s ctr images list | grep pingora-lb || true
EOF

print_status "✅ Pingora load balancer image ready!"
print_info "Image: localhost:5000/pingora-lb:latest"

echo ""
echo "Next steps:"
echo "  kubectl apply -k k8s/pingora-lb/"
echo "  kubectl get pods -n stepflow-demo -l app=pingora-lb"
