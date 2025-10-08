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

# Smart Lima k3s setup script
# Automatically detects best backend (VZ vs QEMU) based on system capabilities

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIMA_INSTANCE="${LIMA_INSTANCE:-stepflow-k3s}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored output
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

# Function to get macOS version
get_macos_version() {
    sw_vers -productVersion
}

# Function to get chip type
get_chip_type() {
    system_profiler SPHardwareDataType | grep "Chip:" | awk '{print $2, $3}' | sed 's/Apple //'
}

# Function to check if VZ is supported
check_vz_support() {
    local macos_version=$(get_macos_version)
    local chip=$(get_chip_type)

    print_info "Detected macOS $macos_version with $chip chip"

    # Parse version numbers
    local major=$(echo $macos_version | cut -d. -f1)
    local minor=$(echo $macos_version | cut -d. -f2)

    # Check macOS 15+ requirement
    if [ "$major" -lt 15 ]; then
        print_warning "macOS 15+ required for VZ (you have $macos_version)"
        return 1
    fi

    # Check M3+ chip requirement (M3+ has better VZ support)
    case "$chip" in
        M3|M3*|M4|M4*|M5|M5*)
            print_status "✅ VZ supported: macOS $macos_version + $chip chip"
            return 0
            ;;
        M1|M2|M1*|M2*)
            # M1/M2 can use VZ but QEMU might be more stable for some workloads
            print_info "ℹ️  VZ available but QEMU recommended for $chip chip"
            return 1
            ;;
        *)
            print_warning "❓ Unknown chip type: $chip. Using QEMU for compatibility"
            return 1
            ;;
    esac
}

# Function to select optimal configuration
select_config() {
    if check_vz_support >&2; then
        print_status "🚀 Using VZ backend for optimal performance" >&2
        echo "$SCRIPT_DIR/../config/lima-k3s-vz.yaml"
    else
        print_status "🐌 Using QEMU backend for compatibility" >&2

        # Check if socket_vmnet is available for QEMU
        if [ ! -x "/opt/socket_vmnet/bin/socket_vmnet" ]; then
            print_warning "⚠️  QEMU backend works best with socket_vmnet for networking" >&2
            print_info "   Install with: brew install socket_vmnet" >&2
            print_info "   Lima will use default networking if socket_vmnet is not available" >&2
        fi

        echo "$SCRIPT_DIR/../config/lima-k3s-qemu.yaml"
    fi
}

# Parse command line arguments
START_MODE=""
while [[ $# -gt 0 ]]; do
    case $1 in
        --shell)
            START_MODE="shell"
            shift
            ;;
        --name)
            LIMA_INSTANCE="$2"
            shift 2
            ;;
        --help|-h)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --shell         Open a shell session in the Lima VM"
            echo "  --name <name>   Use a custom Lima instance name (default: stepflow-k3s)"
            echo "  --help,-h       Show this help message"
            echo ""
            echo "Environment Variables:"
            echo "  LIMA_INSTANCE   Override the Lima instance name (default: stepflow-k3s)"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Main execution
print_status "🔍 Detecting optimal Lima backend for k3s..."

# Check if Lima is installed
if ! command -v limactl &> /dev/null; then
    print_error "Lima is not installed. Please install it first:"
    echo "  brew install lima"
    exit 1
fi

# Select configuration
LIMA_CONFIG=$(select_config)

print_info "Using configuration: $LIMA_CONFIG"

# Check if the instance is already running
if limactl list | grep -q "$LIMA_INSTANCE.*Running"; then
    print_warning "Lima instance '$LIMA_INSTANCE' is already running."

    echo ""
    echo "Management commands:"
    echo "  limactl shell --workdir /home/lima.linux/stepflow $LIMA_INSTANCE  # Connect"
    echo "  limactl stop $LIMA_INSTANCE                                        # Stop"
    echo "  limactl delete $LIMA_INSTANCE && ./start-lima-k3s.sh               # Recreate VM"
    echo ""
    echo "k3s cluster access:"
    echo "  ./setup-kubectl.sh                                                 # Configure kubectl on Mac"
    echo "  kubectl get nodes                                                   # Check cluster status"
    exit 0
fi

# Check if the instance exists but is stopped
if limactl list | grep -q "$LIMA_INSTANCE"; then
    print_status "Starting existing Lima instance '$LIMA_INSTANCE'..."
    limactl start "$LIMA_INSTANCE" --tty=false
else
    print_status "Creating and starting new Lima instance '$LIMA_INSTANCE'..."
    print_info "Using GRPC port forwarding for automatic port mapping"

    # Use GRPC port forwarding (recommended since Lima v1.1.0)
    LIMA_SSH_PORT_FORWARDER=false limactl start --name="$LIMA_INSTANCE" "$LIMA_CONFIG" --tty=false
fi

# Wait for the instance to be ready
print_status "Waiting for Lima instance to be ready..."
timeout=120
counter=0
while [ $counter -lt $timeout ]; do
    if limactl list | grep -q "$LIMA_INSTANCE.*Running"; then
        break
    fi
    sleep 2
    counter=$((counter + 2))
done

if [ $counter -ge $timeout ]; then
    print_error "Timeout waiting for Lima instance to start"
    exit 1
fi

print_status "Lima instance '$LIMA_INSTANCE' is ready!"
print_status "Stepflow project mounted at: /home/lima.linux/stepflow"

echo ""
echo "🎯 Next steps:"
echo ""
echo "1. Configure kubectl on your Mac:"
echo "   cd $SCRIPT_DIR && ./setup-kubectl.sh"
echo ""
echo "2. Verify k3s cluster:"
echo "   kubectl get nodes"
echo "   kubectl cluster-info"
echo ""
echo "3. Deploy component servers:"
echo "   kubectl apply -k ../k8s/component-server/"
echo ""
echo "4. Run batch tests:"
echo "   python3 ../workflows/test-batch.py"
echo ""
echo "📚 Management commands:"
echo "  limactl shell --workdir /home/lima.linux/stepflow $LIMA_INSTANCE  # Connect to VM"
echo "  limactl stop $LIMA_INSTANCE                                        # Stop the VM"
echo "  limactl delete $LIMA_INSTANCE                                      # Delete the VM"
echo ""

if [[ "$LIMA_CONFIG" == *"lima-k3s-vz.yaml" ]]; then
    print_status "🎉 VZ backend provides optimal performance for k3s!"
else
    print_warning "⚠️  QEMU backend is functional but may be slower than VZ"
fi

# Start shell if requested
if [ "$START_MODE" = "shell" ]; then
    print_status "🐚 Opening shell session..."
    limactl shell --workdir /home/lima.linux/stepflow "$LIMA_INSTANCE"
fi
