# Stepflow Kubernetes Deployment

This directory contains Kubernetes manifests for deploying Stepflow and its supporting infrastructure on a local Kind cluster.

## Overview

The Stepflow K8s deployment provides a complete local development environment with:

- **Stepflow Server**: Core workflow orchestration engine
- **Load Balancer**: Pingora-based request routing to component servers
- **Langflow Component Server**: Python-based component execution (3 replicas)
- **OpenSearch**: Document storage and vector search
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
                                    │  │Component││Component││Component│ │
                                    │  │Server 1 ││Server 2 ││Server 3 │ │
                                    │  └─────────┘└─────────┘└─────────┘ │
                                    │                 │                   │
                                    │                 ▼                   │
                                    │  ┌─────────────────────────────┐   │
                                    │  │        OpenSearch           │   │
                                    │  │   (documents, vectors)      │   │
                                    │  └─────────────────────────────┘   │
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
podman build -t langflow-component-server:latest -f docker/langflow-component-server/Dockerfile .
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
kind load docker-image langflow-component-server:latest --name stepflow
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

## Namespaces

| Namespace | Purpose |
|-----------|---------|
| `stepflow` | Application workloads (server, load balancer, components, OpenSearch) |
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
├── stepflow/                      # Application manifests
│   ├── server/                    # Stepflow server
│   ├── loadbalancer/              # Pingora load balancer
│   ├── langflow-component-server/ # Python component servers
│   └── opensearch/                # OpenSearch
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
