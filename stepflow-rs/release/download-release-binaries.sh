#!/usr/bin/env bash
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

# Download binaries from a GitHub release
#
# This script downloads release binaries from GitHub and extracts them
# into the ./binaries/ directory, ready for Docker image building.
#
# Usage:
#   ./download-release-binaries.sh <version> [repo]
#
# Example:
#   ./download-release-binaries.sh 1.2.3
#   ./download-release-binaries.sh 1.2.3 datastax/stepflow

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
VERSION="${1:-}"
REPO="${2:-}"
BINARIES_DIR="./binaries"
TEMP_DIR=$(mktemp -d)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Cleanup on exit
trap "rm -rf $TEMP_DIR" EXIT

# Ensure we're in the release directory
cd "$SCRIPT_DIR"

if [ -z "$VERSION" ]; then
  echo -e "${RED}Error: Version required${NC}"
  echo "Usage: $0 <version> [repo]"
  echo "Example: $0 0.5.0"
  echo "Example: $0 0.5.0 myorg/myrepo"
  exit 1
fi

# Auto-detect repository from git remote if not provided
if [ -z "$REPO" ]; then
  # Try to get repo from git remote
  if git remote get-url origin &>/dev/null; then
    git_url=$(git remote get-url origin)
    # Extract owner/repo from various Git URL formats
    if [[ "$git_url" =~ github.com[:/]([^/]+/[^/]+)(\.git)?$ ]]; then
      REPO="${BASH_REMATCH[1]}"
      REPO="${REPO%.git}"  # Remove .git suffix if present
      echo -e "${BLUE}Auto-detected repository: $REPO${NC}"
    fi
  fi

  # Fallback to default if still not set
  if [ -z "$REPO" ]; then
    REPO="datastax/stepflow"
    echo -e "${YELLOW}Could not auto-detect repository, using default: $REPO${NC}"
  fi
fi

echo -e "${BLUE}Downloading binaries for version: $VERSION${NC}"
echo -e "${BLUE}Repository: $REPO${NC}"
echo ""

# Create binaries directory
mkdir -p "$BINARIES_DIR"

# GitHub release tag format
TAG="stepflow-rs-${VERSION}"
BASE_URL="https://github.com/${REPO}/releases/download/${TAG}"

# Targets to download (Linux only for Docker)
targets=(
  "x86_64-unknown-linux-gnu"
  "aarch64-unknown-linux-gnu"
  "x86_64-unknown-linux-musl"
  "aarch64-unknown-linux-musl"
)

echo -e "${BLUE}Downloading and extracting binaries...${NC}"
echo ""

for target in "${targets[@]}"; do
  archive="stepflow-${target}.tar.gz"
  url="${BASE_URL}/${archive}"

  echo -e "${GREEN}Downloading: $archive${NC}"

  if curl -fsSL "$url" -o "$TEMP_DIR/$archive"; then
    echo -e "${GREEN}✓ Downloaded: $archive${NC}"

    # Extract to temp directory
    tar -xzf "$TEMP_DIR/$archive" -C "$TEMP_DIR"

    # Move binaries to binaries directory with target suffix
    for binary in stepflow stepflow-server stepflow-load-balancer; do
      if [ -f "$TEMP_DIR/$binary" ]; then
        mv "$TEMP_DIR/$binary" "$BINARIES_DIR/${binary}-${target}"
        chmod +x "$BINARIES_DIR/${binary}-${target}"
        echo -e "  ${GREEN}→${NC} $binary → ${binary}-${target}"
      fi
    done

    echo ""
  else
    echo -e "${YELLOW}⊘ Failed to download: $archive${NC}"
    echo -e "${YELLOW}  URL: $url${NC}"
    echo ""
  fi
done

# List downloaded binaries
echo -e "${BLUE}=========================================${NC}"
echo -e "${BLUE}Downloaded binaries:${NC}"
echo -e "${BLUE}=========================================${NC}"

if [ -d "$BINARIES_DIR" ] && [ "$(ls -A $BINARIES_DIR 2>/dev/null)" ]; then
  ls -lh "$BINARIES_DIR" | tail -n +2 | while read -r line; do
    echo -e "${GREEN}✓${NC} $line"
  done
else
  echo -e "${YELLOW}No binaries downloaded${NC}"
  exit 1
fi

echo ""
echo -e "${GREEN}=========================================${NC}"
echo -e "${GREEN}Ready to build Docker images!${NC}"
echo -e "${GREEN}=========================================${NC}"
echo ""
echo -e "${BLUE}Next step:${NC}"
echo -e "  ${GREEN}./build-docker-images.sh $VERSION [registry]${NC}"
