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

# Build an Alpine Linux rootfs for Firecracker VMs with the Stepflow vsock worker.
#
# Produces:
#   build/vmlinux.bin           - Firecracker-compatible kernel
#   build/stepflow-rootfs.ext4  - 512MB ext4 rootfs with Python + stepflow_py
#
# Must be run inside a Linux environment (Lima VM or native Linux).
# Requires: sudo, curl, mkfs.ext4, chroot

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROXY_DIR="$(dirname "$SCRIPT_DIR")"
REPO_ROOT="$(cd "$PROXY_DIR/../../.." && pwd)"
PY_SDK_DIR="$REPO_ROOT/sdks/python/stepflow-py"

BUILD_DIR="$PROXY_DIR/build"
ROOTFS_IMG="$BUILD_DIR/stepflow-rootfs.ext4"
KERNEL_PATH="$BUILD_DIR/vmlinux.bin"

echo "🏗️  Building Alpine Linux rootfs with Stepflow vsock worker..."

# Verify Python SDK exists
if [ ! -f "$PY_SDK_DIR/pyproject.toml" ]; then
    echo "❌ Python SDK not found at $PY_SDK_DIR"
    exit 1
fi

# Detect architecture
ARCH=$(uname -m)
case "$ARCH" in
    "x86_64")
        ALPINE_ARCH="x86_64"
        KERNEL_URL="https://s3.amazonaws.com/spec.ccfc.min/firecracker-ci/v1.7/x86_64/vmlinux-5.10.209"
        ;;
    "aarch64"|"arm64")
        ALPINE_ARCH="aarch64"
        KERNEL_URL="https://s3.amazonaws.com/spec.ccfc.min/firecracker-ci/v1.7/aarch64/vmlinux-5.10.209"
        ;;
    *)
        echo "❌ Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

mkdir -p "$BUILD_DIR"

# Skip if rootfs already exists (use --force-build in test script to rebuild)
if [ -f "$ROOTFS_IMG" ] && [ -f "$KERNEL_PATH" ]; then
    echo "✅ Rootfs already exists at $ROOTFS_IMG ($(ls -lh "$ROOTFS_IMG" | awk '{print $5}'))"
    echo "   Delete build/ directory to force a rebuild."
    exit 0
fi

# Download kernel
if [ ! -f "$KERNEL_PATH" ]; then
    echo "📦 Downloading Firecracker kernel..."
    curl -L -o "$KERNEL_PATH" "$KERNEL_URL"
fi

# Build rootfs in temp directory
WORK_DIR=$(mktemp -d)
trap "sudo rm -rf $WORK_DIR" EXIT

cd "$WORK_DIR"

echo "📦 Downloading Alpine Linux 3.18 ($ALPINE_ARCH)..."
ALPINE_VERSION="3.18"
ALPINE_RELEASE="alpine-minirootfs-${ALPINE_VERSION}.4-${ALPINE_ARCH}.tar.gz"
curl -L -o "$ALPINE_RELEASE" "https://dl-cdn.alpinelinux.org/alpine/v${ALPINE_VERSION}/releases/${ALPINE_ARCH}/$ALPINE_RELEASE"

echo "🗃️  Extracting..."
mkdir alpine-root
cd alpine-root
tar -xzf "../$ALPINE_RELEASE"

# Copy DNS for chroot
sudo cp /etc/resolv.conf etc/ < /dev/null

# Mount virtual filesystems for chroot
sudo mount -t proc proc proc/ < /dev/null
sudo mount -t sysfs sysfs sys/ < /dev/null
sudo mount --bind /dev dev/ < /dev/null
sudo mount --bind /run run/ < /dev/null

# Copy Python SDK into chroot for installation
sudo mkdir -p opt/stepflow-py < /dev/null
sudo cp -r "$PY_SDK_DIR"/* opt/stepflow-py/ < /dev/null

# ─── Chroot: install packages and Python SDK only ───
# Keep this minimal — no config files, no heredocs with special chars
sudo chroot . /bin/ash -c '
set -e
apk update
apk add --no-cache python3 py3-pip python3-dev build-base linux-headers curl openrc ca-certificates
pip install /opt/stepflow-py --break-system-packages 2>/dev/null || pip install /opt/stepflow-py
rm -rf /opt/stepflow-py
apk cache clean
' < /dev/null

# Unmount chroot
echo "🧹 Unmounting chroot..."
sudo umount -l run/ 2>/dev/null || true
sudo umount -l dev/ 2>/dev/null || true
sudo umount -l sys/ 2>/dev/null || true
sudo umount -l proc/ 2>/dev/null || true
sleep 1

# ─── Write config files directly (no chroot, no heredoc escaping issues) ───

# Copy test worker script
sudo mkdir -p opt/stepflow
sudo cp "$PROXY_DIR/tests/test_worker.py" opt/stepflow/worker.py

# Write a wrapper that logs to serial console for debugging
sudo tee opt/stepflow/run_worker.py > /dev/null << 'PYEOF'
#!/usr/bin/python3
"""Wrapper that runs the vsock worker with console logging for debugging."""
import sys, os

console = open('/dev/console', 'w')
def log(msg):
    console.write(f'WORKER: {msg}\n')
    console.flush()

log('starting...')

# Route ALL Python logging to the serial console so timing data is visible
import logging
console_handler = logging.StreamHandler(console)
console_handler.setLevel(logging.INFO)
console_handler.setFormatter(logging.Formatter('%(name)s: %(message)s'))
logging.root.addHandler(console_handler)
logging.root.setLevel(logging.INFO)

try:
    from stepflow_py.worker.server import StepflowServer
    from stepflow_py.worker.vsock_worker import run_vsock_worker
    import msgspec

    # Pre-load grpcio and proto modules so they're in memory when the snapshot
    # is captured. Without this, these imports happen on-demand after restore,
    # causing ~600ms of page faults as the memory is loaded from the snapshot file.
    log('pre-loading grpc and proto modules...')
    import grpc
    import grpc.aio
    from google.protobuf import struct_pb2
    from stepflow_py.proto.orchestrator_pb2_grpc import OrchestratorServiceStub
    from stepflow_py.proto import (
        TaskHeartbeatRequest, CompleteTaskRequest, ComponentExecuteResponse, TaskError,
    )
    from stepflow_py.worker.task_handler import (
        handle_task, execute_component, proto_value_to_python, python_to_proto_value,
    )
    from stepflow_py.worker.grpc_context import GrpcContext
    from stepflow_py.worker.orchestrator_tracker import OrchestratorTracker
    log('pre-load complete')

    # Simulate a heavy dependency load (like Langflow, ML models, etc.)
    # In a real scenario this would be `import langflow` or `import torch`.
    # The key insight: in the snapshot case, this cost is paid once at snapshot time.
    # In the subprocess case, it's paid on every single task invocation.
    log('simulating heavy dependency load...')
    import time as _time
    import hashlib
    _heavy_load_start = _time.monotonic()
    # Build a lookup table simulating loaded ML model weights / config.
    # In a subprocess, this runs on every task. With snapshots, it's free.
    _lookup_table = {}
    for i in range(500_000):
        key = hashlib.md5(str(i).encode()).hexdigest()[:12]
        _lookup_table[key] = i * 42
    _heavy_load_ms = (_time.monotonic() - _heavy_load_start) * 1000
    log(f'TIMING: heavy_dependency_load={_heavy_load_ms:.0f}ms ({len(_lookup_table)} entries)')

    class DoubleInput(msgspec.Struct):
        value: int

    class DoubleOutput(msgspec.Struct):
        result: int

    server = StepflowServer()

    import time as _time

    @server.component
    def double(input: DoubleInput) -> DoubleOutput:
        t = _time.monotonic()
        # Use the pre-loaded lookup table to prove snapshot data is accessible.
        # Look up the hash of the input value and verify it matches.
        key = hashlib.md5(str(input.value).encode()).hexdigest()[:12]
        looked_up = _lookup_table.get(key)
        expected = input.value * 42
        if looked_up != expected:
            log(f'WARNING: lookup mismatch for {input.value}: got {looked_up}, expected {expected}')
        result = DoubleOutput(result=input.value * 2)
        log(f'double({input.value}) done in {(_time.monotonic()-t)*1000:.1f}ms (lookup_table has {len(_lookup_table)} entries, verified={looked_up == expected})')
        return result

    log('listening on vsock port 5000')
    import asyncio
    # No oneshot — worker loops accepting connections. The proxy kills the VM when done.
    # This also allows snapshot/restore: the worker is captured in accept() and resumes.
    asyncio.run(run_vsock_worker(server=server, vsock_port=5000))
    log('done')
except Exception as e:
    log(f'CRASHED: {e}')
    import traceback
    traceback.print_exc(file=console)
    sys.exit(1)
PYEOF

# Inittab (busybox init)
sudo tee etc/inittab > /dev/null << 'EOF'
::sysinit:/sbin/openrc sysinit
::sysinit:/sbin/openrc boot
::wait:/sbin/openrc default
ttyS0::respawn:/sbin/getty -L ttyS0 115200 vt100
::ctrlaltdel:/sbin/reboot
::shutdown:/sbin/openrc shutdown
EOF

# OpenRC init script for the vsock worker
sudo tee etc/init.d/stepflow-worker > /dev/null << 'EOF'
#!/sbin/openrc-run

name="Stepflow Vsock Worker"
command="/usr/bin/python3"
command_args="/opt/stepflow/run_worker.py"
pidfile="/var/run/stepflow-worker.pid"
command_background="yes"
output_log="/var/log/stepflow-worker.log"
error_log="/var/log/stepflow-worker.err"

depend() {
    need net
}
EOF
sudo chmod +x etc/init.d/stepflow-worker

# Enable services via symlinks (rc-update inside chroot doesn't persist)
sudo mkdir -p etc/runlevels/default etc/runlevels/boot
sudo ln -sf /etc/init.d/stepflow-worker etc/runlevels/default/stepflow-worker
sudo ln -sf /etc/init.d/networking etc/runlevels/boot/networking

# Static networking with ARP flush on up (critical for snapshot restore —
# the restored VM's ARP cache has the old TAP's MAC, flush forces re-ARP)
sudo tee etc/network/interfaces > /dev/null << 'EOF'
auto lo
iface lo inet loopback

auto eth0
iface eth0 inet static
    address 192.168.241.2
    netmask 255.255.255.248
    gateway 192.168.241.1
    post-up ip neigh flush all
EOF

# Set very short ARP cache timeout so stale entries from snapshots expire fast
sudo mkdir -p etc/sysctl.d
sudo tee etc/sysctl.d/arp.conf > /dev/null << 'EOF'
net.ipv4.neigh.default.base_reachable_time_ms = 100
net.ipv4.neigh.default.gc_stale_time = 1
EOF

# DNS
sudo tee etc/resolv.conf > /dev/null << 'EOF'
nameserver 8.8.8.8
nameserver 8.8.4.4
EOF

# Hostname
echo "localhost" | sudo tee etc/hostname > /dev/null

# ─── Create ext4 image ───

echo "📦 Creating rootfs image (512MB)..."
cd ..
dd if=/dev/zero of=stepflow-rootfs.ext4 bs=1M count=512 2>/dev/null
mkfs.ext4 -F stepflow-rootfs.ext4 >/dev/null

mkdir -p rootfs-mount
sudo mount -o loop stepflow-rootfs.ext4 rootfs-mount/ < /dev/null
sudo cp -a alpine-root/* rootfs-mount/
sudo umount rootfs-mount/

cp stepflow-rootfs.ext4 "$ROOTFS_IMG"

echo ""
echo "✅ Rootfs built successfully!"
echo "   Kernel: $KERNEL_PATH"
echo "   Rootfs: $ROOTFS_IMG"
