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

# Unified port-forward script for Kubernetes services
# Usage: ./start-port-forward.sh [stepflow|pingora]
#
# stepflow - Forward to Stepflow server (primary workflow)
# pingora  - Forward to Pingora load balancer (debugging only)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
LIMA_INSTANCE="${LIMA_INSTANCE:-stepflow-k3s}"

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

print_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Parse service argument
SERVICE=${1:-stepflow}

case $SERVICE in
  stepflow)
    SERVICE_NAME="stepflow-server"
    LOCAL_PORT=7840
    REMOTE_PORT=7840
    NAMESPACE="stepflow-demo"
    USE_LIMA=false
    DESCRIPTION="Stepflow server (primary workflow)"
    ;;
  pingora)
    SERVICE_NAME="pingora-lb"
    LOCAL_PORT=80
    REMOTE_PORT=8080
    NAMESPACE="stepflow-demo"
    USE_LIMA=true
    DESCRIPTION="Pingora load balancer (debugging only)"
    ;;
  *)
    echo "Usage: $0 [stepflow|pingora]"
    echo ""
    echo "Services:"
    echo "  stepflow - Forward to Stepflow server (default, primary workflow)"
    echo "  pingora  - Forward to Pingora load balancer (debugging only)"
    exit 1
    ;;
esac

echo ""
print_status "🔌 Starting port-forward to $DESCRIPTION..."
echo ""

# Special warnings for pingora
if [ "$SERVICE" = "pingora" ]; then
    print_warning "For workflow execution, use 'stepflow' instead"
    print_warning "This script is for direct component server access (debugging)"
    echo ""
fi

# Setup kubectl (for stepflow, use direct kubeconfig)
if [ "$USE_LIMA" = "false" ]; then
    export KUBECONFIG="$PROJECT_DIR/kubeconfig"

    # Kill any existing port-forward
    print_info "Stopping any existing port-forward on port $LOCAL_PORT..."
    pkill -f "kubectl.*port-forward.*$LOCAL_PORT" 2>/dev/null || true
    sleep 1

    # Start port-forward
    print_info "Starting port-forward: localhost:$LOCAL_PORT → $SERVICE_NAME.$NAMESPACE:$REMOTE_PORT"
    kubectl port-forward -n "$NAMESPACE" "service/$SERVICE_NAME" "$LOCAL_PORT:$REMOTE_PORT"
else
    # For pingora, use Lima
    # Check if Lima instance is running
    if ! limactl list | grep -q "$LIMA_INSTANCE.*Running"; then
        print_error "Lima instance '$LIMA_INSTANCE' is not running"
        echo "Start it with: ./scripts/start-lima-k3s.sh"
        exit 1
    fi

    print_info "This will forward Lima VM port $LOCAL_PORT to your Mac's localhost:$LOCAL_PORT"

    # Start port forwarding in Lima VM (runs in foreground)
    limactl shell --workdir /tmp "$LIMA_INSTANCE" kubectl port-forward \
      --address 0.0.0.0 \
      -n "$NAMESPACE" \
      "service/$SERVICE_NAME" \
      "$LOCAL_PORT:$REMOTE_PORT"
fi

echo ""
print_status "✅ Port-forward stopped"
