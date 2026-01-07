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

# Stepflow Kubernetes Deployment Script
# Applies manifests in correct dependency order
#
# Usage: ./apply.sh
#
# Prerequisites:
#   - kubectl configured with access to your cluster
#   - Docker images built and loaded into Kind (if using Kind)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "=== Stepflow Kubernetes Deployment ==="
echo ""

# 1. Namespaces (must exist before anything else)
echo "[1/6] Creating namespaces..."
kubectl apply -f namespaces.yaml

# 2. Observability stack (so telemetry endpoints are ready)
echo "[2/6] Deploying observability stack (stepflow-o12y)..."
kubectl apply -f stepflow-o12y/otel-collector/
kubectl apply -f stepflow-o12y/jaeger/
kubectl apply -f stepflow-o12y/prometheus/
kubectl apply -f stepflow-o12y/loki/
kubectl apply -f stepflow-o12y/promtail/
kubectl apply -f stepflow-o12y/grafana/

# 3. Wait for OTel Collector to be ready (apps depend on it)
echo "    Waiting for OTel Collector..."
kubectl wait --for=condition=available --timeout=120s \
    deployment/otel-collector -n stepflow-o12y 2>/dev/null || echo "    (OTel Collector not ready yet, continuing...)"

# 4. OpenSearch (data layer, apps may depend on it)
echo "[3/6] Deploying OpenSearch..."
kubectl apply -f stepflow/opensearch/

# 5. Wait for OpenSearch to be ready
echo "    Waiting for OpenSearch..."
kubectl wait --for=condition=available --timeout=180s \
    deployment/opensearch -n stepflow 2>/dev/null || echo "    (OpenSearch not ready yet, continuing...)"

# 6. Application components
echo "[4/6] Deploying stepflow applications..."
kubectl apply -f stepflow/server/
kubectl apply -f stepflow/loadbalancer/
kubectl apply -f stepflow/langflow-component-server/

# 7. Wait for application components
echo "[5/6] Waiting for application components..."
kubectl wait --for=condition=available --timeout=120s \
    deployment/stepflow-server -n stepflow 2>/dev/null || echo "    (stepflow-server not ready yet)"
kubectl wait --for=condition=available --timeout=120s \
    deployment/stepflow-load-balancer -n stepflow 2>/dev/null || echo "    (load-balancer not ready yet)"

# 8. Final status
echo "[6/6] Deployment complete. Checking status..."
echo ""
echo "=== Stepflow Namespace ==="
kubectl get pods -n stepflow
echo ""
echo "=== Observability Namespace ==="
kubectl get pods -n stepflow-o12y
echo ""
echo "=== Access URLs ==="
echo "Stepflow API:  http://localhost:7840  (via Kind port mapping)"
echo "Grafana:       http://localhost:3000  (admin/admin)"
echo "Jaeger:        http://localhost:16686"
echo "Prometheus:    http://localhost:9090"
echo ""
echo "Or use kubectl port-forward:"
echo "  kubectl port-forward -n stepflow svc/stepflow-server 7840:7840"
echo "  kubectl port-forward -n stepflow-o12y svc/grafana 3000:3000"
echo "  kubectl port-forward -n stepflow-o12y svc/jaeger 16686:16686"
echo "  kubectl port-forward -n stepflow-o12y svc/prometheus 9090:9090"
