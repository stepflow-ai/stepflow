# Kubernetes Batch Execution Demo

This demo showcases Stepflow's ability to distribute component execution across a Kubernetes cluster with a centralized orchestration runtime.

## Architecture

- **Stepflow Runtime**: Deployed in k8s with in-memory state store (single pod)
- **Component Servers**: Python HTTP servers deployed in k8s (3 replicas)
- **Load Balancer**: Pingora-based load balancer with SSE streaming support (2 replicas)
- **Local k3s Cluster**: Lightweight k8s running in Lima VM on macOS

## Key Features Demonstrated

- ✅ Distributed component execution across k8s pods
- ✅ Bidirectional communication with instance affinity routing
- ✅ SSE-aware load balancing via Pingora
- ✅ Parallel workflow execution with concurrent components
- ✅ Simple orchestration (single runtime, no distributed state complexity)
- ✅ Horizontal scaling of component servers

## Quick Start

See [TESTING.md](TESTING.md) for complete testing guide.

```bash
cd examples/kubernetes-batch-demo

# 1. Start Lima VM with k3s
./scripts/start-lima-k3s.sh

# 2. Configure kubectl
./scripts/setup-kubectl.sh
export KUBECONFIG=$(pwd)/kubeconfig

# 3. Deploy all services (one command)
./scripts/deploy-all.sh

# 4. Start port-forward to Stepflow server (in separate terminal)
./scripts/start-port-forward.sh stepflow

# 5. Run workflow tests
cd workflows
./test-workflows.sh
```

**Time:** ~5 minutes for first-time setup, ~60 seconds for testing

**Note:** The demo now uses released Docker images for `stepflow-server` and `stepflow-load-balancer`:
- `ghcr.io/stepflow-ai/stepflow/stepflow-server:alpine-0.6.0`
- `ghcr.io/stepflow-ai/stepflow/stepflow-load-balancer:alpine-0.6.0`

Only the component server requires building locally (Python HTTP component server).

**Expected output:**
- Test 1: 10 simple workflows complete successfully
- Test 2: 5 bidirectional workflows with blob storage
- Test 3: 5 parallel workflows (3 concurrent components each)
- Test 4: 20 batch workflows executed with max 5 concurrent
- Load distributed evenly across 3 component server pods
- Instance affinity maintained for bidirectional operations

## What's Different from production-model-serving?

This demo **simplifies** by focusing on distributed component execution:

- ❌ No PostgreSQL (uses SQLite)
- ❌ No distributed Stepflow runtime
- ❌ No StatefulSets for state storage
- ✅ Single Stepflow runtime on Mac
- ✅ Component servers scale in k8s
- ✅ Demonstrates batch execution patterns

## Directory Structure

```
kubernetes-batch-demo/
├── config/                    # Lima VM configurations
│   ├── lima-k3s-vz.yaml      # VZ backend (macOS 15+ M3+)
│   └── lima-k3s-qemu.yaml    # QEMU fallback
├── scripts/                   # Setup and deployment scripts
├── k8s/                      # Kubernetes manifests
│   ├── component-server/     # Component server deployment
│   └── load-balancer/        # Stepflow load balancer
├── docker/                   # Dockerfiles
├── workflows/                # Test workflows and configs
└── docs/                     # Documentation
```

## Use Cases

This architecture is ideal when you want to:

- Scale component execution independently from orchestration
- Run batch workloads that benefit from distributed compute
- Keep state management simple (single runtime, SQLite)
- Demonstrate k8s scaling without complex distributed systems

## Next Steps

- [Setup Guide](docs/SETUP.md) - Detailed setup instructions
- [Architecture](docs/ARCHITECTURE.md) - Architecture deep dive
- [Troubleshooting](docs/TROUBLESHOOTING.md) - Common issues
