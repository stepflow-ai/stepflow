#!/bin/bash
set -euo pipefail

# StepFlow Container Build Script using BuildKit
# Usage: ./build.sh <component> [options]

# Configuration
REGISTRY="${REGISTRY:-localhost:5000}"
PROJECT="stepflow"
BUILD_CONTEXT="${BUILD_CONTEXT:-../../..}"
BUILDKIT_HOST="${BUILDKIT_HOST:-unix:///run/buildkit/buildkitd.sock}"

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Functions
log() {
    echo -e "${GREEN}[BUILD]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1" >&2
    exit 1
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

usage() {
    cat << EOF
Usage: $0 <component> [options]

Components:
  core          Build the core StepFlow runtime
  python-sdk    Build the Python SDK component
  inference     Build the GPU-optimized inference component

Options:
  --tag TAG           Override image tag (default: latest)
  --no-cache         Disable build cache
  --push             Push image to registry after build
  --platform PLATFORM Build for specific platform (default: linux/amd64)
  --dev              Build development image with debugging tools
  --registry REG     Override registry (default: localhost:5000)

Examples:
  $0 core --tag v1.0.0 --push
  $0 python-sdk --dev
  $0 inference --platform linux/arm64

EOF
}

# Parse arguments
if [ $# -lt 1 ]; then
    usage
    exit 1
fi

COMPONENT=$1
shift

# Default values
TAG="latest"
CACHE_OPT=""
PUSH=false
PLATFORM="linux/amd64"
DEV_BUILD=false
BUILD_ARGS=""

# Parse options
while [[ $# -gt 0 ]]; do
    case $1 in
        --tag)
            TAG="$2"
            shift 2
            ;;
        --no-cache)
            CACHE_OPT="--no-cache"
            shift
            ;;
        --push)
            PUSH=true
            shift
            ;;
        --platform)
            PLATFORM="$2"
            shift 2
            ;;
        --dev)
            DEV_BUILD=true
            TAG="${TAG}-dev"
            shift
            ;;
        --registry)
            REGISTRY="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            error "Unknown option: $1"
            ;;
    esac
done

# Validate component
case $COMPONENT in
    core|python-sdk|inference)
        ;;
    *)
        error "Invalid component: $COMPONENT. Use 'core', 'python-sdk', or 'inference'"
        ;;
esac

# Set component-specific variables
CONTAINERFILE="infrastructure/containers/${COMPONENT}/Containerfile"
IMAGE_NAME="${REGISTRY}/${PROJECT}/${COMPONENT}:${TAG}"

# Add development build args if needed
if [ "$DEV_BUILD" = true ]; then
    case $COMPONENT in
        core)
            BUILD_ARGS="--build-arg FEATURES=debug"
            ;;
        python-sdk)
            BUILD_ARGS="--build-arg DEV_DEPS=true"
            ;;
        inference)
            BUILD_ARGS="--build-arg CUDA_DEBUG=1"
            ;;
    esac
fi

# Check if Containerfile exists
if [ ! -f "${BUILD_CONTEXT}/${CONTAINERFILE}" ]; then
    error "Containerfile not found: ${BUILD_CONTEXT}/${CONTAINERFILE}"
fi

# Build command
BUILD_CMD="buildctl build \
    --frontend dockerfile.v0 \
    --local context=${BUILD_CONTEXT} \
    --local dockerfile=${BUILD_CONTEXT}/$(dirname ${CONTAINERFILE}) \
    --opt filename=$(basename ${CONTAINERFILE}) \
    --opt platform=${PLATFORM} \
    --output type=image,name=${IMAGE_NAME},push=${PUSH} \
    ${CACHE_OPT} \
    ${BUILD_ARGS}"

# Execute build
log "Building ${COMPONENT} component..."
log "Image: ${IMAGE_NAME}"
log "Platform: ${PLATFORM}"
log "Context: ${BUILD_CONTEXT}"

if [ "$DEV_BUILD" = true ]; then
    warn "Building development image with debugging tools"
fi

# Check BuildKit daemon
if ! buildctl debug workers 2>/dev/null; then
    error "BuildKit daemon not running. Please start buildkitd or check BUILDKIT_HOST"
fi

# Run the build
if eval "${BUILD_CMD}"; then
    log "Build completed successfully!"
    
    if [ "$PUSH" = true ]; then
        log "Image pushed to ${IMAGE_NAME}"
    else
        log "Image built locally. Use --push to upload to registry"
    fi
    
    # Print next steps
    echo
    log "Next steps:"
    echo "  - Test locally: nerdctl run --rm ${IMAGE_NAME}"
    echo "  - Deploy to K8s: kubectl apply -f infrastructure/containers/k8s/"
else
    error "Build failed!"
fi