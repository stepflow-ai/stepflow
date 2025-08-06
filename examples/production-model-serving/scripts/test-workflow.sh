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

# Test script for production model serving demo

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "🧪 Testing Production Model Serving Demo"
echo "========================================"

echo ""
echo "1️⃣  Testing Development Mode (Direct Run)..."
echo "============================================="
"$SCRIPT_DIR/run-dev-direct.sh" text

echo ""
echo "2️⃣  Testing Development Mode (Serve/Submit)..."
echo "==============================================="
"$SCRIPT_DIR/run-dev.sh" text

echo ""
echo "3️⃣  Testing Production Mode..."
echo "==============================="
"$SCRIPT_DIR/run-prod.sh" text

echo ""
echo "4️⃣  Testing Batch Processing (Direct Run)..."
echo "============================================="
"$SCRIPT_DIR/run-dev-direct.sh" batch

echo ""
echo "5️⃣  Testing Multimodal Processing (Production)..."
echo "================================================="
"$SCRIPT_DIR/run-prod.sh" multimodal

echo ""
echo "✅ All tests completed successfully!"
echo ""
echo "🧹 Cleaning up..."
cd "$(dirname "$SCRIPT_DIR")"
docker-compose down

echo "✨ Demo test suite completed!"