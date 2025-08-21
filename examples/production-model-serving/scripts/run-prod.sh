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

# Copyright {{ year }} {{ authors }}
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

# Production mode runner for production model serving demo
# Uses serve/submit pattern with containerized services

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXAMPLE_DIR="$(dirname "$SCRIPT_DIR")"
STEPFLOW_DIR="$(dirname "$(dirname "$EXAMPLE_DIR")")/stepflow-rs"

echo "🏭 Running Production Model Serving Demo in Production Mode"
echo "==========================================================="
echo "Using containerized serve/submit pattern for production deployment"

# Check dependencies
echo "📦 Checking dependencies..."
if ! command -v docker &> /dev/null; then
    echo "❌ Docker is required but not installed"
    exit 1
fi

if ! command -v docker-compose &> /dev/null; then
    echo "❌ Docker Compose is required but not installed"
    exit 1
fi

echo "✅ Dependencies satisfied"

# Navigate to example directory
cd "$EXAMPLE_DIR"

echo "🔧 Configuration: Production (containerized serve/submit)"
echo "📂 Working directory: $EXAMPLE_DIR"

# Build Stepflow binary for submit command
echo "🔨 Building Stepflow binary for client operations..."
cd "$STEPFLOW_DIR"
cargo build --release

STEPFLOW_BINARY="$STEPFLOW_DIR/target/release/stepflow"
if [ ! -f "$STEPFLOW_BINARY" ]; then
    echo "❌ Failed to build Stepflow binary"
    exit 1
fi

echo "✅ Stepflow binary built successfully"

# Return to example directory
cd "$EXAMPLE_DIR"

# Choose input file
INPUT_FILE="sample_input_text.json"
if [ "$1" = "multimodal" ]; then
    INPUT_FILE="sample_input_multimodal.json"
elif [ "$1" = "batch" ]; then
    INPUT_FILE="sample_input_batch.json"
fi

echo "📄 Using input: $INPUT_FILE"

# Build and start all services including Stepflow server
echo ""
echo "🏗️  Building and starting production services..."
echo "   - Stepflow Runtime Server"
echo "   - Text Models Server"
echo "   - Vision Models Server"
echo "   - Monitoring (Prometheus, Redis)"

docker-compose up -d --build

# Wait for all services to be healthy
echo ""
echo "⏳ Waiting for all services to be healthy..."
max_attempts=60  # Increased for Docker builds
attempt=0

while [ $attempt -lt $max_attempts ]; do
    # Check if Stepflow server is healthy
    if curl -s "http://localhost:7837/health" >/dev/null 2>&1; then
        echo "✅ All services are healthy and ready!"
        break
    fi

    echo "   Attempt $((attempt + 1))/$max_attempts - waiting for services to start..."
    sleep 5
    attempt=$((attempt + 1))
done

if [ $attempt -eq $max_attempts ]; then
    echo "❌ Services failed to become healthy. Checking logs:"
    docker-compose logs stepflow-server
    exit 1
fi

# Show service status
echo ""
echo "📊 Service Status:"
docker-compose ps

echo ""
echo "🌐 Stepflow Server Running:"
echo "   - URL: http://localhost:7837"
echo "   - Health: http://localhost:7837/health"
echo "   - API: http://localhost:7837/api/v1"

echo ""
echo "▶️  Submitting workflow to production Stepflow server..."

"$STEPFLOW_BINARY" submit \
    --url="http://localhost:7837/api/v1" \
    --flow="ai_pipeline_workflow.yaml" \
    --input="$INPUT_FILE"

echo ""
echo "✅ Production mode execution completed!"
echo ""
echo "🔍 Production services are still running. To view logs:"
echo "   docker-compose logs stepflow-server  # Stepflow runtime logs"
echo "   docker-compose logs text-models      # Text model server logs"
echo "   docker-compose logs vision-models    # Vision model server logs"
echo ""
echo "📊 Service endpoints:"
echo "   - Stepflow API: http://localhost:7837/api/v1"
echo "   - Text Models: http://localhost:8080"
echo "   - Vision Models: http://localhost:8081"
echo "   - Prometheus: http://localhost:9090"
echo ""
echo "🛑 To stop all services:"
echo "   docker-compose down"
echo ""
echo "💡 To test other input types:"
echo "   $0 multimodal  # Test with image processing"
echo "   $0 batch       # Test batch processing"
echo ""
echo "🔄 To submit additional workflows (while services are running):"
echo "   $STEPFLOW_BINARY submit --url=http://localhost:7837/api/v1 --flow=ai_pipeline_workflow.yaml --input=$INPUT_FILE"