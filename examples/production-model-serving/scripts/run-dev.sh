#!/bin/bash
# Licensed to the Apache Software Foundation (ASF) under one or more contributor
# license agreements.  See the NOTICE file distributed with this work for
# additional information regarding copyright ownership.  The ASF licenses this
# file to you under the Apache License, Version 2.0 (the "License"); you may not
# use this file except in compliance with the License.  You may obtain a copy of
# the License at
#
#   http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.  See the
# License for the specific language governing permissions and limitations under
# the License.

# Development mode runner for production model serving demo
# Uses serve/submit pattern for realistic production simulation

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXAMPLE_DIR="$(dirname "$SCRIPT_DIR")"
STEPFLOW_DIR="$(dirname "$(dirname "$EXAMPLE_DIR")")/stepflow-rs"

echo "🚀 Running Production Model Serving Demo in Development Mode"
echo "============================================================"
echo "Using serve/submit pattern for production-like workflow execution"

# Check dependencies
echo "📦 Checking dependencies..."
if ! command -v python3 &> /dev/null; then
    echo "❌ Python 3 is required but not installed"
    exit 1
fi

# Build StepFlow binary first
echo "🔨 Building StepFlow binary..."
cd "$STEPFLOW_DIR"
cargo build --release

STEPFLOW_BINARY="$STEPFLOW_DIR/target/release/stepflow"
if [ ! -f "$STEPFLOW_BINARY" ]; then
    echo "❌ Failed to build StepFlow binary"
    exit 1
fi

echo "✅ StepFlow binary built successfully"

# Navigate to example directory for model server execution
cd "$EXAMPLE_DIR"

echo "🔧 Configuration: Development (subprocess-based serve/submit)"
echo "📂 Working directory: $EXAMPLE_DIR"
echo "🏗️  StepFlow binary: $STEPFLOW_BINARY"

# Choose input file
INPUT_FILE="sample_input_text.json"
if [ "$1" = "multimodal" ]; then
    INPUT_FILE="sample_input_multimodal.json"
elif [ "$1" = "batch" ]; then
    INPUT_FILE="sample_input_batch.json"
fi

echo "📄 Using input: $INPUT_FILE"
echo ""

# Start StepFlow server in background
SERVER_PORT=7837
SERVER_PID=""

cleanup() {
    if [ ! -z "$SERVER_PID" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
        echo "🛑 Stopping StepFlow server (PID: $SERVER_PID)..."
        kill "$SERVER_PID"
        wait "$SERVER_PID" 2>/dev/null || true
    fi
}

trap cleanup EXIT

echo "🌐 Starting StepFlow server on port $SERVER_PORT..."
"$STEPFLOW_BINARY" serve \
    --port="$SERVER_PORT" \
    --log-file="stepflow-server-dev.log" \
    --config="stepflow-config-dev.yml" &
SERVER_PID=$!

# Wait for server to start
echo "⏳ Waiting for server to be ready..."
max_attempts=30
attempt=0

while [ $attempt -lt $max_attempts ]; do
    if curl -s "http://localhost:$SERVER_PORT/health" >/dev/null 2>&1; then
        echo "✅ StepFlow server is ready!"
        break
    fi

    if ! kill -0 "$SERVER_PID" 2>/dev/null; then
        echo "❌ StepFlow server failed to start"
        exit 1
    fi

    echo "   Attempt $((attempt + 1))/$max_attempts - waiting for server..."
    sleep 2
    attempt=$((attempt + 1))
done

if [ $attempt -eq $max_attempts ]; then
    echo "❌ Server failed to become ready"
    exit 1
fi

echo ""
echo "▶️  Submitting workflow for execution..."

"$STEPFLOW_BINARY" submit \
    --url="http://localhost:$SERVER_PORT/api/v1" \
    --flow="ai_pipeline_workflow.yaml" \
    --input="$INPUT_FILE"

echo ""
echo "✅ Development mode execution completed!"
echo ""
echo "💡 To test other input types:"
echo "   $0 multimodal  # Test with image processing"
echo "   $0 batch       # Test batch processing"
echo ""
echo "🔍 Server was running on: http://localhost:$SERVER_PORT"
echo "📊 Health endpoint: http://localhost:$SERVER_PORT/health"