# Stepflow Kubernetes Deployment

This directory contains Kubernetes manifests for deploying Stepflow and its supporting infrastructure on a local Kind cluster.

## Overview

The Stepflow K8s deployment provides a complete local development environment with:

- **Stepflow Server**: Core workflow orchestration engine
- **Load Balancer**: Pingora-based request routing to workers
- **Langflow Worker**: Python-based component execution (3 replicas)
- **OpenSearch**: Document storage and vector search
- **Docling**: Document processing service (PDF, DOCX, images to structured formats)
- **Observability Stack**: Full telemetry pipeline (traces, metrics, logs)

## Architecture

```
                                    ┌─────────────────────────────────────┐
                                    │         stepflow namespace          │
┌──────────────┐                    │  ┌─────────────────────────────┐   │
│   Client     │───────────────────▶│  │     Stepflow Server         │   │
│  (localhost) │      :7840         │  │     (workflow engine)       │   │
└──────────────┘                    │  └──────────────┬──────────────┘   │
                                    │                 │                   │
                                    │                 ▼                   │
                                    │  ┌─────────────────────────────┐   │
                                    │  │      Load Balancer          │   │
                                    │  │   (Pingora, instance-aware) │   │
                                    │  └──────────────┬──────────────┘   │
                                    │                 │                   │
                                    │       ┌─────────┼─────────┐        │
                                    │       ▼         ▼         ▼        │
                                    │  ┌─────────┐┌─────────┐┌─────────┐ │
                                    │  │Langflow ││Langflow ││Langflow │ │
                                    │  │Worker 1 ││Worker 2 ││Worker 3 │ │
                                    │  └────┬────┘└────┬────┘└────┬────┘ │
                                    │       │         │          │       │
                                    │       └────┬────┴────┬─────┘       │
                                    │            ▼         ▼             │
                                    │  ┌─────────────┐ ┌─────────────┐   │
                                    │  │ OpenSearch  │ │   Docling   │   │
                                    │  │ (vectors)   │ │  (parsing)  │   │
                                    │  └─────────────┘ └─────────────┘   │
                                    └─────────────────────────────────────┘
                                                      │
                                                      │ OTLP
                                                      ▼
                                    ┌─────────────────────────────────────┐
                                    │       stepflow-o12y namespace       │
                                    │                                     │
                                    │  OTel Collector → Jaeger (traces)   │
                                    │                 → Prometheus (metrics)
                                    │                 → Loki (logs)       │
                                    │                                     │
                                    │           Grafana (UI)              │
                                    └─────────────────────────────────────┘
```

## Prerequisites

- **Kind**: Kubernetes in Docker - `brew install kind`
- **kubectl**: Kubernetes CLI - `brew install kubectl`
- **Podman** (or Docker): Container runtime - `brew install podman`

## Quick Start

### 1. Build Docker Images

From the repository root:

```bash
# Build all three images
podman build -t stepflow-server:latest -f docker/Dockerfile.server .
podman build -t stepflow-load-balancer:latest -f docker/Dockerfile.loadbalancer .
podman build -t langflow-worker:latest -f docker/langflow-worker/Dockerfile .
```

### 2. Create Kind Cluster

```bash
kind create cluster --config k8s/kind-config.yaml
```

This creates a cluster named `stepflow` with port mappings for:
- 7840 → Stepflow API
- 3000 → Grafana
- 16686 → Jaeger
- 9090 → Prometheus

### 3. Load Images into Kind

```bash
kind load docker-image stepflow-server:latest --name stepflow
kind load docker-image stepflow-load-balancer:latest --name stepflow
kind load docker-image langflow-worker:latest --name stepflow
```

### 4. Create Secrets

Create the required secrets (do not commit credentials to source control):

```bash
kubectl create secret generic stepflow-secrets \
  --namespace=stepflow \
  --from-literal=openai-api-key="YOUR_OPENAI_KEY" \
  --from-literal=opensearch-password="YOUR_PASSWORD"
```

**Note**: Run this after namespaces are created (step 5) or the apply script will create the namespace first.

### 5. Deploy Everything

```bash
./k8s/apply.sh
```

This deploys all components in the correct order with dependency waits.

### 6. Verify Deployment

```bash
kubectl get pods -n stepflow
kubectl get pods -n stepflow-o12y
```

All pods should show `Running` status.

## Access URLs

| Service | URL | Description |
|---------|-----|-------------|
| Stepflow API | http://localhost:7840 | Workflow API |
| Swagger UI | http://localhost:7840/swagger-ui/ | API documentation |
| Grafana | http://localhost:3000 | Observability dashboards (admin/admin) |
| Jaeger | http://localhost:16686 | Distributed tracing |
| Prometheus | http://localhost:9090 | Metrics |
| Docling API | Internal only | Document processing (see port-forward below) |

**Accessing Docling** (internal service):
```bash
kubectl port-forward -n stepflow svc/docling-serve 5001:5001
# Then access: http://localhost:5001/docs (API documentation)
```

## Namespaces

| Namespace | Purpose |
|-----------|---------|
| `stepflow` | Application workloads (server, load balancer, workers) and infrastructure services (OpenSearch, Docling) |
| `stepflow-o12y` | Observability stack (OTel Collector, Jaeger, Prometheus, Loki, Grafana) |

## Teardown

To remove all resources:

```bash
./k8s/teardown.sh
```

To delete the Kind cluster entirely:

```bash
kind delete cluster --name stepflow
```

## Directory Structure

```
k8s/
├── README.md                      # This file
├── OBSERVABILITY.md               # Observability stack documentation
├── COMMANDS.md                    # Command reference
├── kind-config.yaml               # Kind cluster configuration
├── namespaces.yaml                # Namespace definitions
├── apply.sh                       # Deployment script
├── teardown.sh                    # Teardown script
├── stepflow-config.yml            # Reference config (see stepflow/server/configmap.yaml)
├── stepflow/                      # Application manifests
│   ├── server/                    # Stepflow server
│   ├── loadbalancer/              # Pingora load balancer
│   ├── langflow-worker/           # Python workers
│   ├── opensearch/                # OpenSearch (infrastructure)
│   └── docling/                   # Docling document processing (infrastructure)
└── stepflow-o12y/                 # Observability manifests
    ├── otel-collector/            # OpenTelemetry Collector
    ├── jaeger/                    # Distributed tracing
    ├── prometheus/                # Metrics
    ├── loki/                      # Log aggregation
    ├── promtail/                  # Log collection
    └── grafana/                   # Dashboards
```

## Configuration

### Stepflow Server

The server configuration is stored in `k8s/stepflow/server/configmap.yaml`. Key settings:

- Routes `/langflow/*` components to the load balancer
- Routes `/builtin/*` to built-in components
- Sends telemetry to OTel Collector

### Environment Variables

All services are configured via environment variables in their deployment manifests. Sensitive values (API keys, passwords) are referenced from the `stepflow-secrets` secret.

## Infrastructure Services vs. Routed Components

This deployment distinguishes between two types of services:

### Routed Components

Components accessible via the Stepflow routing configuration (e.g., `/langflow/*`, `/builtin/*`). These are invoked by workflow steps and routed through the load balancer.

| Path Pattern | Plugin | Description |
|--------------|--------|-------------|
| `/langflow/*` | langflow_k8s | Langflow components via load balancer |
| `/builtin/*` | builtin | Built-in components (OpenAI, eval, etc.) |

### Infrastructure Services

Backend services accessed directly by workers via Kubernetes DNS. These are NOT part of the Stepflow routing configuration.

| Service | DNS Endpoint | Port | Purpose |
|---------|--------------|------|---------|
| OpenSearch | `opensearch.stepflow.svc.cluster.local` | 9200 | Document storage, vector search |
| Docling | `docling-serve.stepflow.svc.cluster.local` | 5001 | Document processing (PDF, DOCX, images) |

Workers discover these services through environment variables (e.g., `OPENSEARCH_HOST`, `DOCLING_SERVE_URL`).

## Observability

The deployment includes a full observability stack. See [OBSERVABILITY.md](OBSERVABILITY.md) for:

- Component details (OTel Collector, Jaeger, Prometheus, Loki, Grafana)
- Telemetry flow (traces, metrics, logs)
- Dashboard navigation
- Trace-to-log correlation
- Troubleshooting guides

## Command Reference

See [COMMANDS.md](COMMANDS.md) for useful commands including:

- Cluster management
- Pod operations and logs
- Port forwarding
- Troubleshooting
- Image management

## Production Considerations

This deployment is designed for local development. For production:

- Use a managed Kubernetes service (EKS, GKE, AKS)
- Configure persistent storage for OpenSearch, Prometheus, Loki
- Set up proper ingress controllers
- Configure authentication for Grafana
- Implement network policies
- Set resource limits and requests appropriately
- Configure horizontal pod autoscaling
- Set up alerting rules in Prometheus/Grafana

## Future Work

### Docling Enhancements

1. **Model Serving Strategy**: The current deployment uses pre-baked models in the container image. Future work should consider:
   - PVC-mounted model cache for updates without image rebuilds
   - Model versioning and rollback support
   - Shared model storage across replicas

2. **GPU Acceleration**: The `docling-serve-cu126` and `docling-serve-cu128` images exist for NVIDIA GPU acceleration. To enable:
   - Install NVIDIA device plugin in the cluster
   - Change image to `quay.io/docling-project/docling-serve-cu126:latest`
   - Add GPU resource requests to the deployment

3. **Horizontal Scaling**: Document processing is CPU-intensive. Consider:
   - Horizontal Pod Autoscaler (HPA) based on CPU utilization
   - Queue-based scaling for batch processing workloads
   - Pod disruption budgets for availability

4. **Direct Docling Routing**: Future work may add `/docling/{*component}` routes for direct component access from workflows, enabling Docling as a routed component rather than infrastructure service.
