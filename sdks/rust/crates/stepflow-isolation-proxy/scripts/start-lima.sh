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

# Start a Lima VM for Firecracker development on macOS.
# Auto-detects the best backend (VZ with nested virt, or QEMU fallback).
#
# Usage:
#   ./scripts/start-lima.sh              # Create/start VM
#   ./scripts/start-lima.sh --shell      # Open shell in VM
#   ./scripts/start-lima.sh --name foo   # Custom instance name

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROXY_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
LIMA_INSTANCE="${LIMA_INSTANCE:-stepflow-firecracker}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC} $1"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Detect if VZ nested virtualization is supported (macOS 15+ with M3+)
check_vz_support() {
    local major
    major=$(sw_vers -productVersion | cut -d. -f1)
    if [ "$major" -lt 15 ]; then
        return 1
    fi
    local chip_line
    chip_line=$(system_profiler SPHardwareDataType 2>/dev/null | grep "Chip:")
    # "Chip: Apple M5 Pro" → check if it contains M3/M4/M5
    case "$chip_line" in
        *M3*|*M4*|*M5*) return 0 ;;
        *) return 1 ;;
    esac
}

select_config() {
    if check_vz_support; then
        info "Using VZ backend with nested virtualization" >&2
        echo "$PROXY_DIR/config/lima-firecracker-vz.yaml"
    else
        warn "VZ not supported — Firecracker requires nested virtualization (macOS 15+ with M3+)"
        exit 1
    fi
}

# Parse args
START_MODE=""
while [[ $# -gt 0 ]]; do
    case $1 in
        --shell)  START_MODE="shell"; shift ;;
        --name)   LIMA_INSTANCE="$2"; shift 2 ;;
        --help|-h)
            echo "Usage: $0 [--shell] [--name <instance>]"
            exit 0
            ;;
        *) error "Unknown option: $1"; exit 1 ;;
    esac
done

# Check Lima installed
if ! command -v limactl &>/dev/null; then
    error "Lima not installed. Run: brew install lima"
    exit 1
fi

# Already running?
if limactl list 2>/dev/null | grep -q "$LIMA_INSTANCE.*Running"; then
    info "Lima instance '$LIMA_INSTANCE' already running"
    if [ "$START_MODE" = "shell" ]; then
        exec limactl shell "$LIMA_INSTANCE"
    fi
    echo ""
    echo "Connect:  limactl shell $LIMA_INSTANCE"
    echo "Stop:     limactl stop $LIMA_INSTANCE"
    echo "Delete:   limactl delete $LIMA_INSTANCE"
    exit 0
fi

# Exists but stopped?
if limactl list 2>/dev/null | grep -q "$LIMA_INSTANCE"; then
    info "Starting existing Lima instance '$LIMA_INSTANCE'..."
    limactl start "$LIMA_INSTANCE" --tty=false
else
    LIMA_CONFIG=$(select_config)
    info "Creating Lima instance '$LIMA_INSTANCE' from $LIMA_CONFIG..."
    LIMA_SSH_PORT_FORWARDER=false limactl start --name="$LIMA_INSTANCE" "$LIMA_CONFIG" --tty=false
fi

# Wait for ready
info "Waiting for Lima instance..."
for i in $(seq 1 30); do
    if limactl list 2>/dev/null | grep -q "$LIMA_INSTANCE.*Running"; then
        break
    fi
    sleep 2
done

if ! limactl list 2>/dev/null | grep -q "$LIMA_INSTANCE.*Running"; then
    error "Timeout waiting for Lima instance"
    exit 1
fi

info "Lima instance '$LIMA_INSTANCE' ready!"

if [ "$START_MODE" = "shell" ]; then
    exec limactl shell "$LIMA_INSTANCE"
fi

echo ""
echo "Next steps:"
echo "  limactl shell $LIMA_INSTANCE"
echo ""
echo "  # Inside the VM:"
echo "  cd $(echo $PROXY_DIR | sed "s|$HOME|/home/$USER.linux|")"
echo "  ./scripts/build-rootfs.sh     # Build Alpine rootfs with vsock worker"
echo "  source \$HOME/.cargo/env"
echo "  cargo build -p stepflow-isolation-proxy"
