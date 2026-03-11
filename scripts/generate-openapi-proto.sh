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

# Generate OpenAPI spec from proto files.
#
# Pipeline:
#   1. buf generate — runs google-gnostic-openapi plugin to produce OpenAPI YAML
#   2. Inject servers block (protoc-gen-openapi doesn't support it natively)
#   3. (optional) tonic-rest-openapi — patch with SSE, validation, security, etc.
#
# Prerequisites:
#   buf (https://buf.build/docs/installation)
#   Optional: cargo install tonic-rest-openapi --features cli
#
# Usage: ./scripts/generate-openapi-proto.sh [OUTPUT_PATH]
#
# Arguments:
#   OUTPUT_PATH  Optional path for the generated spec (default: schemas/openapi.yaml)
#
# Output: schemas/openapi.yaml (or OUTPUT_PATH if specified)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PROTO_DIR="$PROJECT_ROOT/proto"
OUTPUT_PATH="${1:-$PROJECT_ROOT/schemas/openapi.yaml}"
OUTPUT_DIR="$(dirname "$OUTPUT_PATH")"

# Check prerequisites
if ! command -v buf &>/dev/null; then
    echo "ERROR: buf not found."
    echo "Install from: https://buf.build/docs/installation"
    echo "  brew install bufbuild/buf/buf  (macOS)"
    exit 1
fi

mkdir -p "$OUTPUT_DIR"

cd "$PROTO_DIR"

# Generate base OpenAPI spec via buf + google-gnostic-openapi plugin.
# Use a temporary buf.gen.yaml that outputs to the target directory.
TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT
TMPGEN="$TMPDIR/buf.gen.yaml"
sed "s|out: .*|out: $OUTPUT_DIR|" "$PROTO_DIR/buf.gen.yaml" > "$TMPGEN"
buf generate --template "$TMPGEN"

# If the output directory differs from schemas/, the file lands as
# $OUTPUT_DIR/openapi.yaml. Move it to the requested path if needed.
GENERATED="$OUTPUT_DIR/openapi.yaml"
if [ "$GENERATED" != "$OUTPUT_PATH" ]; then
    mv "$GENERATED" "$OUTPUT_PATH"
fi

# Inject servers block — protoc-gen-openapi doesn't support it natively.
if ! grep -q '^servers:' "$OUTPUT_PATH" 2>/dev/null; then
    # Use perl for portable in-place editing (BSD sed -i differs from GNU)
    perl -pi -e 'print "servers:\n    - url: http://localhost:7837/api/v1\n      description: Local development server\n" if /^paths:/' "$OUTPUT_PATH"
fi

# Optionally apply tonic-rest-openapi patches (SSE, validation, security, etc.)
OPENAPI_CLI="${OPENAPI_CLI:-tonic-rest-openapi}"
if command -v "$OPENAPI_CLI" &>/dev/null; then
    echo "Applying tonic-rest-openapi patches..."
    "$OPENAPI_CLI" generate \
        --config "$PROTO_DIR/openapi-config.yaml" \
        --cargo-toml "$PROJECT_ROOT/stepflow-rs/Cargo.toml"
    echo "Patched: $OUTPUT_PATH"
fi

echo "Generated: $OUTPUT_PATH"
