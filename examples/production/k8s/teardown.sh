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

# Stepflow Kubernetes Teardown Script
# Removes all stepflow resources in reverse order
#
# Usage: ./teardown.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "=== Stepflow Kubernetes Teardown ==="
echo ""
echo "This will delete ALL stepflow resources including persistent data."
read -p "Are you sure? (y/N) " -n 1 -r
echo ""

if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Aborted."
    exit 0
fi

# 1. Application components
echo "[1/4] Removing stepflow applications..."
kubectl delete -f stepflow/langflow-worker/ --ignore-not-found
kubectl delete -f stepflow/loadbalancer/ --ignore-not-found
kubectl delete -f stepflow/server/ --ignore-not-found

# 2. OpenSearch
echo "[2/4] Removing OpenSearch..."
kubectl delete -f stepflow/opensearch/ --ignore-not-found

# 3. Observability stack
echo "[3/4] Removing observability stack..."
kubectl delete -f stepflow-o12y/grafana/ --ignore-not-found
kubectl delete -f stepflow-o12y/promtail/ --ignore-not-found
kubectl delete -f stepflow-o12y/loki/ --ignore-not-found
kubectl delete -f stepflow-o12y/prometheus/ --ignore-not-found
kubectl delete -f stepflow-o12y/jaeger/ --ignore-not-found
kubectl delete -f stepflow-o12y/otel-collector/ --ignore-not-found

# 4. Namespaces (cleans up any remaining resources)
echo "[4/4] Removing namespaces..."
kubectl delete -f namespaces.yaml --ignore-not-found

echo ""
echo "=== Teardown complete ==="
