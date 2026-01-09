# Stepflow K8s Command Reference

Quick reference for common operations when working with the Stepflow Kubernetes deployment.

## Cluster Management

### Create/Delete Cluster

```bash
# Create cluster
kind create cluster --config k8s/kind-config.yaml

# Delete cluster
kind delete cluster --name stepflow

# List clusters
kind get clusters
```

### Context Management

```bash
# Switch to stepflow context
kubectl config use-context kind-stepflow

# Verify current context
kubectl config current-context

# View cluster info
kubectl cluster-info --context kind-stepflow
```

## Deployment

### Full Deployment

```bash
# Deploy everything (recommended)
./k8s/apply.sh

# Teardown everything
./k8s/teardown.sh
```

### Manual Deployment

```bash
# Create namespaces first
kubectl apply -f k8s/namespaces.yaml

# Deploy observability stack
kubectl apply -f k8s/stepflow-o12y/otel-collector/
kubectl apply -f k8s/stepflow-o12y/jaeger/
kubectl apply -f k8s/stepflow-o12y/prometheus/
kubectl apply -f k8s/stepflow-o12y/loki/
kubectl apply -f k8s/stepflow-o12y/promtail/
kubectl apply -f k8s/stepflow-o12y/grafana/

# Deploy application stack
kubectl apply -f k8s/stepflow/opensearch/
kubectl apply -f k8s/stepflow/server/
kubectl apply -f k8s/stepflow/loadbalancer/
kubectl apply -f k8s/stepflow/langflow-worker/
```

## Pod Operations

### View Pods

```bash
# All stepflow application pods
kubectl get pods -n stepflow

# All observability pods
kubectl get pods -n stepflow-o12y

# All pods across both namespaces
kubectl get pods -n stepflow && kubectl get pods -n stepflow-o12y

# Watch pods (auto-refresh)
kubectl get pods -n stepflow -w
```

### Pod Details

```bash
# Describe a pod (events, conditions, volumes)
kubectl describe pod -n stepflow <pod-name>

# Get pod YAML
kubectl get pod -n stepflow <pod-name> -o yaml

# Get pod resource usage
kubectl top pod -n stepflow
```

### Restart Pods

```bash
# Restart a deployment (rolling restart)
kubectl rollout restart deployment/stepflow-server -n stepflow
kubectl rollout restart deployment/stepflow-load-balancer -n stepflow
kubectl rollout restart deployment/langflow-worker -n stepflow
kubectl rollout restart deployment/opensearch -n stepflow

# Restart observability components
kubectl rollout restart deployment/otel-collector -n stepflow-o12y
kubectl rollout restart deployment/grafana -n stepflow-o12y
kubectl rollout restart deployment/jaeger -n stepflow-o12y
kubectl rollout restart deployment/prometheus -n stepflow-o12y

# Delete all pods in namespace (forces recreation)
kubectl delete pods -n stepflow --all
```

## Logs

### Application Logs

```bash
# Stepflow server logs
kubectl logs -n stepflow deployment/stepflow-server --tail=100
kubectl logs -n stepflow deployment/stepflow-server -f  # follow

# Load balancer logs
kubectl logs -n stepflow deployment/stepflow-load-balancer --tail=100

# Worker logs (all replicas)
kubectl logs -n stepflow -l app=langflow-worker --tail=50

# OpenSearch logs
kubectl logs -n stepflow deployment/opensearch --tail=100

# Previous container logs (after crash)
kubectl logs -n stepflow deployment/stepflow-server --previous
```

### Observability Logs

```bash
# OTel Collector logs
kubectl logs -n stepflow-o12y deployment/otel-collector --tail=100

# Grafana logs
kubectl logs -n stepflow-o12y deployment/grafana --tail=100

# Jaeger logs
kubectl logs -n stepflow-o12y deployment/jaeger --tail=100

# Promtail logs (DaemonSet)
kubectl logs -n stepflow-o12y -l app=promtail --tail=50

# Loki logs
kubectl logs -n stepflow-o12y statefulset/loki --tail=100
```

### Filtered Logs

```bash
# Search for errors
kubectl logs -n stepflow deployment/stepflow-server --tail=500 | grep -i error

# JSON log parsing (requires jq)
kubectl logs -n stepflow deployment/stepflow-server --tail=100 | jq -r '.message'

# Logs since time
kubectl logs -n stepflow deployment/stepflow-server --since=10m
```

## Port Forwarding

### Application Services

```bash
# Stepflow API (primary)
kubectl port-forward -n stepflow svc/stepflow-server 7840:7840

# Load balancer (direct access)
kubectl port-forward -n stepflow svc/stepflow-load-balancer 8080:8080

# OpenSearch
kubectl port-forward -n stepflow svc/opensearch 9200:9200
```

### Observability Services

```bash
# Grafana (dashboards)
kubectl port-forward -n stepflow-o12y svc/grafana 3000:3000

# Jaeger (traces)
kubectl port-forward -n stepflow-o12y svc/jaeger 16686:16686

# Prometheus (metrics)
kubectl port-forward -n stepflow-o12y svc/prometheus 9090:9090

# Loki (logs API)
kubectl port-forward -n stepflow-o12y svc/loki 3100:3100

# OTel Collector (for debugging)
kubectl port-forward -n stepflow-o12y svc/otel-collector 4317:4317
```

### Background Port Forwarding

```bash
# Run all port forwards in background
kubectl port-forward -n stepflow svc/stepflow-server 7840:7840 &
kubectl port-forward -n stepflow-o12y svc/grafana 3000:3000 &
kubectl port-forward -n stepflow-o12y svc/jaeger 16686:16686 &
kubectl port-forward -n stepflow-o12y svc/prometheus 9090:9090 &

# Kill all port forwards
pkill -f "kubectl port-forward"
```

## Services

```bash
# List all services
kubectl get svc -n stepflow
kubectl get svc -n stepflow-o12y

# Get service endpoints
kubectl get endpoints -n stepflow
kubectl get endpoints -n stepflow-o12y
```

## Secrets

```bash
# Create secrets (do NOT commit to git)
kubectl create secret generic stepflow-secrets \
  --namespace=stepflow \
  --from-literal=openai-api-key="sk-..." \
  --from-literal=opensearch-password="..."

# View secret (base64 encoded)
kubectl get secret stepflow-secrets -n stepflow -o yaml

# Decode a secret value
kubectl get secret stepflow-secrets -n stepflow -o jsonpath='{.data.openai-api-key}' | base64 -d

# Delete and recreate secret
kubectl delete secret stepflow-secrets -n stepflow
kubectl create secret generic stepflow-secrets --namespace=stepflow ...
```

## ConfigMaps

```bash
# List configmaps
kubectl get configmaps -n stepflow
kubectl get configmaps -n stepflow-o12y

# View configmap content
kubectl get configmap stepflow-server-config -n stepflow -o yaml
kubectl get configmap otel-collector-config -n stepflow-o12y -o yaml

# Edit configmap (triggers restart needed)
kubectl edit configmap stepflow-server-config -n stepflow
```

## Image Management

### Build Images

```bash
# From repository root
podman build -t stepflow-server:latest -f docker/Dockerfile.server .
podman build -t stepflow-load-balancer:latest -f docker/Dockerfile.loadbalancer .
podman build -t langflow-worker:latest -f docker/langflow-worker/Dockerfile .
```

### Load into Kind

```bash
kind load docker-image stepflow-server:latest --name stepflow
kind load docker-image stepflow-load-balancer:latest --name stepflow
kind load docker-image langflow-worker:latest --name stepflow
```

### Verify Images in Kind

```bash
# List images in Kind node
docker exec stepflow-control-plane crictl images | grep -E "(stepflow|langflow)"
```

### Rebuild and Reload

```bash
# Full rebuild cycle for stepflow-server
podman build -t stepflow-server:latest -f docker/Dockerfile.server . && \
kind load docker-image stepflow-server:latest --name stepflow && \
kubectl rollout restart deployment/stepflow-server -n stepflow
```

## Troubleshooting

### Pod Not Starting

```bash
# Check events
kubectl describe pod -n stepflow <pod-name>

# Check image pull status
kubectl get pods -n stepflow -o jsonpath='{.items[*].status.containerStatuses[*].state}'

# Common issues:
# - ErrImageNeverPull: Image not loaded into Kind
# - CrashLoopBackOff: Check logs for errors
# - Pending: Check resources/scheduling
```

### Network Issues

```bash
# Test DNS resolution from a pod
kubectl exec -n stepflow deployment/stepflow-server -- nslookup opensearch.stepflow.svc.cluster.local

# Test connectivity to service
kubectl exec -n stepflow deployment/stepflow-server -- curl -s http://opensearch.stepflow.svc.cluster.local:9200

# Test OTel Collector connectivity
kubectl exec -n stepflow deployment/stepflow-server -- nc -zv otel-collector.stepflow-o12y.svc.cluster.local 4317
```

### Resource Issues

```bash
# Check node resources
kubectl describe node stepflow-control-plane

# Check pod resource usage
kubectl top pods -n stepflow
kubectl top pods -n stepflow-o12y

# Check PVC status
kubectl get pvc -n stepflow-o12y
```

### Exec into Pod

```bash
# Shell into stepflow-server
kubectl exec -it -n stepflow deployment/stepflow-server -- /bin/bash

# Shell into worker
kubectl exec -it -n stepflow deployment/langflow-worker -- /bin/bash

# Run single command
kubectl exec -n stepflow deployment/stepflow-server -- env | grep OTEL
```

## Health Checks

### Service Health

```bash
# Stepflow API
curl -s http://localhost:7840/swagger-ui/ | head -5

# Grafana
curl -s http://localhost:3000/api/health | jq

# Prometheus
curl -s http://localhost:9090/-/healthy

# Jaeger
curl -s http://localhost:16686/ | head -5

# OpenSearch (via port-forward)
kubectl port-forward -n stepflow svc/opensearch 9200:9200 &
curl -s http://localhost:9200/_cluster/health | jq
```

### Kubernetes Health

```bash
# Check all deployments ready
kubectl get deployments -n stepflow
kubectl get deployments -n stepflow-o12y

# Wait for deployment
kubectl rollout status deployment/stepflow-server -n stepflow --timeout=120s
```

## Useful Aliases

Add to your shell profile (`~/.bashrc` or `~/.zshrc`):

```bash
# Namespace shortcuts
alias ks='kubectl -n stepflow'
alias ko='kubectl -n stepflow-o12y'

# Quick pod list
alias ksp='kubectl get pods -n stepflow'
alias kop='kubectl get pods -n stepflow-o12y'

# Quick logs
alias ksl='kubectl logs -n stepflow'
alias kol='kubectl logs -n stepflow-o12y'

# Port forward shortcuts
alias pf-stepflow='kubectl port-forward -n stepflow svc/stepflow-server 7840:7840'
alias pf-grafana='kubectl port-forward -n stepflow-o12y svc/grafana 3000:3000'
alias pf-jaeger='kubectl port-forward -n stepflow-o12y svc/jaeger 16686:16686'
```
