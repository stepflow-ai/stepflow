# stepflow-isolation-proxy

Dispatches Stepflow tasks to sandboxed workers via vsock/Unix socket. Workers handle the full task lifecycle (claim, heartbeat, execute, complete) by talking directly to the orchestrator via gRPC.

## Backends

### Subprocess

Spawns a Python worker per task, communicates via Unix socket. Works on all platforms.

```bash
stepflow-isolation-proxy \
  --tasks-url http://127.0.0.1:7837 \
  subprocess
```

### Firecracker

Boots Firecracker microVMs via the jailer, dispatches tasks via vsock. Requires Linux with KVM.

```bash
stepflow-isolation-proxy \
  --tasks-url http://127.0.0.1:7837 \
  firecracker \
  --kernel ./build/vmlinux.bin \
  --rootfs ./build/stepflow-rootfs.ext4 \
  --pool-size 2
```

## Development

### Prerequisites

- Rust toolchain
- `protoc` (for stepflow-proto)
- Python 3.10+ with `uv` (for the vsock worker)

### macOS (Lima)

Firecracker requires Linux with KVM. On macOS, use Lima with VZ backend (M3+ chip, macOS 15+):

```bash
cd stepflow-rs/crates/stepflow-isolation-proxy

# Start Lima VM (installs Firecracker, Rust, uv)
./scripts/start-lima.sh

# Inside Lima: build rootfs
./scripts/build-rootfs.sh

# Run the end-to-end test
./scripts/test-firecracker.sh
```

### Linux (native or CI)

```bash
# Install Firecracker
sudo ./scripts/setup-firecracker-ci.sh

# Build rootfs
sudo ./scripts/build-rootfs.sh

# Run the test
./scripts/test-firecracker.sh
```

### Running tests

```bash
# Subprocess backend integration test (no VM needed)
cd stepflow-rs
cargo build -p stepflow-server --no-default-features
STEPFLOW_DEV_BINARY=$(pwd)/target/debug/stepflow-server \
  cargo test -p stepflow-isolation-proxy --test integration -- --include-ignored

# Firecracker backend end-to-end test (requires KVM)
cd crates/stepflow-isolation-proxy
./scripts/test-firecracker.sh
```

## Architecture

```
Orchestrator ──[PullTasks stream]──▶ Proxy ──[vsock/socket]──▶ Worker
                                                                  │
                                                        [heartbeat, complete,
                                                         blobs, nested runs]
                                                                  │
                                                                  ▼
                                                           Orchestrator (direct gRPC)
```

The proxy is a thin dispatcher. It does not handle heartbeats, completion, or results. The worker inside the sandbox owns the full task lifecycle.

### Firecracker VM lifecycle

1. **Snapshot creation** (one-time at startup): Boot VM → wait for vsock worker ready → pause → snapshot
2. **VM pool**: Pre-warm VMs by restoring from snapshot (~700ms per VM)
3. **Task dispatch**: Take pre-warmed VM → CONNECT vsock → send task envelope → wait for worker EOF
4. **Cleanup**: Kill VM → remove jailer chroot → delete namespace

## Benchmarking

The test script outputs detailed timing for each phase. To reproduce:

### Subprocess baseline

```bash
cd stepflow-rs
cargo build -p stepflow-server --no-default-features
STEPFLOW_DEV_BINARY=$(pwd)/target/debug/stepflow-server \
  cargo test -p stepflow-isolation-proxy --test integration -- --include-ignored --nocapture 2>&1 \
  | grep "test_dev_mode"
```

### Firecracker (requires Linux with KVM)

```bash
cd stepflow-rs/crates/stepflow-isolation-proxy

# First run (builds rootfs, creates snapshot — takes ~2 min):
./scripts/test-firecracker.sh

# Subsequent runs (uses cached rootfs — faster):
./scripts/test-firecracker.sh
```

Look for `TIMING:` lines in the output. Key metrics:

| Metric | What it measures |
|--------|-----------------|
| `pool_take` | Time to get a pre-warmed VM (should be ~µs) |
| `vsock_connect` | Vsock CONNECT handshake to guest worker |
| `channel_open` | Python grpcio channel creation (includes demand paging) |
| `heartbeat_claim` | First gRPC RPC to orchestrator (TCP + HTTP/2 setup) |
| `execute` | Component execution time |
| `complete_task` | gRPC CompleteTask RPC |
| `e2e_proxy` | Total time from task received to worker done |
| Overall `Latency` | Submit to orchestrator reports complete |

### Reference numbers

| Environment | Subprocess (submit→complete) | Firecracker e2e_proxy | Firecracker handle_task | Notes |
|-------------|-----------|-----------|-----------|-------|
| macOS M5 (Lima/nested KVM) | ~100ms | ~1.6s | ~1.2s | Demand paging through virtio-fs |
| GitHub CI (ubuntu-latest) | ~100ms | ~250ms | ~140ms | Native KVM + NVMe |

The test worker includes a simulated heavy dependency load (~20MB of data structures) to
demonstrate the snapshot advantage: in the subprocess case this load happens on every task,
while in the Firecracker case it's captured in the snapshot and costs 0ms on restore.

Please share your numbers by running the test and reporting the `TIMING:` output along with your platform details.
