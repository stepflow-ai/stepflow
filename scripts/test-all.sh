#!/bin/bash
# Licensed to the Apache Software Foundation (ASF) under one or more contributor license agreements.
# See the NOTICE file distributed with this work for additional information regarding copyright
# ownership.  The ASF licenses this file to you under the Apache License, Version 2.0 (the
# "License"); you may not use this file except in compliance with the License.  You may obtain a
# copy of the License at
#
#   http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software distributed under the License
# is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
# or implied.  See the License for the specific language governing permissions and limitations under
# the License.

set -e

echo "🚀 Running complete test suite..."

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

# Run Rust unit tests
echo "📦 Running Rust unit tests..."
cd stepflow-rs
cargo test
cd ..

# Run Python SDK tests
echo "🐍 Running Python SDK tests..."
cd sdks/python
uv run poe test
cd ../..

# Run integration tests
echo "🔗 Running integration tests..."
./scripts/test-integration.sh

echo "✅ Complete test suite finished successfully"