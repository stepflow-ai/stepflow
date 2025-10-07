#!/bin/bash
set -e

# Configure kubectl on Mac to access k3s cluster in Lima VM

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIMA_INSTANCE="${LIMA_INSTANCE:-stepflow-k3s}"
KUBECONFIG_FILE="$SCRIPT_DIR/../kubeconfig"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_status() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
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

print_status "Copying kubeconfig from Lima VM..."

# Copy kubeconfig from Lima VM
limactl shell --workdir /tmp "$LIMA_INSTANCE" sudo cat /etc/rancher/k3s/k3s.yaml > "$KUBECONFIG_FILE"

# Replace localhost with the Lima VM's IP or use port forwarding
# Since we're using GRPC port forwarding, localhost:6443 should work
print_status "Configuring kubeconfig for Mac access..."
sed -i.bak 's/127.0.0.1:6443/127.0.0.1:6443/g' "$KUBECONFIG_FILE"
rm -f "$KUBECONFIG_FILE.bak"

# Set permissions
chmod 600 "$KUBECONFIG_FILE"

print_status "✅ kubectl configured successfully!"

echo ""
echo "To use kubectl with this cluster:"
echo "  export KUBECONFIG=$KUBECONFIG_FILE"
echo ""
echo "Or permanently add to your shell:"
echo "  echo 'export KUBECONFIG=$KUBECONFIG_FILE' >> ~/.bashrc"
echo "  echo 'export KUBECONFIG=$KUBECONFIG_FILE' >> ~/.zshrc"
echo ""
echo "Test the connection:"
echo "  export KUBECONFIG=$KUBECONFIG_FILE"
echo "  kubectl get nodes"
echo "  kubectl cluster-info"
echo ""

# Test connection
print_status "Testing connection..."
if KUBECONFIG="$KUBECONFIG_FILE" kubectl get nodes > /dev/null 2>&1; then
    print_status "🎉 Successfully connected to k3s cluster!"
    echo ""
    KUBECONFIG="$KUBECONFIG_FILE" kubectl get nodes
else
    print_warning "⚠️  Could not connect to cluster. Port forwarding might not be ready yet."
    print_info "Wait a few seconds and try: kubectl get nodes"
fi
