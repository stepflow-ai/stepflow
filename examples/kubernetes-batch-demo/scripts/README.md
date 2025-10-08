# Scripts Directory

This directory contains scripts for deploying and testing the Kubernetes batch demo.

## Setup Scripts

### `start-lima-k3s.sh`
Starts Lima VM with k3s cluster. Auto-detects best backend (VZ or QEMU).

**Usage:**
```bash
./start-lima-k3s.sh
```

### `setup-kubectl.sh`
Configures kubectl to access k3s cluster from Mac.

**Usage:**
```bash
./setup-kubectl.sh
export KUBECONFIG=$(pwd)/../kubeconfig
```

## Build Scripts

### `build-component-server.sh`
Builds and pushes component server Docker image to local registry.

**Usage:**
```bash
./build-component-server.sh
```

### `build-load-balancer.sh`
Builds and pushes Load balancer load balancer Docker image.

**Usage:**
```bash
./build-load-balancer.sh
```

### `build-stepflow-server.sh`
Builds and pushes Stepflow runtime server Docker image.

**Usage:**
```bash
./build-stepflow-server.sh
```

## Deployment Scripts

### `deploy-all.sh` ⭐ **Recommended**
Complete deployment automation. Builds all images and deploys all services.

**Usage:**
```bash
./deploy-all.sh
```

This script:
1. Builds all Docker images (component server, Load balancer, Stepflow runtime)
2. Creates stepflow-demo namespace
3. Deploys all services
4. Waits for pods to be ready
5. Shows deployment status

### `deploy-k8s.sh`
Deploys component servers and Load balancer (legacy script, use `deploy-all.sh` instead).

**Usage:**
```bash
./deploy-k8s.sh
```

## Port Forwarding Script

### `start-port-forward.sh`
Unified port-forward script supporting multiple services.

**Usage:**
```bash
# Forward to Stepflow server (default, primary workflow)
./start-port-forward.sh stepflow

# Forward to Load balancer load balancer (debugging only)
./start-port-forward.sh load-balancer

# Show help
./start-port-forward.sh
```

**Services:**
- **stepflow** - Stepflow server on localhost:7840 ⭐ **Primary workflow**
  - Use for submitting workflows via `cargo run -- submit`
  - Use for running test scripts that use Stepflow API
  - Required for normal workflow testing

- **load-balancer** - Load balancer load balancer on localhost:8080 🔧 **Debugging only**
  - Use for direct component server access
  - Use for low-level debugging of component execution
  - Not needed for normal workflow testing

## Testing Scripts

All testing scripts are in `../workflows/` directory:

### Test Script
- `test-workflows.sh` - Unified test suite covering all workflow types (20 workflows total)
  - 10 simple workflows (load distribution)
  - 5 bidirectional workflows (blob storage + instance affinity)
  - 5 parallel workflows (concurrent execution: 5 × 3 = 15 components)

## Quick Start Workflow

```bash
# 1. Start k3s cluster
./start-lima-k3s.sh

# 2. Configure kubectl
./setup-kubectl.sh
export KUBECONFIG=$(pwd)/../kubeconfig

# 3. Deploy everything
./deploy-all.sh

# 4. Start port-forward (in separate terminal)
./start-port-forward.sh stepflow

# 5. Run tests
cd ../workflows
./test-workflows.sh
```

## Script Dependencies

```
start-lima-k3s.sh
  └── setup-kubectl.sh
        └── build-component-server.sh
        └── build-load-balancer.sh
        └── build-stepflow-server.sh
              └── deploy-all.sh
                    └── start-port-forward.sh stepflow
                          └── test-workflows.sh
```

## Environment Variables

### `LIMA_INSTANCE`
Override Lima instance name (default: `stepflow-k3s`)

**Example:**
```bash
LIMA_INSTANCE=my-k3s ./start-lima-k3s.sh
```

### `KUBECONFIG`
Path to kubeconfig file (set by `setup-kubectl.sh`)

**Example:**
```bash
export KUBECONFIG=/path/to/kubeconfig
```
