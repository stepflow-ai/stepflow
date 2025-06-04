# StepFlow Container Infrastructure

This directory contains the containerization infrastructure for StepFlow, designed for minimal overhead using containerd and BuildKit directly.

## Overview

StepFlow is containerized into three main components:

1. **Core Runtime** (~10MB) - The main StepFlow engine built with Rust on Alpine Linux
2. **Python SDK** (~30MB) - Python component service using distroless base
3. **Inference** (GPU-optimized) - ML inference service with CUDA support

## Prerequisites Installation (macOS Apple Silicon)

This section provides complete installation instructions for all required tools on macOS Apple Silicon.

### 1. Install Core Development Tools

```bash
# Install Lima for Linux VM management
brew install lima

# Install kubectl for Kubernetes management
brew install kubectl

# Install kustomize for Kubernetes configuration management
brew install kustomize
```

### 2. Install Rust (if not already installed)

```bash
# Install rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add to path and reload shell
source ~/.cargo/env

# Verify installation
rustc --version
cargo --version
```

### 3. Optional: Install Additional Container Tools

```bash
# Install Docker Desktop (alternative to Lima, but heavier)
brew install --cask docker

# Install Helm for Kubernetes package management
brew install helm

# Install k9s for interactive Kubernetes management
brew install k9s
```

### 4. Verify Installation

```bash
# Check versions
lima --version
kubectl version --client
kustomize version
cargo --version

# Check architecture (should show arm64)
uname -m

# Test Lima installation
limactl --help

# Verify Rust targets
rustup target list --installed
```

### 5. Post-Installation Setup

```bash
# Add Rust target for Linux (needed for containers)
rustup target add aarch64-unknown-linux-musl

# Create Lima data directory (if not exists)
mkdir -p ~/.lima

# Set up shell completion (optional)
# For bash
lima completion bash > /usr/local/etc/bash_completion.d/lima
# For zsh
lima completion zsh > "${fpath[1]}/_lima"
```

## Quick Start

### Prerequisites Summary

After following the installation steps above, you should have:
- ✅ Lima for containerd VM management
- ✅ kubectl for Kubernetes operations
- ✅ kustomize for configuration management
- ✅ Rust toolchain for building StepFlow
- ✅ Standard Unix tools (curl, jq)

### Local Development Setup

1. **Initialize the development environment:**
   ```bash
   cd infrastructure/containers/scripts
   ./dev.sh setup
   ```

2. **Start the development environment:**
   ```bash
   ./dev.sh start
   ```

3. **Build components:**
   ```bash
   # Build all components
   ./build.sh core
   ./build.sh python-sdk
   ./build.sh inference

   # Build with specific options
   ./build.sh core --tag v1.0.0 --push
   ./build.sh python-sdk --dev --no-cache
   ```

4. **Run components locally:**
   ```bash
   ./dev.sh run core
   ./dev.sh run python-sdk
   ```

## Directory Structure

```
infrastructure/containers/
├── core/               # Core runtime container
│   └── Containerfile
├── python-sdk/         # Python SDK container
│   └── Containerfile
├── inference/          # GPU inference container
│   └── Containerfile
├── scripts/            # Build and development scripts
│   ├── build.sh       # BuildKit build script
│   └── dev.sh         # Local development helper
└── k8s/               # Kubernetes manifests
    ├── base/          # Base configurations
    └── overlays/      # Environment-specific configs
        ├── dev/       # Development environment
        └── prod/      # Production environment
```

## Building Images

### Using BuildKit

The `build.sh` script provides a unified interface for building all components:

```bash
# Basic build
./scripts/build.sh <component>

# Build with options
./scripts/build.sh core --tag v1.0.0 --platform linux/arm64 --push

# Development build with debug tools
./scripts/build.sh python-sdk --dev
```

### Build Arguments

- `--tag TAG` - Image tag (default: latest)
- `--no-cache` - Disable build cache
- `--push` - Push to registry after build
- `--platform PLATFORM` - Target platform (default: linux/amd64)
- `--dev` - Build development image with debugging tools
- `--registry REG` - Override registry (default: localhost:5000)

## Kubernetes Deployment

### Deploy to Development

```bash
# Apply development configuration
kubectl apply -k k8s/overlays/dev/

# Check deployment status
kubectl -n stepflow-dev get pods
```

### Deploy to Production

```bash
# Update image references in kustomization.yaml first
kubectl apply -k k8s/overlays/prod/

# Monitor deployment
kubectl -n stepflow rollout status deployment/stepflow-core
```

## Component Details

### Core Runtime (Port 8080)

- Minimal Alpine-based image
- Static Rust binary with musl
- Runs as non-root user (UID 1001)
- Health check endpoint: `/health`

### Python SDK (Port 8081)

- Distroless Python 3.11 base
- FastAPI/Uvicorn for HTTP server
- JSON-RPC protocol support
- Runs as non-root user (UID 65532)

### Inference Service (Port 8082)

- CUDA 12.1 with PyTorch
- GPU acceleration support
- Model caching with persistent volume
- Shared memory for tensor operations

## Security Features

1. **Non-root containers** - All containers run as non-privileged users
2. **Read-only root filesystem** - Prevents runtime modifications
3. **Network policies** - Restrict inter-component communication
4. **Security contexts** - Drop all capabilities, enforce security profiles
5. **Pod Security Standards** - Namespace enforces restricted policy

## Local Development Workflow

1. **Start Lima VM:**
   ```bash
   ./scripts/dev.sh start
   ```

2. **Build and test locally:**
   ```bash
   # Build component
   ./scripts/build.sh core --dev

   # Run tests
   ./scripts/dev.sh test

   # Run component
   ./scripts/dev.sh run core
   ```

3. **Deploy to local K3s:**
   ```bash
   kubectl apply -k k8s/overlays/dev/
   ```

## Production Deployment

1. **Build production images:**
   ```bash
   ./scripts/build.sh core --tag v1.0.0 --registry your-registry.io --push
   ./scripts/build.sh python-sdk --tag v1.0.0 --registry your-registry.io --push
   ./scripts/build.sh inference --tag v1.0.0 --registry your-registry.io --push
   ```

2. **Update kustomization.yaml with new tags**

3. **Apply production configuration:**
   ```bash
   kubectl apply -k k8s/overlays/prod/
   ```

## Monitoring and Debugging

### View logs:
```bash
# Core logs
kubectl -n stepflow logs -l component=core -f

# Python SDK logs
kubectl -n stepflow logs -l component=python-sdk -f
```

### Access shell (development only):
```bash
# Core container
kubectl -n stepflow-dev exec -it deploy/dev-stepflow-core -- sh

# Python SDK
kubectl -n stepflow-dev exec -it deploy/dev-stepflow-python-sdk -- bash
```

### Health checks:
```bash
# Check core health
curl http://localhost:8080/health

# Check Python SDK health
curl http://localhost:8081/health

# Check inference health
curl http://localhost:8082/health
```

## Troubleshooting

### macOS Apple Silicon Specific Issues

#### Lima VM Architecture
If you encounter architecture mismatches:
```bash
# Check Lima VM architecture
limactl shell containerd-dev uname -m

# Should show aarch64 for Apple Silicon
# If showing x86_64, recreate VM with correct arch
limactl delete containerd-dev
./scripts/dev.sh setup
```

#### Homebrew Installation Issues
```bash
# If brew command not found after installation
echo 'eval "$(/opt/homebrew/bin/brew shellenv)"' >> ~/.zshrc
source ~/.zshrc

# Verify Homebrew is using Apple Silicon binaries
brew config | grep CPU
```

#### Rust Cross-compilation
```bash
# Add ARM64 Linux target for cross-compilation
rustup target add aarch64-unknown-linux-musl

# Add x86_64 Linux target if needed
rustup target add x86_64-unknown-linux-musl
```

#### Lima VM Memory Issues
If VM runs out of memory:
```bash
# Stop VM and adjust memory in config
limactl stop containerd-dev
# Edit ~/.lima/containerd-dev/lima.yaml and increase memory
limactl start containerd-dev
```

### BuildKit Issues

If BuildKit is not running:
```bash
limactl shell containerd-dev sudo systemctl restart buildkit

# Check BuildKit status
limactl shell containerd-dev sudo systemctl status buildkit
```

### Registry Connection

Ensure local registry is running:
```bash
./scripts/dev.sh registry

# Test registry connectivity
curl http://localhost:5000/v2/_catalog
```

### Networking Issues

If services can't reach each other:
```bash
# Check Lima port forwarding
limactl list

# Restart Lima VM to refresh port forwards
limactl stop containerd-dev
limactl start containerd-dev
```

### GPU Support

For inference component, ensure:
- NVIDIA GPU drivers installed on nodes
- NVIDIA device plugin deployed to cluster
- RuntimeClass `nvidia` configured

**Note:** Apple Silicon Macs don't have NVIDIA GPUs. For GPU testing:
- Use cloud instances with NVIDIA GPUs
- Disable GPU requirements in development overlay
- Use CPU-only inference for local testing

## Notes

- The `serve` command is not yet implemented in StepFlow core
- HTTP endpoints for health checks need to be implemented
- Adjust resource limits based on actual workload requirements
- GPU nodes require proper labeling: `accelerator=nvidia-gpu`