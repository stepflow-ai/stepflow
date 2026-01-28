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

set -euo pipefail

# Build platform-specific wheels for stepflow-orchestrator
#
# This script takes pre-built binaries and creates Python wheels with the
# appropriate platform tags for each supported platform.
#
# Usage:
#   ./scripts/build-orchestrator-wheels.sh <binaries-dir> [output-dir]
#
# Arguments:
#   binaries-dir   Directory containing pre-built binaries named:
#                    stepflow-server-<rust-target>[.exe]
#   output-dir     Output directory for wheels (default: ./wheels)
#
# Expects binaries named:
#   stepflow-server-x86_64-unknown-linux-gnu
#   stepflow-server-aarch64-unknown-linux-gnu
#   stepflow-server-x86_64-unknown-linux-musl
#   stepflow-server-aarch64-unknown-linux-musl
#   stepflow-server-x86_64-apple-darwin
#   stepflow-server-aarch64-apple-darwin
#   stepflow-server-x86_64-pc-windows-msvc.exe
#
# Example:
#   # Build all wheels from release binaries
#   ./scripts/build-orchestrator-wheels.sh ./binaries ./dist
#
#   # Build wheel for current platform only (development)
#   cargo build --release -p stepflow-server
#   mkdir /tmp/bins && cp target/release/stepflow-server /tmp/bins/stepflow-server-$(rustc -vV | grep host | cut -d' ' -f2)
#   ./scripts/build-orchestrator-wheels.sh /tmp/bins ./wheels

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Help function
show_help() {
    cat << 'EOF'
Usage: build-orchestrator-wheels.sh <binaries-dir> [output-dir]

Build platform-specific wheels for stepflow-orchestrator from pre-built binaries.

ARGUMENTS:
    binaries-dir    Directory containing stepflow-server binaries
    output-dir      Output directory for wheels (default: ./wheels)

BINARY NAMING:
    Binaries should be named: stepflow-server-<rust-target>[.exe]

    Supported targets:
      - x86_64-unknown-linux-gnu    -> manylinux_2_17_x86_64
      - aarch64-unknown-linux-gnu   -> manylinux_2_17_aarch64
      - x86_64-unknown-linux-musl   -> musllinux_1_2_x86_64
      - aarch64-unknown-linux-musl  -> musllinux_1_2_aarch64
      - x86_64-apple-darwin         -> macosx_11_0_x86_64
      - aarch64-apple-darwin        -> macosx_11_0_arm64
      - x86_64-pc-windows-msvc      -> win_amd64

EXAMPLES:
    # Build from CI artifacts
    ./scripts/build-orchestrator-wheels.sh ./artifacts ./wheels

    # Build for current platform (development)
    cd stepflow-rs && cargo build --release -p stepflow-server
    mkdir /tmp/bins
    cp target/release/stepflow-server /tmp/bins/stepflow-server-$(rustc -vV | grep host | cut -d' ' -f2)
    ./scripts/build-orchestrator-wheels.sh /tmp/bins ./wheels
EOF
}

# Parse arguments
if [[ $# -lt 1 ]] || [[ "$1" == "-h" ]] || [[ "$1" == "--help" ]]; then
    show_help
    exit 0
fi

BINARIES_DIR="$1"
OUTPUT_DIR="${2:-./wheels}"

# Convert paths to absolute (needed because we cd during build)
BINARIES_DIR="$(cd "$BINARIES_DIR" && pwd)"
OUTPUT_DIR="$(mkdir -p "$OUTPUT_DIR" && cd "$OUTPUT_DIR" && pwd)"

# Validate binaries directory
if [[ ! -d "$BINARIES_DIR" ]]; then
    echo -e "${RED}Error: Binaries directory not found: $BINARIES_DIR${NC}" >&2
    exit 1
fi

# Find project root and orchestrator package
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ORCHESTRATOR_DIR="$PROJECT_ROOT/sdks/python/stepflow-orchestrator"

if [[ ! -d "$ORCHESTRATOR_DIR" ]]; then
    echo -e "${RED}Error: Orchestrator package not found at: $ORCHESTRATOR_DIR${NC}" >&2
    exit 1
fi

# Platform mappings: rust_target -> wheel_tag
# Format: "rust_target:wheel_tag:binary_suffix"
PLATFORMS=(
    "x86_64-unknown-linux-gnu:manylinux_2_17_x86_64:"
    "aarch64-unknown-linux-gnu:manylinux_2_17_aarch64:"
    "x86_64-unknown-linux-musl:musllinux_1_2_x86_64:"
    "aarch64-unknown-linux-musl:musllinux_1_2_aarch64:"
    "x86_64-apple-darwin:macosx_11_0_x86_64:"
    "aarch64-apple-darwin:macosx_11_0_arm64:"
    "x86_64-pc-windows-msvc:win_amd64:.exe"
)

# Create temporary directory for wheel building
BUILD_TMPDIR=$(mktemp -d)
trap "rm -rf '$BUILD_TMPDIR'" EXIT

echo -e "${BLUE}Building stepflow-orchestrator wheels${NC}"
echo "  Binaries dir: $BINARIES_DIR"
echo "  Output dir:   $OUTPUT_DIR"
echo ""

# Get package version
VERSION=$(grep '^version = ' "$ORCHESTRATOR_DIR/pyproject.toml" | head -1 | sed 's/version = "\(.*\)"/\1/')
echo -e "${BLUE}Package version: ${GREEN}$VERSION${NC}"
echo ""

# Track which platforms we built
BUILT_PLATFORMS=()
SKIPPED_PLATFORMS=()

# Build wheels for each platform
for platform_spec in "${PLATFORMS[@]}"; do
    IFS=':' read -r rust_target wheel_tag binary_suffix <<< "$platform_spec"

    binary_name="stepflow-server-${rust_target}${binary_suffix}"
    binary_path="$BINARIES_DIR/$binary_name"

    # Check if binary exists
    if [[ ! -f "$binary_path" ]]; then
        echo -e "${YELLOW}Skipping $rust_target (binary not found: $binary_name)${NC}"
        SKIPPED_PLATFORMS+=("$rust_target")
        continue
    fi

    echo -e "${BLUE}Building wheel for ${GREEN}$rust_target${NC} -> ${GREEN}$wheel_tag${NC}"

    # Create a clean copy of the package for this build
    BUILD_DIR="$BUILD_TMPDIR/$rust_target"
    cp -r "$ORCHESTRATOR_DIR" "$BUILD_DIR"

    # Copy binary to bin directory
    BIN_DIR="$BUILD_DIR/src/stepflow_orchestrator/bin"
    mkdir -p "$BIN_DIR"

    # Determine target binary name (stepflow-server or stepflow-server.exe)
    if [[ -n "$binary_suffix" ]]; then
        target_binary="stepflow-server${binary_suffix}"
    else
        target_binary="stepflow-server"
    fi

    cp "$binary_path" "$BIN_DIR/$target_binary"
    chmod +x "$BIN_DIR/$target_binary"

    # Build wheel
    cd "$BUILD_DIR"

    # Clean any existing dist
    rm -rf dist/

    # Build with uv
    uv build --wheel --quiet

    # Find the built wheel and rename with platform tag
    for wheel in dist/*.whl; do
        if [[ -f "$wheel" ]]; then
            # Original: stepflow_orchestrator-X.Y.Z-py3-none-any.whl
            # Target: stepflow_orchestrator-X.Y.Z-py3-none-<wheel_tag>.whl
            wheel_basename=$(basename "$wheel")
            new_name=$(echo "$wheel_basename" | sed "s/-py3-none-any\.whl/-py3-none-${wheel_tag}.whl/")

            # Move to output directory with new name
            mv "$wheel" "$OUTPUT_DIR/$new_name"
            echo -e "  ${GREEN}Created: $new_name${NC}"
        fi
    done

    BUILT_PLATFORMS+=("$rust_target")

    cd "$PROJECT_ROOT"
done

echo ""
echo -e "${GREEN}=========================================${NC}"
echo -e "${GREEN}Wheel building complete!${NC}"
echo -e "${GREEN}=========================================${NC}"
echo ""

if [[ ${#BUILT_PLATFORMS[@]} -gt 0 ]]; then
    echo -e "${BLUE}Built wheels for ${#BUILT_PLATFORMS[@]} platform(s):${NC}"
    for platform in "${BUILT_PLATFORMS[@]}"; do
        echo "  - $platform"
    done
fi

if [[ ${#SKIPPED_PLATFORMS[@]} -gt 0 ]]; then
    echo ""
    echo -e "${YELLOW}Skipped ${#SKIPPED_PLATFORMS[@]} platform(s) (binaries not found):${NC}"
    for platform in "${SKIPPED_PLATFORMS[@]}"; do
        echo "  - $platform"
    done
fi

echo ""
echo -e "${BLUE}Output:${NC}"
ls -la "$OUTPUT_DIR"/*.whl 2>/dev/null || echo "  (no wheels found)"

if [[ ${#BUILT_PLATFORMS[@]} -eq 0 ]]; then
    echo ""
    echo -e "${RED}Error: No wheels were built. Check that binaries exist in $BINARIES_DIR${NC}"
    exit 1
fi
