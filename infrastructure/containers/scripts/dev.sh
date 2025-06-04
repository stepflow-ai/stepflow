#!/bin/bash
set -euo pipefail

# StepFlow Local Development Script for Lima/nerdctl
# Usage: ./dev.sh <command> [options]

# Configuration
LIMA_INSTANCE="${LIMA_INSTANCE:-containerd-dev}"
PROJECT_ROOT="${PROJECT_ROOT:-$(cd ../../.. && pwd)}"
REGISTRY="${REGISTRY:-localhost:5000}"

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Functions
log() {
    echo -e "${GREEN}[DEV]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1" >&2
    exit 1
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

usage() {
    cat << EOF
Usage: $0 <command> [options]

Commands:
  setup           Setup Lima VM and development environment
  start           Start Lima VM and services
  stop            Stop Lima VM
  build           Build component images
  run             Run a component locally
  test            Test component integration
  registry        Manage local registry
  clean           Clean up resources

Examples:
  $0 setup                    # Initial setup
  $0 start                    # Start development environment
  $0 build core              # Build core component
  $0 run python-sdk          # Run Python SDK locally
  $0 test --integration      # Run integration tests

EOF
}

# Setup Lima VM and environment
setup_lima() {
    log "Setting up Lima VM for containerd development..."
    
    # Check if Lima is installed
    if ! command -v limactl &> /dev/null; then
        error "Lima not found. Please install Lima first: brew install lima"
    fi
    
    # Create Lima configuration
    cat > /tmp/containerd-dev.yaml << 'EOF'
# Lima configuration for StepFlow development
images:
  - location: "https://cloud-images.ubuntu.com/releases/22.04/release/ubuntu-22.04-server-cloudimg-arm64.img"
    arch: "aarch64"
  - location: "https://cloud-images.ubuntu.com/releases/22.04/release/ubuntu-22.04-server-cloudimg-amd64.img"
    arch: "x86_64"

cpus: 4
memory: "8GiB"
disk: "50GiB"

mounts:
  - location: "~"
    writable: true
  - location: "/tmp/lima"
    writable: true

containerd:
  system: true
  user: false

provision:
  - mode: system
    script: |
      #!/bin/bash
      set -eux -o pipefail
      
      # Install nerdctl
      curl -sSL https://github.com/containerd/nerdctl/releases/download/v1.7.0/nerdctl-full-1.7.0-linux-$(uname -m | sed 's/x86_64/amd64/g' | sed 's/aarch64/arm64/g').tar.gz | tar -xz -C /usr/local
      
      # Install BuildKit
      curl -sSL https://github.com/moby/buildkit/releases/download/v0.12.4/buildkit-v0.12.4.linux-$(uname -m | sed 's/x86_64/amd64/g' | sed 's/aarch64/arm64/g').tar.gz | tar -xz -C /usr/local
      
      # Start BuildKit daemon
      cat > /etc/systemd/system/buildkit.service << 'SYSTEMD'
[Unit]
Description=BuildKit
After=containerd.service

[Service]
Type=simple
ExecStart=/usr/local/bin/buildkitd
Restart=always

[Install]
WantedBy=multi-user.target
SYSTEMD
      
      systemctl daemon-reload
      systemctl enable --now buildkit
      
      # Install K3s for local Kubernetes testing
      curl -sfL https://get.k3s.io | sh -
      
      # Configure nerdctl
      mkdir -p /etc/nerdctl
      cat > /etc/nerdctl/nerdctl.toml << 'CONFIG'
namespace = "k8s.io"
debug = false
debug_full = false
insecure_registry = true
CONFIG

portForwards:
  - guestPort: 5000
    hostPort: 5000
  - guestPort: 8080
    hostPort: 8080
  - guestPort: 8081
    hostPort: 8081
  - guestPort: 8082
    hostPort: 8082
  - guestPort: 6443
    hostPort: 6443
EOF
    
    # Create Lima instance
    if limactl list | grep -q "$LIMA_INSTANCE"; then
        warn "Lima instance '$LIMA_INSTANCE' already exists"
    else
        log "Creating Lima instance '$LIMA_INSTANCE'..."
        limactl start --name="$LIMA_INSTANCE" /tmp/containerd-dev.yaml
    fi
    
    # Wait for instance to be ready
    log "Waiting for instance to be ready..."
    sleep 10
    
    # Setup local registry
    setup_registry
    
    log "Setup complete! Run '$0 start' to begin development"
}

# Start development environment
start_dev() {
    log "Starting development environment..."
    
    # Start Lima VM
    if ! limactl list | grep -q "$LIMA_INSTANCE.*Running"; then
        limactl start "$LIMA_INSTANCE"
    fi
    
    # Wait for services
    log "Waiting for services to be ready..."
    sleep 5
    
    # Check BuildKit
    if limactl shell "$LIMA_INSTANCE" buildctl debug workers &>/dev/null; then
        info "BuildKit is running"
    else
        warn "BuildKit not running, starting..."
        limactl shell "$LIMA_INSTANCE" sudo systemctl start buildkit
    fi
    
    # Check K3s
    if limactl shell "$LIMA_INSTANCE" sudo k3s kubectl get nodes &>/dev/null; then
        info "K3s is running"
    else
        warn "K3s not running, starting..."
        limactl shell "$LIMA_INSTANCE" sudo systemctl start k3s
    fi
    
    log "Development environment ready!"
    info "Registry: localhost:5000"
    info "BuildKit: available via 'lima nerdctl build'"
    info "Kubernetes: available via 'lima sudo k3s kubectl'"
}

# Stop development environment
stop_dev() {
    log "Stopping development environment..."
    limactl stop "$LIMA_INSTANCE"
}

# Build component
build_component() {
    local component=$1
    shift
    
    log "Building $component component..."
    
    # Run build script in Lima
    limactl shell "$LIMA_INSTANCE" bash -c "cd $PROJECT_ROOT && ./infrastructure/containers/scripts/build.sh $component $*"
}

# Run component locally
run_component() {
    local component=$1
    shift
    
    case $component in
        core)
            log "Running StepFlow core..."
            limactl shell "$LIMA_INSTANCE" nerdctl run --rm -it \
                -p 8080:8080 \
                -v "$PROJECT_ROOT/examples:/app/examples:ro" \
                "$REGISTRY/stepflow/core:latest" "$@"
            ;;
        python-sdk)
            log "Running Python SDK..."
            limactl shell "$LIMA_INSTANCE" nerdctl run --rm -it \
                -p 8081:8081 \
                -e STEPFLOW_MODE=stdio \
                "$REGISTRY/stepflow/python-sdk:latest" "$@"
            ;;
        inference)
            log "Running inference service..."
            limactl shell "$LIMA_INSTANCE" nerdctl run --rm -it \
                -p 8082:8082 \
                --gpus all \
                "$REGISTRY/stepflow/inference:latest" "$@"
            ;;
        *)
            error "Unknown component: $component"
            ;;
    esac
}

# Setup local registry
setup_registry() {
    log "Setting up local registry..."
    
    limactl shell "$LIMA_INSTANCE" bash -c '
        if ! nerdctl ps | grep -q registry; then
            nerdctl run -d --name registry --restart always -p 5000:5000 registry:2
        fi
    '
    
    info "Local registry available at localhost:5000"
}

# Run tests
run_tests() {
    log "Running integration tests..."
    
    # Build test images
    build_component core --tag test
    build_component python-sdk --tag test
    
    # Run test workflow
    limactl shell "$LIMA_INSTANCE" bash -c "cd $PROJECT_ROOT && cargo test --all"
}

# Clean up resources
cleanup() {
    log "Cleaning up resources..."
    
    # Remove containers
    limactl shell "$LIMA_INSTANCE" nerdctl rm -f $(limactl shell "$LIMA_INSTANCE" nerdctl ps -aq) 2>/dev/null || true
    
    # Remove images
    if [[ "$1" == "--all" ]]; then
        limactl shell "$LIMA_INSTANCE" nerdctl rmi -f $(limactl shell "$LIMA_INSTANCE" nerdctl images -q) 2>/dev/null || true
    fi
    
    log "Cleanup complete"
}

# Main script logic
case "${1:-}" in
    setup)
        setup_lima
        ;;
    start)
        start_dev
        ;;
    stop)
        stop_dev
        ;;
    build)
        shift
        build_component "$@"
        ;;
    run)
        shift
        run_component "$@"
        ;;
    test)
        shift
        run_tests "$@"
        ;;
    registry)
        setup_registry
        ;;
    clean)
        shift
        cleanup "$@"
        ;;
    -h|--help|"")
        usage
        ;;
    *)
        error "Unknown command: $1"
        ;;
esac