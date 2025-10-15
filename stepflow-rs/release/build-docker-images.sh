#!/usr/bin/env bash
# Build Docker images for Stepflow
#
# This script builds Docker images from pre-built binaries.
# It expects binaries to be in the ./binaries/ directory with names like:
#   stepflow-x86_64-unknown-linux-gnu
#   stepflow-server-aarch64-unknown-linux-musl
#   etc.
#
# Usage:
#   ./build-docker-images.sh <version> [registry]
#
# Example:
#   ./build-docker-images.sh 1.2.3
#   ./build-docker-images.sh 1.2.3 ghcr.io/myorg/myrepo
#
# The script will:
# - Build all images for which binaries exist
# - Skip images where binaries are missing (with a warning)
# - Create multi-platform manifests
# - Optionally push to a registry

set -eo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
VERSION="${1:-}"
REGISTRY="${2:-}"
PUSH="${PUSH:-false}"
BINARIES_DIR="./binaries"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Ensure we're in the release directory
cd "$SCRIPT_DIR"

if [ -z "$VERSION" ]; then
  echo -e "${RED}Error: Version required${NC}"
  echo "Usage: $0 <version> [registry]"
  echo "Example: $0 1.2.3"
  exit 1
fi

# If registry is provided, enable push
if [ -n "$REGISTRY" ]; then
  PUSH=true
  echo -e "${BLUE}Registry: $REGISTRY${NC}"
  echo -e "${BLUE}Push enabled${NC}"
else
  REGISTRY="localhost"
  echo -e "${YELLOW}No registry specified, images will be tagged as 'localhost'${NC}"
  echo -e "${YELLOW}To push to a registry, provide it as second argument${NC}"
fi

echo -e "${BLUE}Version: $VERSION${NC}"

# Warn about local builds
if [ "$PUSH" = "false" ]; then
  echo ""
  echo -e "${YELLOW}Note: Local builds (--load) can only load images for your current architecture${NC}"
  echo -e "${YELLOW}Cross-platform images will be built but may fail to load${NC}"
  echo -e "${YELLOW}To build and push all platforms, specify a registry${NC}"
fi

echo ""

# Check if binaries directory exists
if [ ! -d "$BINARIES_DIR" ]; then
  echo -e "${RED}Error: Binaries directory not found: $BINARIES_DIR${NC}"
  echo "Please create it and add binaries, or download from a release"
  exit 1
fi

# Check for Docker and buildx
if ! command -v docker &> /dev/null; then
  echo -e "${RED}Error: Docker is not installed${NC}"
  exit 1
fi

if ! docker buildx version &> /dev/null; then
  echo -e "${RED}Error: Docker Buildx is not available${NC}"
  echo -e "${YELLOW}Docker Buildx is required for multi-platform builds${NC}"
  echo ""
  echo "To enable buildx:"
  echo "  1. Update Docker Desktop to the latest version, or"
  echo "  2. Enable experimental features in Docker"
  echo ""
  echo "For Docker Desktop:"
  echo "  - Open Docker Desktop preferences"
  echo "  - Enable 'Use Docker Compose V2'"
  echo "  - Buildx should be available automatically"
  echo ""
  echo "Or install manually: https://docs.docker.com/buildx/working-with-buildx/"
  exit 1
fi

# Create buildx builder if needed
if ! docker buildx inspect multiplatform &> /dev/null; then
  echo -e "${BLUE}Creating buildx builder 'multiplatform'...${NC}"
  docker buildx create --name multiplatform --use --platform linux/amd64,linux/arm64
  echo -e "${GREEN}✓ Builder created${NC}"
  echo ""
else
  # Use existing builder
  docker buildx use multiplatform &> /dev/null || true
fi

# Helper functions to map targets to platforms and bases
get_platform() {
  case "$1" in
    x86_64-unknown-linux-gnu|x86_64-unknown-linux-musl) echo "linux/amd64" ;;
    aarch64-unknown-linux-gnu|aarch64-unknown-linux-musl) echo "linux/arm64" ;;
    *) echo "unknown" ;;
  esac
}

get_base() {
  case "$1" in
    *-musl) echo "alpine" ;;
    *-gnu) echo "debian" ;;
    *) echo "unknown" ;;
  esac
}

# Images to build
images=("stepflow-server" "stepflow-load-balancer" "stepflow")
targets=("x86_64-unknown-linux-gnu" "aarch64-unknown-linux-gnu" "x86_64-unknown-linux-musl" "aarch64-unknown-linux-musl")

# Track what we built for manifest creation (using simple string tracking)
built_images_list=""

# Build each image type for each target
for image in "${images[@]}"; do
  echo -e "${BLUE}=========================================${NC}"
  echo -e "${BLUE}Building $image images...${NC}"
  echo -e "${BLUE}=========================================${NC}"
  echo ""

  for target in "${targets[@]}"; do
    platform=$(get_platform "$target")
    base=$(get_base "$target")
    binary_path="$BINARIES_DIR/${image}-${target}"

    # Check if binary exists
    if [ ! -f "$binary_path" ]; then
      echo -e "${YELLOW}⊘ Skipping $image:$base-$target (binary not found: $binary_path)${NC}"
      continue
    fi

    echo -e "${GREEN}Building $image:$base-$target for $platform${NC}"

    # Prepare temporary build context
    build_dir=$(mktemp -d)
    trap "rm -rf $build_dir" EXIT

    # Copy binary to build context
    cp "$binary_path" "$build_dir/$image"
    chmod +x "$build_dir/$image"

    # Build image
    image_tag="$REGISTRY/$image:${base}-${target}-${VERSION}"

    docker buildx build \
      --platform "$platform" \
      --file "Dockerfile.${image}.${base}" \
      --tag "$image_tag" \
      $([ "$PUSH" = "true" ] && echo "--push" || echo "--load") \
      "$build_dir"

    # Track this for manifest creation
    built_images_list="${built_images_list}${image}:${base}:${image_tag},"

    echo -e "${GREEN}✓ Built: $image_tag${NC}"
    echo ""

    # Clean up build dir
    rm -rf "$build_dir"
  done
done

echo ""

# Only create manifests if pushing to registry
if [ "$PUSH" = "true" ]; then
  echo -e "${BLUE}=========================================${NC}"
  echo -e "${BLUE}Creating multi-platform manifests...${NC}"
  echo -e "${BLUE}=========================================${NC}"
  echo ""

  # Create manifests for each image:base combination
  # Process the built images list to create manifests
  for image in "${images[@]}"; do
    for base in debian alpine; do
      # Find all tags for this image:base combination
      manifest_tags=""
      IFS=',' read -ra ENTRIES <<< "$built_images_list"
      for entry in "${ENTRIES[@]}"; do
        if [ -z "$entry" ]; then continue; fi
        IFS=':' read -r img b tag <<< "$entry"
        if [ "$img" = "$image" ] && [ "$b" = "$base" ]; then
          manifest_tags="$manifest_tags $tag"
        fi
      done

      # Skip if no images were built for this combination
      if [ -z "$manifest_tags" ]; then
        continue
      fi

      manifest_tag="$REGISTRY/$image:${base}-${VERSION}"
      echo -e "${GREEN}Creating manifest: $manifest_tag${NC}"

      # Create manifest (note: manifest_tags has leading space, which is fine)
      docker buildx imagetools create \
        --tag "$manifest_tag" \
        $manifest_tags

      echo -e "${GREEN}✓ Created manifest: $manifest_tag${NC}"

      # Also create major.minor tag if version is semver
      if [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        major_minor=$(echo "$VERSION" | cut -d. -f1,2)
        major_minor_tag="$REGISTRY/$image:${base}-${major_minor}"

        docker buildx imagetools create \
          --tag "$major_minor_tag" \
          $manifest_tags

        echo -e "${GREEN}✓ Created manifest: $major_minor_tag${NC}"
      fi

      echo ""
    done
  done
else
  echo -e "${YELLOW}=========================================${NC}"
  echo -e "${YELLOW}Skipping manifest creation (local build)${NC}"
  echo -e "${YELLOW}=========================================${NC}"
  echo -e "${YELLOW}Multi-platform manifests require pushing to a registry${NC}"
  echo ""
fi

echo ""
echo -e "${GREEN}=========================================${NC}"
echo -e "${GREEN}All images built successfully!${NC}"
echo -e "${GREEN}=========================================${NC}"
echo ""

# Summary
echo -e "${BLUE}Built images:${NC}"
for image in "${images[@]}"; do
  for base in debian alpine; do
    # Check if we built this combination
    found=false
    IFS=',' read -ra ENTRIES <<< "$built_images_list"
    for entry in "${ENTRIES[@]}"; do
      if [ -z "$entry" ]; then continue; fi
      IFS=':' read -r img b tag <<< "$entry"
      if [ "$img" = "$image" ] && [ "$b" = "$base" ]; then
        found=true
        break
      fi
    done

    if [ "$found" = true ]; then
      manifest_tag="$REGISTRY/$image:${base}-${VERSION}"
      echo -e "  ${GREEN}✓${NC} $manifest_tag"
    fi
  done
done

if [ "$PUSH" = "false" ]; then
  echo ""
  echo -e "${YELLOW}Images are local only (not pushed to registry)${NC}"
  echo -e "${YELLOW}To push to a registry, run with:${NC}"
  echo -e "  ${BLUE}$0 $VERSION <registry>${NC}"
  echo -e "  ${BLUE}# or${NC}"
  echo -e "  ${BLUE}PUSH=true $0 $VERSION${NC}"
fi
