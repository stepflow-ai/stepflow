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

# Set up Firecracker on a Linux host (CI or dev machine).
# Installs Firecracker, enables KVM, and sets up networking prerequisites.
#
# Usage:
#   sudo ./scripts/setup-firecracker-ci.sh
#
# For GitHub Actions, run this in a step before test-firecracker.sh.

set -euo pipefail

echo "Setting up Firecracker environment..."

# Detect architecture
ARCH=$(uname -m)
case "$ARCH" in
    x86_64)  FC_ARCH="x86_64" ;;
    aarch64) FC_ARCH="aarch64" ;;
    *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

# Install Firecracker
FIRECRACKER_VERSION="v1.13.1"
if ! command -v firecracker &>/dev/null; then
    echo "Installing Firecracker $FIRECRACKER_VERSION ($FC_ARCH)..."
    wget -q -O /tmp/firecracker.tgz \
        "https://github.com/firecracker-microvm/firecracker/releases/download/${FIRECRACKER_VERSION}/firecracker-${FIRECRACKER_VERSION}-${FC_ARCH}.tgz"
    tar -xzf /tmp/firecracker.tgz -C /tmp
    mv "/tmp/release-${FIRECRACKER_VERSION}-${FC_ARCH}/firecracker-${FIRECRACKER_VERSION}-${FC_ARCH}" /usr/local/bin/firecracker
    mv "/tmp/release-${FIRECRACKER_VERSION}-${FC_ARCH}/jailer-${FIRECRACKER_VERSION}-${FC_ARCH}" /usr/local/bin/jailer
    chmod +x /usr/local/bin/firecracker /usr/local/bin/jailer
    rm -rf /tmp/firecracker.tgz "/tmp/release-${FIRECRACKER_VERSION}-${FC_ARCH}"
    echo "Firecracker installed: $(firecracker --version)"
else
    echo "Firecracker already installed: $(firecracker --version)"
fi

# Enable KVM access
if [ -c /dev/kvm ]; then
    chmod 666 /dev/kvm
    echo "KVM enabled: /dev/kvm"
else
    echo "WARNING: /dev/kvm not found — Firecracker won't work"
    echo "On GitHub Actions, KVM is available on ubuntu-latest runners"
fi

# Enable IP forwarding (needed for VM networking)
sysctl -w net.ipv4.ip_forward=1 >/dev/null 2>&1 || true
echo "IP forwarding enabled"

# Install iptables if missing
if ! command -v iptables &>/dev/null; then
    apt-get update -qq && apt-get install -y -qq iptables >/dev/null
    echo "iptables installed"
fi

echo "Firecracker environment ready"
