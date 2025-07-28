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

set -e

echo "🧪 Running integration tests..."

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

# Setup Python environment
echo "🐍 Setting up Python environment..."
cd sdks/python
uv sync
cd ../..

# Build stepflow binary
echo "🔨 Building stepflow binary..."
cd stepflow-rs
cargo build

# Test workflows using stepflow test command
echo "🔗 Testing workflows..."

# Test both tests and examples directories together
echo "  Testing both tests and examples directories..."
./target/debug/stepflow test ../tests ../examples

echo "✅ Integration tests completed successfully"