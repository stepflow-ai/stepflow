# Kubernetes Batch Demo Setup Guide

This guide walks you through setting up the Kubernetes batch execution demo on macOS.

## Prerequisites

### Required Software

1. **Homebrew** - Package manager for macOS
   ```bash
   /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
   ```

2. **Lima** - Lightweight Linux VMs on macOS
   ```bash
   brew install lima
   ```

3. **kubectl** - Kubernetes command-line tool
   ```bash
   brew install kubectl
   ```

4. **Optional: socket_vmnet** - For QEMU networking (if not using VZ)
   ```bash
   brew install socket_vmnet
   ```

### System Requirements

- **macOS Version**: 13.0+ (macOS 15+ recommended for VZ support)
- **Chip**: Apple Silicon (M1, M2, M3, M4) or Intel
- **RAM**: 16GB+ recommended (8GB minimum)
- **Disk Space**: 50GB free space

### VZ vs QEMU Backend

The setup script automatically detects the best backend:

- **VZ** (recommended): macOS 15+ with M3+ chip
  - Faster performance
  - Better resource efficiency
  - Native virtualization support

- **QEMU** (fallback): Other systems
  - Compatible with all Macs
  - Slightly slower but fully functional
  - Works with M1/M2 and Intel chips

## Setup Steps

### Step 1: Start Lima VM with k3s

Navigate to the demo directory:
```bash
cd examples/kubernetes-batch-demo
```

Start the Lima VM (automatically selects VZ or QEMU):
```bash
./scripts/start-lima-k3s.sh
```

**What this does**:
- Creates a Lima VM with Ubuntu 24.04
- Installs k3s (lightweight Kubernetes)
- Sets up local Docker registry at localhost:5000
- Mounts your stepflow project at `/home/lima.linux/stepflow`
- Configures port forwarding:
  - 6443 → k3s API server (required for kubectl access)
  - 8080 → HTTP services (Load Balancer load balancer, Stepflow server, component servers)
  - 5000 → Local Docker registry (required for pushing images from Mac to k3s)

**Expected output**:
```
[INFO] 🔍 Detecting optimal Lima backend for k3s...
[INFO] Detected macOS 15.0 with M3 Pro chip
[INFO] ✅ VZ supported: macOS 15.0 + M3 Pro chip
[INFO] 🚀 Using VZ backend for optimal performance
...
[INFO] ✅ k3s cluster ready!
```

**Time**: First run takes 5-10 minutes (downloading Ubuntu image + k3s installation)

### Step 2: Configure kubectl on Mac

Configure kubectl to access the k3s cluster:
```bash
./scripts/setup-kubectl.sh
```

**What this does**:
- Copies kubeconfig from Lima VM
- Configures it for Mac access via port forwarding
- Creates `kubeconfig` file in the demo directory

**Set environment variable**:
```bash
export KUBECONFIG=$(pwd)/kubeconfig
```

Add to your shell profile for persistence:
```bash
# For zsh (default on modern macOS)
echo "export KUBECONFIG=$(pwd)/kubeconfig" >> ~/.zshrc

# For bash
echo "export KUBECONFIG=$(pwd)/kubeconfig" >> ~/.bashrc
```

### Step 3: Verify k3s Cluster

Test the connection:
```bash
kubectl get nodes
```

**Expected output**:
```
NAME           STATUS   ROLES                  AGE   VERSION
lima-stepflow  Ready    control-plane,master   2m    v1.28.3+k3s1
```

Check cluster info:
```bash
kubectl cluster-info
```

## Next Steps

Now that your k3s cluster is running, proceed to:

1. **Deploy Component Servers** - See Phase 2 in main README
2. **Configure Stepflow** - See Phase 3 in main README
3. **Run Batch Tests** - See Phase 4 in main README

## Common Operations

### Connect to Lima VM

Open a shell in the VM:
```bash
limactl shell --workdir /home/lima.linux/stepflow stepflow-k3s
```

### Stop the VM

Stop the Lima VM (preserves state):
```bash
limactl stop stepflow-k3s
```

### Start Stopped VM

Restart a stopped VM:
```bash
limactl start stepflow-k3s
```

### Delete and Recreate VM

Complete reset:
```bash
limactl stop stepflow-k3s
limactl delete stepflow-k3s
./scripts/start-lima-k3s.sh
```

### View VM Status

List all Lima instances:
```bash
limactl list
```

### Access k3s Logs

View k3s service logs:
```bash
limactl shell stepflow-k3s sudo journalctl -u k3s -f
```

## Troubleshooting

### Port 6443 Already in Use

If you have another Kubernetes cluster running:
```bash
# Find process using port 6443
lsof -i :6443

# Stop conflicting service (e.g., Docker Desktop's Kubernetes)
# Or use a different port in lima-k3s-*.yaml
```

### kubectl Can't Connect

1. Verify Lima VM is running:
   ```bash
   limactl list
   ```

2. Verify port forwarding:
   ```bash
   curl -k https://localhost:6443
   # Should return "Client sent an HTTP request to an HTTPS server"
   ```

3. Re-run kubectl setup:
   ```bash
   ./scripts/setup-kubectl.sh
   export KUBECONFIG=$(pwd)/kubeconfig
   ```

### k3s Not Starting

Check k3s status in VM:
```bash
limactl shell stepflow-k3s sudo systemctl status k3s
```

View detailed logs:
```bash
limactl shell stepflow-k3s sudo journalctl -u k3s -n 100
```

### Lima VM Won't Start

1. Check Lima version:
   ```bash
   limactl --version
   # Update if < 1.0.0
   brew upgrade lima
   ```

2. Check for conflicting VMs:
   ```bash
   limactl list
   # Stop/delete old instances
   ```

3. Try QEMU instead of VZ:
   ```bash
   limactl delete stepflow-k3s
   # Edit scripts/start-lima-k3s.sh to force QEMU
   ```

### Out of Disk Space

Increase disk size in config files:
```yaml
# Edit config/lima-k3s-vz.yaml and config/lima-k3s-qemu.yaml
disk: "100GiB"  # Increase this value
```

Then recreate VM:
```bash
limactl delete stepflow-k3s
./scripts/start-lima-k3s.sh
```

## Performance Tips

### VZ Backend Performance

For best performance with VZ:
- Use macOS 15.0+ with M3+ chip
- Allocate sufficient resources (4+ CPUs, 8GB+ RAM)
- Enable Rosetta 2 for x86 emulation if needed

### QEMU Backend Performance

For better QEMU performance:
- Install socket_vmnet: `brew install socket_vmnet`
- Use ARM64 images on Apple Silicon
- Increase CPU/memory allocation if needed

### Resource Allocation

Edit CPU/memory in config files before first start:
```yaml
# config/lima-k3s-vz.yaml
cpus: 6        # Increase for more parallelism
memory: "12GiB"  # Increase for larger workloads
```

## Advanced Configuration

### Custom Instance Name

Use a custom instance name:
```bash
LIMA_INSTANCE=my-k3s ./scripts/start-lima-k3s.sh
```

### Multiple k3s Clusters

Run multiple isolated clusters:
```bash
LIMA_INSTANCE=k3s-dev ./scripts/start-lima-k3s.sh
LIMA_INSTANCE=k3s-staging ./scripts/start-lima-k3s.sh
```

Switch between them:
```bash
./scripts/setup-kubectl.sh
export KUBECONFIG=$(pwd)/kubeconfig
```

### Access from Other Machines

To access k3s from other machines on your network:

1. Get Lima VM IP:
   ```bash
   limactl shell stepflow-k3s ip addr show
   ```

2. Update kubeconfig server address
3. Configure firewall to allow access to port 6443

## Security Notes

### Kubeconfig Permissions

The kubeconfig file contains cluster admin credentials:
```bash
chmod 600 kubeconfig  # Already set by setup script
```

### Local Development Only

This setup is for **local development only**:
- No authentication configured
- No TLS certificate validation
- Admin access by default

**Do not use in production or expose to untrusted networks.**

## Next Steps

Once k3s is running and kubectl is configured:

### Quick Deployment

Use the all-in-one deployment script:
```bash
cd examples/kubernetes-batch-demo
./scripts/deploy-all.sh
```

This script will:
1. Build all Docker images (component server, Load Balancer, Stepflow runtime)
2. Create the stepflow-demo namespace
3. Deploy all services to k8s
4. Wait for pods to be ready

### Manual Deployment

If you prefer to deploy step-by-step:

```bash
# Build images
./scripts/build-component-server.sh  # Component server
./scripts/build-load-balancer.sh           # Load Balancer load balancer
./scripts/build-stepflow-server.sh   # Stepflow runtime

# Deploy services
kubectl apply -f k8s/namespace.yaml
kubectl apply -k k8s/component-server/
kubectl apply -k k8s/stepflow-load-balancer/
kubectl apply -k k8s/stepflow-server/

# Wait for readiness
kubectl wait --for=condition=Ready pods -l app=component-server -n stepflow-demo --timeout=120s
kubectl wait --for=condition=Ready pods -l app=stepflow-load-balancer -n stepflow-demo --timeout=120s
kubectl wait --for=condition=Ready pods -l app=stepflow-server -n stepflow-demo --timeout=120s
```

### Testing

See the [Testing Guide](../TESTING.md) for complete testing instructions.

Quick test:
```bash
# Start port-forward to Stepflow server (in separate terminal)
./scripts/start-port-forward.sh stepflow

# Run workflow tests
./workflows/test-workflows.sh
```

### Additional Resources

- [Architecture Documentation](../ARCHITECTURE.md) - Understand the system design
- [Testing Guide](../TESTING.md) - Complete testing instructions
- [Main README](../README.md) - Overview and quick start
