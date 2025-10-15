# Release Docker Images

This directory contains Dockerfiles and configuration for building release container images. These are optimized for production use with minimal build context and small image sizes.

**Location:** `stepflow-rs/release/`

Three distinct images are available, each optimized for a specific use case.

## Available Images

### 1. stepflow-server
HTTP workflow execution server for running workflows via REST API.

**Images:**
- `Dockerfile.stepflow-server.debian` - Debian-based (bookworm-slim)
- `Dockerfile.stepflow-server.alpine` - Alpine-based (3.19)

**Configuration:**
- **Port:** 7840 (default)
- **Listens on:** 0.0.0.0 (all interfaces)
- **Entry point:** `stepflow-server --port 7840`

**Usage:**
```bash
docker run -p 7840:7840 ghcr.io/datastax/stepflow/stepflow-server:debian-X.Y.Z
```

### 2. stepflow-load-balancer
Load balancer for distributed component servers using Pingora.

**Images:**
- `Dockerfile.stepflow-load-balancer.debian` - Debian-based (bookworm-slim)
- `Dockerfile.stepflow-load-balancer.alpine` - Alpine-based (3.19)

**Configuration:**
- **Port:** 8080 (default)
- **Environment:** `UPSTREAM_SERVICE` (default: `component-server.stepflow-demo.svc.cluster.local:8080`)

**Usage:**
```bash
docker run -p 8080:8080 \
  -e UPSTREAM_SERVICE=my-components.example.com:8080 \
  ghcr.io/datastax/stepflow/stepflow-load-balancer:alpine-X.Y.Z
```

### 3. stepflow (CLI)
Command-line interface for workflow execution and management.

**Images:**
- `Dockerfile.stepflow.debian` - Debian-based (bookworm-slim)
- `Dockerfile.stepflow.alpine` - Alpine-based (3.19)

**Usage:**
```bash
docker run -v $(pwd):/workspace \
  ghcr.io/datastax/stepflow/stepflow:debian-X.Y.Z \
  run --flow /workspace/workflow.yaml --input /workspace/input.json
```

## Building Locally

These Dockerfiles are used by the GitHub Actions release workflow, but can also be built locally.

### Prerequisites

1. Build the binary for the target platform:
```bash
cd stepflow-rs

# For native platform
cargo build --release --bin stepflow-server

# For Linux (from macOS using cross)
cross build --release --target x86_64-unknown-linux-gnu --bin stepflow-server
```

2. Copy binary to release directory:
```bash
cp target/release/stepflow-server release/stepflow-server
# or
cp target/x86_64-unknown-linux-gnu/release/stepflow-server release/stepflow-server
```

### Building Images

From the `stepflow-rs/release/` directory:

```bash
cd release

# Build stepflow-server (Debian)
docker build -f Dockerfile.stepflow-server.debian -t stepflow-server:local .

# Build stepflow-load-balancer (Alpine)
docker build -f Dockerfile.stepflow-load-balancer.alpine -t stepflow-load-balancer:local .

# Build stepflow CLI (Debian)
docker build -f Dockerfile.stepflow.debian -t stepflow:local .
```

**Note:** The `.dockerignore` file ensures only binaries are included in the build context, keeping builds fast and secure.

## Architecture Support

All images are built for multiple architectures:
- **linux/amd64** (x86_64)
- **linux/arm64** (aarch64)

Docker will automatically pull the correct image for your platform.

## Base Image Selection

**Debian (bookworm-slim):**
- Larger image size (~100MB base)
- Better compatibility with some libraries
- Recommended for most use cases

**Alpine (3.19):**
- Smaller image size (~50MB base)
- Uses musl libc instead of glibc
- Recommended for size-constrained environments

## Directory Structure

```
stepflow-rs/release/
├── .dockerignore              # Minimal build context (binaries only)
├── README.md                  # This file
├── Dockerfile.stepflow-server.debian
├── Dockerfile.stepflow-server.alpine
├── Dockerfile.stepflow-load-balancer.debian
├── Dockerfile.stepflow-load-balancer.alpine
├── Dockerfile.stepflow.debian
└── Dockerfile.stepflow.alpine
```

## Release Process

These Dockerfiles are used by `.github/workflows/release_rust.yml` during the release process:

1. Binaries are built for multiple targets (x86_64, aarch64, glibc, musl)
2. For each image type and base:
   - Binary is downloaded from artifacts to `stepflow-rs/release/`
   - Renamed to match Dockerfile expectations (e.g., `stepflow-server-x86_64-unknown-linux-gnu` → `stepflow-server`)
   - Docker image is built using the appropriate Dockerfile from this directory
   - Image is pushed to GitHub Container Registry by digest
3. Multi-platform manifests are created combining amd64 and arm64 images

The `.dockerignore` ensures minimal build context - only the required binary is included, not the entire source tree.

## Registry

Published images are available at:
- `ghcr.io/datastax/stepflow/stepflow-server:debian-X.Y.Z`
- `ghcr.io/datastax/stepflow/stepflow-server:alpine-X.Y.Z`
- `ghcr.io/datastax/stepflow/stepflow-load-balancer:debian-X.Y.Z`
- `ghcr.io/datastax/stepflow/stepflow-load-balancer:alpine-X.Y.Z`
- `ghcr.io/datastax/stepflow/stepflow:debian-X.Y.Z`
- `ghcr.io/datastax/stepflow/stepflow:alpine-X.Y.Z`

## Development vs Release Dockerfiles

This `release/` directory contains Dockerfiles optimized for **production releases**:
- Minimal build context (binaries only via `.dockerignore`)
- Pre-built binaries copied into images
- Used by CI/CD release pipeline
- Small, secure images

For **local development** Dockerfiles that build from source, you can create additional Dockerfiles in the `stepflow-rs/` root directory (e.g., `Dockerfile.dev`). These can use multi-stage builds to compile the binary within Docker, useful for development environments where you don't want to set up the Rust toolchain locally.
