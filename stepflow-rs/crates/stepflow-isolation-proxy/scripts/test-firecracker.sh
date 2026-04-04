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

# End-to-end test for the Firecracker isolation backend.
#
# Builds everything, boots a Firecracker VM, dispatches a task via the
# isolation proxy, and verifies the result.
#
# Works in two modes:
#   - Linux (native or CI): runs Firecracker directly (requires /dev/kvm)
#   - macOS: runs inside a Lima VM
#
# Usage:
#   ./scripts/test-firecracker.sh                # Full test (builds are incremental)
#   ./scripts/test-firecracker.sh --force-build  # Clean and rebuild everything
#
# Prerequisites:
#   macOS: Lima (brew install lima), M3+ chip, macOS 15+
#   Linux: /dev/kvm, firecracker (or run setup-firecracker-ci.sh first)
#   Both:  Rust toolchain, uv (Python), protoc
#
# For GitHub Actions CI:
#   - Needs: ubuntu-latest runner with KVM enabled
#   - Run setup-firecracker-ci.sh as a setup step
#   - protoc installed via arduino/setup-protoc action

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROXY_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$PROXY_DIR/../../.." && pwd)"
STEPFLOW_RS_DIR="$REPO_ROOT/stepflow-rs"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

info()  { echo -e "${BLUE}[INFO]${NC} $1"; }
ok()    { echo -e "${GREEN}[OK]${NC} $1"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }
fail()  { echo -e "${RED}[FAIL]${NC} $1"; }
step()  { echo -e "\n${GREEN}━━━ $1 ━━━${NC}"; }

FORCE_BUILD=false
for arg in "$@"; do
    case $arg in
        --force-build) FORCE_BUILD=true ;;
    esac
done

# Detect platform
OS="$(uname -s)"
ARCH="$(uname -m)"

# ─────────────────────────────────────────────────────────────────────────────
# Platform detection and Lima wrapper
# ─────────────────────────────────────────────────────────────────────────────

LIMA_INSTANCE="stepflow-firecracker"

# Run a command — on macOS, runs inside Lima; on Linux, runs directly.
# Returns the exit code of the command.
run_cmd() {
    if [ "$OS" = "Darwin" ]; then
        limactl shell "$LIMA_INSTANCE" -- bash -c "cd $PROXY_DIR && $*"
    else
        bash -c "cd $PROXY_DIR && $*"
    fi
}

# Run a command that needs sudo — on macOS via Lima, on Linux directly.
run_sudo() {
    if [ "$OS" = "Darwin" ]; then
        limactl shell "$LIMA_INSTANCE" -- bash -c "cd $PROXY_DIR && sudo $*"
    else
        sudo bash -c "cd $PROXY_DIR && $*"
    fi
}

# ─────────────────────────────────────────────────────────────────────────────
# Cleanup
# ─────────────────────────────────────────────────────────────────────────────

kill_by_pidfile() {
    local pidfile="$1"
    local name="$2"
    local pid
    pid=$(run_cmd "cat $pidfile 2>/dev/null" 2>/dev/null) || return 0
    if [ -n "$pid" ]; then
        info "Stopping $name (pid $pid)..."
        run_cmd "kill $pid 2>/dev/null" || true
        # Wait for graceful shutdown (up to 10s)
        for i in $(seq 1 20); do
            if ! run_cmd "kill -0 $pid 2>/dev/null"; then
                break
            fi
            sleep 0.5
        done
        # Force kill if still alive
        run_cmd "kill -9 $pid 2>/dev/null" || true
        run_cmd "rm -f $pidfile" || true
    fi
}

cleanup() {
    info "Cleaning up..."
    kill_by_pidfile /tmp/stepflow-proxy.pid "isolation proxy"
    kill_by_pidfile /tmp/stepflow-orch.pid "orchestrator"
    # Clean up firecracker/jailer processes (exclude this script and grep itself)
    run_sudo "pgrep -f 'firecracker --api-sock' | xargs -r kill 2>/dev/null" || true
    run_sudo "pgrep -f 'jailer --id' | xargs -r kill 2>/dev/null" || true
    run_sudo "bash -c 'for ns in \$(ip netns list 2>/dev/null | grep fc-ns | cut -d\" \" -f1); do ip netns del \$ns 2>/dev/null; done'" || true
    run_sudo "bash -c 'for v in \$(ip link show 2>/dev/null | grep veth | cut -d: -f2 | tr -d \" \"); do ip link del \$v 2>/dev/null; done'" || true
    run_sudo "rm -rf /srv/jailer/firecracker/* 2>/dev/null" || true
    run_sudo "rm -rf /tmp/firecracker-snapshots 2>/dev/null" || true
}
trap cleanup EXIT

# Clean up any leftover resources from previous runs
info "Cleaning up stale resources..."
cleanup 2>/dev/null || true

# ─────────────────────────────────────────────────────────────────────────────
# Prerequisites
# ─────────────────────────────────────────────────────────────────────────────

step "Checking prerequisites"

if [ "$OS" = "Darwin" ]; then
    info "Platform: macOS ($ARCH) — will use Lima VM"

    if ! command -v limactl &>/dev/null; then
        fail "Lima not installed. Run: brew install lima"
        exit 1
    fi

    # Start Lima if not running
    if ! limactl list 2>/dev/null | grep -q "$LIMA_INSTANCE.*Running"; then
        info "Starting Lima instance..."
        "$SCRIPT_DIR/start-lima.sh"
    fi
    ok "Lima VM running"

    # Check KVM inside Lima
    if ! run_cmd "test -c /dev/kvm"; then
        fail "KVM not available inside Lima VM (need VZ backend with M3+ chip)"
        exit 1
    fi
    ok "KVM available inside Lima"

    # Fix DNS and hostname (VZ networking quirks)
    run_cmd "grep -q nameserver /etc/resolv.conf 2>/dev/null || sudo bash -c 'echo nameserver 8.8.8.8 > /etc/resolv.conf'"
    run_cmd "grep -q \$(hostname) /etc/hosts 2>/dev/null || sudo bash -c 'echo 127.0.0.1 \$(hostname) >> /etc/hosts'"
else
    info "Platform: Linux ($ARCH) — will run Firecracker directly"

    if [ ! -c /dev/kvm ]; then
        fail "KVM not available (/dev/kvm missing)"
        info "On GitHub Actions: ensure KVM is enabled (ubuntu-latest supports it)"
        exit 1
    fi
    ok "KVM available"
fi

# Check Firecracker is installed
if ! run_cmd "command -v firecracker >/dev/null"; then
    fail "Firecracker not installed"
    if [ "$OS" = "Darwin" ]; then
        info "It should be installed by Lima provisioning."
        info "Try: limactl stop $LIMA_INSTANCE && limactl delete $LIMA_INSTANCE && ./scripts/start-lima.sh"
    else
        info "Run: sudo ./scripts/setup-firecracker-ci.sh"
    fi
    exit 1
fi
FC_VERSION=$(run_cmd "firecracker --version 2>&1 | head -1")
ok "Firecracker: $FC_VERSION"

# Check protoc is available
if ! run_cmd "command -v protoc >/dev/null"; then
    fail "protoc not installed (needed to build stepflow-proto)"
    if [ "$OS" = "Darwin" ]; then
        info "Quick fix: limactl shell $LIMA_INSTANCE -- sudo apt-get install -y protobuf-compiler"
        info "Or recreate: limactl stop $LIMA_INSTANCE && limactl delete $LIMA_INSTANCE && ./scripts/start-lima.sh"
    else
        info "Run: sudo apt-get install -y protobuf-compiler"
    fi
    exit 1
fi
ok "protoc available"

# ─────────────────────────────────────────────────────────────────────────────
# Build rootfs
# ─────────────────────────────────────────────────────────────────────────────

BUILD_DIR="$PROXY_DIR/build"

if [ "$FORCE_BUILD" = true ]; then
    info "Force build: cleaning build artifacts"
    run_cmd "rm -rf build/"
fi

step "Building rootfs (incremental)"
if ! run_cmd "./scripts/build-rootfs.sh"; then
    fail "Rootfs build failed"
    exit 1
fi

if ! run_cmd "test -f build/vmlinux.bin && test -f build/stepflow-rootfs.ext4"; then
    fail "Kernel or rootfs missing after build"
    exit 1
fi
ok "Kernel and rootfs ready"

# ─────────────────────────────────────────────────────────────────────────────
# Build binaries
# ─────────────────────────────────────────────────────────────────────────────

step "Building Rust binaries (incremental)"

info "Building stepflow-server..."
if ! run_cmd "cd $STEPFLOW_RS_DIR && source ~/.cargo/env 2>/dev/null; cargo build -p stepflow-server --no-default-features --features sqlite"; then
    fail "stepflow-server build failed"
    exit 1
fi
ok "stepflow-server built"

info "Building stepflow-isolation-proxy..."
if ! run_cmd "cd $STEPFLOW_RS_DIR && source ~/.cargo/env 2>/dev/null; cargo build -p stepflow-isolation-proxy"; then
    fail "stepflow-isolation-proxy build failed"
    exit 1
fi
ok "stepflow-isolation-proxy built"

STEPFLOW_BIN="$STEPFLOW_RS_DIR/target/debug/stepflow-server"
PROXY_BIN="$STEPFLOW_RS_DIR/target/debug/stepflow-isolation-proxy"

# ─────────────────────────────────────────────────────────────────────────────
# Set up networking
# ─────────────────────────────────────────────────────────────────────────────

step "Setting up networking"

run_sudo "sysctl -w net.ipv4.ip_forward=1 >/dev/null 2>&1" || true
ok "IP forwarding enabled"

# ─────────────────────────────────────────────────────────────────────────────
# Start orchestrator
# ─────────────────────────────────────────────────────────────────────────────

step "Starting orchestrator"

# Use a random port to avoid conflicts with previous runs
ORCH_PORT=$((10000 + RANDOM % 50000))

# Write config inside the VM/host (not on macOS host where Lima can't read it)
run_cmd "cat > /tmp/stepflow-test-config.yml <<'YAML'
plugins:
  builtin:
    type: builtin
  firecracker:
    type: grpc
    queueName: firecracker
routes:
  \"/builtin/{*component}\":
    - plugin: builtin
  \"/fc/{*component}\":
    - plugin: firecracker
YAML"

# Start orchestrator with STEPFLOW_ORCHESTRATOR_URL set to an IP reachable from
# inside Firecracker VMs. The VMs reach the host via 10.0.0.1 (host veth IP).
# The orchestrator binds on 0.0.0.0 so it's accessible on all interfaces.
run_cmd "bash -c 'source ~/.cargo/env 2>/dev/null; $STEPFLOW_BIN --port $ORCH_PORT --config /tmp/stepflow-test-config.yml > /tmp/stepflow-orch.log 2>&1 & PID=\$!; echo \$PID > /tmp/stepflow-orch.pid; disown \$PID; sleep 2; kill -0 \$PID 2>/dev/null && echo OK || echo DEAD'"

# Wait for orchestrator to be healthy
info "Waiting for orchestrator on port $ORCH_PORT..."
for i in $(seq 1 30); do
    if run_cmd "curl -sf http://127.0.0.1:$ORCH_PORT/api/v1/health >/dev/null 2>&1"; then
        break
    fi
    sleep 1
done

if ! run_cmd "curl -sf http://127.0.0.1:$ORCH_PORT/api/v1/health >/dev/null 2>&1"; then
    fail "Orchestrator failed to start"
    exit 1
fi
ok "Orchestrator running on port $ORCH_PORT"

# ─────────────────────────────────────────────────────────────────────────────
# Start isolation proxy with Firecracker backend
# ─────────────────────────────────────────────────────────────────────────────

step "Starting isolation proxy (Firecracker backend)"

# Ensure jailer directory exists
run_sudo "mkdir -p /srv/jailer"

# Start proxy with jailer-based VM management
# --no-snapshot for initial test; snapshot optimization tested separately
run_cmd "bash -c 'source ~/.cargo/env 2>/dev/null; RUST_LOG=info,stepflow_isolation_proxy=debug $PROXY_BIN --tasks-url http://127.0.0.1:$ORCH_PORT --queue-name firecracker firecracker --kernel build/vmlinux.bin --rootfs build/stepflow-rootfs.ext4 --pool-size 1 > /tmp/stepflow-proxy.log 2>&1 & PID=\$!; echo \$PID > /tmp/stepflow-proxy.pid; disown \$PID; sleep 2; kill -0 \$PID 2>/dev/null && echo OK || echo DEAD'"

# Wait for proxy to connect to orchestrator (check proxy log for "Connected")
info "Waiting for proxy to initialize..."
for i in $(seq 1 120); do
    if run_cmd "grep -q 'Connected. Waiting for tasks' /tmp/stepflow-proxy.log 2>/dev/null"; then
        break
    fi
    # Only check for proxy-level errors (not kernel boot messages)
    if run_cmd "grep -q 'stepflow_isolation_proxy.*ERROR\|panicked at' /tmp/stepflow-proxy.log 2>/dev/null"; then
        fail "Proxy error detected. Log:"
        run_cmd "grep 'ERROR\|panicked' /tmp/stepflow-proxy.log" || true
        exit 1
    fi
    sleep 2
done

if ! run_cmd "grep -q 'Connected. Waiting for tasks' /tmp/stepflow-proxy.log 2>/dev/null"; then
    fail "Proxy did not connect within 120s. Log:"
    run_cmd "tail -20 /tmp/stepflow-proxy.log" || true
    exit 1
fi
ok "Proxy connected and ready"

# ─────────────────────────────────────────────────────────────────────────────
# Submit a test flow
# ─────────────────────────────────────────────────────────────────────────────

step "Submitting test flow"

FLOW_FILE="$PROXY_DIR/tests/fixtures/double-flow.json"

info "Storing flow..."
FLOW_RESPONSE=$(run_cmd "curl -sf -X POST http://127.0.0.1:$ORCH_PORT/api/v1/flows \
    -H 'Content-Type: application/json' \
    -d @$FLOW_FILE")
FLOW_ID=$(echo "$FLOW_RESPONSE" | python3 -c "import sys,json; print(json.load(sys.stdin)['flowId'])" 2>/dev/null || echo "")

if [ -z "$FLOW_ID" ]; then
    fail "Failed to store flow: $FLOW_RESPONSE"
    exit 1
fi
ok "Flow stored: $FLOW_ID"

info "Submitting run with input {\"value\": 21}..."
TIME_START=$(date +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))")

# Write run request JSON (needs flow ID interpolated)
run_cmd "echo '{\"flowId\": \"$FLOW_ID\", \"input\": [{\"value\": 21}]}' > /tmp/stepflow-test-run.json"

RUN_RESPONSE=$(run_cmd "curl -sf -X POST http://127.0.0.1:$ORCH_PORT/api/v1/runs \
    -H 'Content-Type: application/json' \
    -d @/tmp/stepflow-test-run.json")

# Debug: check if orchestrator is still alive
if ! run_cmd "kill -0 \$(cat /tmp/stepflow-orch.pid 2>/dev/null) 2>/dev/null"; then
    fail "Orchestrator died during run submission. Last log:"
    run_cmd "tail -5 /tmp/stepflow-orch.log" || true
    exit 1
fi
RUN_ID=$(echo "$RUN_RESPONSE" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('runId', d.get('summary',{}).get('runId','')))" 2>/dev/null || echo "")

if [ -z "$RUN_ID" ]; then
    fail "Failed to submit run: $RUN_RESPONSE"
    exit 1
fi
ok "Run submitted: $RUN_ID"

# ─────────────────────────────────────────────────────────────────────────────
# Wait for result
# ─────────────────────────────────────────────────────────────────────────────

step "Waiting for result"

STATUS=""
for i in $(seq 1 120); do
    STATUS_RESPONSE=$(run_cmd "curl -sf http://127.0.0.1:$ORCH_PORT/api/v1/runs/$RUN_ID" 2>/dev/null || echo "")
    # Status can be string ("completed") or numeric (2=completed, 3=failed)
    STATUS=$(echo "$STATUS_RESPONSE" | python3 -c "
import sys,json
d=json.load(sys.stdin)
s=d.get('status', d.get('summary',{}).get('status',''))
# Map numeric to string
m={0:'pending',1:'running',2:'completed',3:'failed'}
print(m.get(s,s))
" 2>/dev/null || echo "")

    if [ "$STATUS" = "completed" ]; then
        TIME_END=$(date +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))")
        ELAPSED_MS=$(( (TIME_END - TIME_START) / 1000000 ))
        break
    elif [ "$STATUS" = "failed" ]; then
        fail "Run failed!"
        echo "$STATUS_RESPONSE" | python3 -m json.tool 2>/dev/null || echo "$STATUS_RESPONSE"
        exit 1
    fi
    sleep 1
done

if [ "$STATUS" != "completed" ]; then
    fail "Run did not complete within 120s (status: $STATUS)"
    exit 1
fi

ok "Run completed in ${ELAPSED_MS}ms"

# ─────────────────────────────────────────────────────────────────────────────
# Verify result
# ─────────────────────────────────────────────────────────────────────────────

step "Verifying result"

ITEMS_RESPONSE=$(run_cmd "curl -sf http://127.0.0.1:$ORCH_PORT/api/v1/runs/$RUN_ID/items" 2>/dev/null || echo "")
# Check result value (handle both int 42 and float 42.0)
RESULT_CHECK=$(echo "$ITEMS_RESPONSE" | python3 -c "
import sys, json
items = json.load(sys.stdin)['results']
val = items[0]['output']['result']
print('OK' if val == 42 else f'WRONG:{val}')
" 2>/dev/null || echo "PARSE_ERROR")

if [ "$RESULT_CHECK" = "OK" ]; then
    ok "Result correct: {result: 42}"
else
    fail "Expected result=42, got: $RESULT_CHECK"
    echo "Full response: $ITEMS_RESPONSE"
    exit 1
fi

# ─────────────────────────────────────────────────────────────────────────────
# Summary
# ─────────────────────────────────────────────────────────────────────────────

echo ""
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN}  ✅ Firecracker end-to-end test PASSED${NC}"
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo "  Flow:     double({value: 21}) → {result: 42}"
echo "  Latency:  ${ELAPSED_MS}ms (submit → completed)"
echo "  Platform: $OS/$ARCH"
echo ""

# Dump detailed timing from proxy log
PROXY_LOG=/tmp/stepflow-proxy.log
if run_cmd "test -f $PROXY_LOG"; then
    echo "  Detailed timing:"
    run_cmd "grep 'TIMING' $PROXY_LOG" 2>/dev/null || echo "  (no TIMING data in proxy log)"
    echo ""
fi
