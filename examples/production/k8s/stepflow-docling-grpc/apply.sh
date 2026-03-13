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

# Stepflow-Docling gRPC Parity Testbed — Deployment Script
# Self-contained: deploys o11y stack + docling application namespace.
#
# Usage: ./apply.sh
#
# Prerequisites:
#   - kubectl configured with access to your cluster
#   - Docker images built and loaded into Kind:
#     - localhost/docling-facade:grpc-v1
#     - localhost/stepflow-docling-proto-worker:latest
#     - localhost/stepflow-server:alpine-0.10.0

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "=== Stepflow-Docling gRPC Parity Testbed Deployment ==="
echo ""

# 1. Namespaces
echo "[1/7] Creating namespaces..."
kubectl apply -f namespaces.yaml

# 2. Observability stack (shared manifests from parent directory)
echo "[2/7] Deploying observability stack (stepflow-o11y)..."
kubectl apply -f ../stepflow-o11y/otel-collector/
kubectl apply -f ../stepflow-o11y/jaeger/
kubectl apply -f ../stepflow-o11y/prometheus/
kubectl apply -f ../stepflow-o11y/loki/
kubectl apply -f ../stepflow-o11y/promtail/
kubectl apply -f ../stepflow-o11y/grafana/

# 3. Wait for OTel Collector
echo "[3/7] Waiting for OTel Collector..."
kubectl wait --for=condition=available --timeout=120s \
    deployment/otel-collector -n stepflow-o11y 2>/dev/null || echo "    (OTel Collector not ready yet, continuing...)"

# 4. Deploy orchestrator (must be up first — workers connect to it)
echo "[4/7] Deploying orchestrator (facade + stepflow-server)..."
kubectl apply -f orchestrator/

# 5. Wait for orchestrator
echo "[5/7] Waiting for orchestrator..."
kubectl wait --for=condition=available --timeout=120s \
    deployment/docling-orchestrator -n stepflow-docling-grpc 2>/dev/null || echo "    (orchestrator not ready yet)"

# 6. Deploy workers (connect to orchestrator via gRPC)
echo "[6/7] Deploying docling gRPC workers..."
kubectl apply -f worker/

# 7. Status and access URLs
echo "[7/7] Deployment complete. Checking status..."
echo ""
echo "=== stepflow-docling-grpc Namespace ==="
kubectl get pods -n stepflow-docling-grpc
echo ""
echo "=== Observability Namespace ==="
kubectl get pods -n stepflow-o11y
echo ""
echo "=== Access URLs ==="
echo "Docling Facade: http://localhost:5001      (docling-serve parity API)"
echo "  API Docs:     http://localhost:5001/docs"
echo "  Health:       http://localhost:5001/health"
echo "Grafana:        http://localhost:3000      (admin/admin)"
echo "Jaeger:         http://localhost:16686"
echo "Prometheus:     http://localhost:9090"
echo ""
echo "Run the parity test:"
echo "  python test-parity.py"
