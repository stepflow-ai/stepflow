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

# Stepflow-Docling Parity Testbed — Teardown Script
# Removes all resources in reverse dependency order.
#
# Usage: ./teardown.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "=== Stepflow-Docling Parity Testbed Teardown ==="
echo ""
echo "This will delete ALL stepflow-docling resources."
read -p "Are you sure? (y/N) " -n 1 -r
echo ""

if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Aborted."
    exit 0
fi

# 1. Orchestrator
echo "[1/5] Removing orchestrator..."
kubectl delete -f orchestrator/ --ignore-not-found

# 2. Load balancer
echo "[2/5] Removing load balancer..."
kubectl delete -f loadbalancer/ --ignore-not-found

# 3. Workers
echo "[3/5] Removing workers..."
kubectl delete -f worker/ --ignore-not-found

# 4. Observability stack
echo "[4/5] Removing observability stack..."
kubectl delete -f ../stepflow-o11y/grafana/ --ignore-not-found
kubectl delete -f ../stepflow-o11y/promtail/ --ignore-not-found
kubectl delete -f ../stepflow-o11y/loki/ --ignore-not-found
kubectl delete -f ../stepflow-o11y/prometheus/ --ignore-not-found
kubectl delete -f ../stepflow-o11y/jaeger/ --ignore-not-found
kubectl delete -f ../stepflow-o11y/otel-collector/ --ignore-not-found

# 5. Namespaces
echo "[5/5] Removing namespaces..."
kubectl delete -f namespaces.yaml --ignore-not-found

echo ""
echo "=== Teardown complete ==="
