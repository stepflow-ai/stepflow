#!/bin/bash
# Build script for Stepflow Python packages
#
# This script builds the stepflow-server binary and all Python packages.
# The stepflow-runtime package bundles the binary for platform-specific distribution.
#
# Usage:
#   ./scripts/build-python-packages.sh [--release]
#
# Options:
#   --release    Build in release mode (default: debug)
#
# Prerequisites:
#   - Rust toolchain (cargo)
#   - Python 3.11+ with uv installed
#
# Output:
#   - Built wheels in sdks/python/*/dist/

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PYTHON_DIR="$PROJECT_ROOT/sdks/python"

# Parse arguments
BUILD_MODE="debug"
CARGO_FLAGS=""
while [[ $# -gt 0 ]]; do
    case $1 in
        --release)
            BUILD_MODE="release"
            CARGO_FLAGS="--release"
            shift
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

echo "=== Building Stepflow Python Packages ==="
echo "Build mode: $BUILD_MODE"
echo "Project root: $PROJECT_ROOT"
echo ""

# Determine binary name based on OS
if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "cygwin" || "$OSTYPE" == "win32" ]]; then
    BINARY_NAME="stepflow-server.exe"
else
    BINARY_NAME="stepflow-server"
fi

# Step 1: Build stepflow-server binary
echo ">>> Building stepflow-server binary..."
cd "$PROJECT_ROOT/stepflow-rs"
cargo build $CARGO_FLAGS -p stepflow-server

# Determine output path based on build mode
if [[ "$BUILD_MODE" == "release" ]]; then
    BINARY_SRC="$PROJECT_ROOT/stepflow-rs/target/release/$BINARY_NAME"
else
    BINARY_SRC="$PROJECT_ROOT/stepflow-rs/target/debug/$BINARY_NAME"
fi

# Verify binary was built
if [[ ! -f "$BINARY_SRC" ]]; then
    echo "ERROR: Binary not found at $BINARY_SRC"
    exit 1
fi
echo "Binary built: $BINARY_SRC"

# Step 2: Copy binary to stepflow-runtime package
echo ""
echo ">>> Copying binary to stepflow-runtime package..."
RUNTIME_BIN_DIR="$PYTHON_DIR/stepflow-runtime/src/stepflow_runtime/bin"
mkdir -p "$RUNTIME_BIN_DIR"
cp "$BINARY_SRC" "$RUNTIME_BIN_DIR/"
chmod +x "$RUNTIME_BIN_DIR/$BINARY_NAME"
echo "Binary copied to: $RUNTIME_BIN_DIR/$BINARY_NAME"

# Step 3: Build Python packages
echo ""
echo ">>> Building Python packages..."

# Function to build a package
build_package() {
    local pkg_name=$1
    local pkg_dir="$PYTHON_DIR/$pkg_name"

    if [[ ! -d "$pkg_dir" ]]; then
        echo "WARNING: Package directory not found: $pkg_dir"
        return 1
    fi

    echo ""
    echo "Building $pkg_name..."
    cd "$pkg_dir"

    # Clean old builds
    rm -rf dist/ build/ *.egg-info

    # Build using uv
    if command -v uv &> /dev/null; then
        uv build
    else
        echo "WARNING: uv not found, falling back to pip"
        python -m build
    fi

    echo "$pkg_name built successfully"
    ls -la dist/ 2>/dev/null || true
}

# Build packages in dependency order
build_package "stepflow"
build_package "stepflow-client"
build_package "stepflow-server"
build_package "stepflow-runtime"

# Summary
echo ""
echo "=== Build Complete ==="
echo ""
echo "Built packages:"
for pkg in stepflow stepflow-client stepflow-server stepflow-runtime; do
    pkg_dist="$PYTHON_DIR/$pkg/dist"
    if [[ -d "$pkg_dist" ]]; then
        echo "  $pkg:"
        ls "$pkg_dist"/*.whl 2>/dev/null | while read -r f; do
            echo "    - $(basename "$f")"
        done
    fi
done
echo ""
echo "To install locally for development:"
echo "  pip install $PYTHON_DIR/stepflow/dist/*.whl"
echo "  pip install $PYTHON_DIR/stepflow-client/dist/*.whl"
echo "  pip install $PYTHON_DIR/stepflow-server/dist/*.whl"
echo "  pip install $PYTHON_DIR/stepflow-runtime/dist/*.whl"
