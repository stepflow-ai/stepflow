# Kubernetes Batch Demo Architecture

This document describes the architecture of the Kubernetes batch execution demo.

## Overview

The demo showcases distributed component execution in Kubernetes while maintaining a centralized orchestration runtime. This architecture balances simplicity with scalability by keeping the Stepflow runtime as a single instance while distributing component execution across multiple pods.

## Components

### 1. Stepflow Runtime Server

**Purpose**: Centralized workflow orchestration and execution engine

**Deployment**:
- Single pod deployment (no horizontal scaling)
- In-memory state store (sufficient for demo)
- Kubernetes service: `stepflow-server.stepflow-demo.svc.cluster.local:7840`

**Configuration**:
- Routes `/python/*` components to Load Balancer load balancer
- Routes other components to built-in plugins
- No persistent storage (workflows are stateless in this demo)

**API Endpoints**:
- `POST /api/v1/runs` - Submit workflow execution
- `GET /api/v1/runs/{execution_id}` - Get execution status
- `GET /health` - Health check

**Why Single Instance?**
- Simplifies state management (no distributed consensus)
- Sufficient for batch workloads with moderate concurrency
- Easy to scale component servers independently
- Future enhancement: Add PostgreSQL for persistence and multiple instances

### 2. Component Servers (Python HTTP)

**Purpose**: Execute workflow components with bidirectional communication support

**Deployment**:
- 3 replicas (horizontally scalable)
- Kubernetes headless service: `component-server.stepflow-demo.svc.cluster.local:8080`
- Each pod gets unique instance ID from pod name

**Components Provided**:
- `/python/double` - Simple math component (multiplies input by 2)
- `/python/store_and_retrieve_blob` - Bidirectional blob storage component
- Additional user-defined components

**Features**:
- HTTP JSON-RPC with Server-Sent Events (SSE) for bidirectional communication
- Sends `Stepflow-Instance-Id` header in SSE responses
- Supports `StepflowContext` for runtime calls (blob storage, etc.)
- Health endpoint at `/health`

**Bidirectional Communication**:
1. Stepflow runtime sends component execution request
2. Component server responds with SSE stream including `Stepflow-Instance-Id` header
3. During execution, component can call back to runtime (e.g., `put_blob`, `get_blob`)
4. Runtime includes `Stepflow-Instance-Id` header in responses
5. Load Balancer routes responses back to the correct pod instance

### 3. Load Balancer Load Balancer

**Purpose**: Intelligent HTTP load balancing with SSE support and instance affinity

**Deployment**:
- 2 replicas for high availability
- Kubernetes service: `stepflow-load-balancer.stepflow-demo.svc.cluster.local:8080`

**Features**:
- Backend discovery via DNS (queries headless service)
- Health checking of component server pods
- Round-robin + least-connections load balancing
- SSE stream detection and preservation
- Instance affinity routing via `Stepflow-Instance-Id` header

**Routing Logic**:
```
Initial Request (no instance ID) → Round-robin to backend
├─ Backend returns SSE with Stepflow-Instance-Id: pod-A
└─ Runtime stores instance ID for this component execution

Bidirectional Response (with instance ID) → Route to specific backend
├─ Extract Stepflow-Instance-Id: pod-A from request header
└─ Forward to pod-A specifically (maintains affinity)
```

**Why Load Balancer?**
- Native SSE streaming support
- Low latency (async Rust-based)
- Fine-grained control over routing logic
- Better than NGINX for SSE workloads

### 4. Lima VM with k3s

**Purpose**: Local Kubernetes cluster on macOS

**Features**:
- VZ or QEMU backend (automatic detection)
- Local Docker registry at localhost:5000
- Port forwarding: 6443 (k8s API), 8080 (HTTP), 5000 (registry)
- Project mounting at `/home/lima.linux/stepflow`

**Why k3s?**
- Lightweight (single binary)
- Fast startup
- Full Kubernetes API compatibility
- Built-in local storage provider

## Data Flow

### Simple Component Execution

```
User → Stepflow Runtime → Load Balancer LB → Component Server
                             ↓
                    (round-robin selection)
                             ↓
                    Component Server Pod-A
                             ↓
                    Execute /python/double
                             ↓
                    Return result ← ← ← ←
```

### Bidirectional Component Execution

```
1. Initial Request
User → Stepflow Runtime → Load Balancer LB → Component Server Pod-A
                             ↓
                    (round-robin selection)
                             ↓
                    Start SSE stream with
                    Stepflow-Instance-Id: pod-A
                             ↓
2. Component Calls Runtime
Component needs blob storage
         ↓
    put_blob request
         ↓
Runtime receives SSE event
         ↓
Runtime processes put_blob
         ↓
Runtime sends response with
Stepflow-Instance-Id: pod-A
         ↓
3. Instance Affinity Routing
Load Balancer receives response
         ↓
Extract Stepflow-Instance-Id: pod-A
         ↓
Route specifically to Pod-A
         ↓
Pod-A matches request ID
         ↓
Blob storage succeeds ✓
```

## Network Topology

```
┌─────────────────────────────────────────────┐
│               Lima VM (k3s)                 │
│                                             │
│  ┌────────────────────────────────────┐    │
│  │  stepflow-server (1 pod)           │    │
│  │  Port: 7840                        │    │
│  │  State: in-memory                  │    │
│  └────────────┬───────────────────────┘    │
│               │                             │
│               │ Routes /python/* to         │
│               ↓                             │
│  ┌────────────────────────────────────┐    │
│  │  stepflow-load-balancer (2 pods)               │    │
│  │  Port: 8080                        │    │
│  │  Load balancing + Instance routing │    │
│  └────────────┬───────────────────────┘    │
│               │                             │
│               │ Distributes to              │
│               ↓                             │
│  ┌────────────────────────────────────┐    │
│  │  component-server (3 pods)         │    │
│  │  Port: 8080                        │    │
│  │  - Pod-A (10.42.0.41)              │    │
│  │  - Pod-B (10.42.0.42)              │    │
│  │  - Pod-C (10.42.0.43)              │    │
│  └────────────────────────────────────┘    │
│                                             │
└─────────────────────────────────────────────┘
                │
                │ Port-forward 7840 → 7840
                ↓
         ┌──────────────┐
         │  Mac localhost │
         │  :7840         │
         └──────────────┘
```

## Scaling Characteristics

### What Scales

✅ **Component Servers**: Horizontally scalable via k8s replicas
- Add more pods to handle more concurrent component executions
- Load Balancer automatically discovers new backends
- No configuration changes needed

✅ **Load Balancer Load Balancer**: Can add more replicas
- Each replica does independent backend discovery
- Service load-balances across Load Balancer instances

### What Doesn't Scale (By Design)

❌ **Stepflow Runtime**: Single instance
- Uses in-memory state (not shared across pods)
- Simplifies architecture for batch demo
- Sufficient for moderate concurrency

❌ **State Storage**: Ephemeral
- No persistent workflow state
- Workflows complete and results are retrieved immediately

### Future Scaling Enhancements

For production workloads, consider:
1. **PostgreSQL State Store**: Enable persistent workflow state
2. **Multiple Stepflow Runtimes**: Scale orchestration layer
3. **StatefulSets**: For component servers with local state
4. **Horizontal Pod Autoscaler (HPA)**: Auto-scale based on CPU/memory

## Configuration Files

### Stepflow Runtime Config
```yaml
plugins:
  builtin:
    type: builtin
  k8s_components:
    type: stepflow
    url: "http://stepflow-load-balancer.stepflow-demo.svc.cluster.local:8080"

routes:
  "/python/{*component}":
    - plugin: k8s_components
  "/{*component}":
    - plugin: builtin

storageConfig:
  type: inMemory
```

### Kubernetes Resources
- **Namespace**: `stepflow-demo`
- **Component Server**: 3 replicas, headless service
- **Load Balancer LB**: 2 replicas, ClusterIP service
- **Stepflow Server**: 1 replica, ClusterIP service

## Security Considerations

### Current Setup (Development)
- ⚠️ No authentication on Stepflow API
- ⚠️ No TLS between services
- ⚠️ No network policies
- ⚠️ Cluster admin credentials in kubeconfig

### Production Recommendations
- ✅ Add API authentication (OAuth2, JWT)
- ✅ Enable mTLS between services (service mesh)
- ✅ Implement network policies (zero-trust)
- ✅ Use RBAC for least-privilege access
- ✅ Store secrets in Kubernetes Secrets or external vault

## Performance Characteristics

### Latency
- Component execution: ~5-10ms overhead (Load Balancer + network)
- Bidirectional requests: ~2-5ms per runtime call
- Total workflow latency: depends on component complexity

### Throughput
- Component servers: ~100 req/sec per pod (Python HTTP)
- Load Balancer LB: ~10,000 req/sec per pod (Rust async)
- Stepflow runtime: ~50 concurrent workflows (single instance, in-memory)

### Resource Usage
- Component server pod: 100m CPU, 256Mi RAM
- Load Balancer LB pod: 100m CPU, 128Mi RAM
- Stepflow runtime pod: 100m CPU, 256Mi RAM

## Comparison with Other Architectures

### vs. Local Stepflow + Port-forward to Component Server
**Previous approach**: Stepflow runs on Mac, port-forward to component server service

**Current approach**: Stepflow runs in k8s, routes through Load Balancer

**Advantages**:
- ✅ More realistic production setup
- ✅ Stepflow server can be accessed by multiple clients
- ✅ Easier to test scaling behavior
- ✅ No need to keep port-forward running

**Trade-offs**:
- Slightly more complex deployment
- Need to build and push Stepflow server image

### vs. Fully Distributed Stepflow
**Alternative**: Multiple Stepflow runtime instances with PostgreSQL

**Current simplification**:
- Single Stepflow instance
- In-memory state store
- No distributed consensus

**When to use each**:
- **Current (single instance)**: Batch workloads, moderate concurrency, simple demos
- **Distributed**: High availability, high concurrency, production workloads

## Debugging Guide

### View Stepflow Server Logs
```bash
kubectl logs -n stepflow-demo -l app=stepflow-server -f
```

### View Component Server Logs
```bash
# All component servers
kubectl logs -n stepflow-demo -l app=component-server --tail=100

# Specific pod
kubectl logs -n stepflow-demo component-server-xxx-xxx -f
```

### View Load Balancer Logs
```bash
kubectl logs -n stepflow-demo -l app=stepflow-load-balancer -f
```

### Check Service Endpoints
```bash
# Component server endpoints (should show 3 pod IPs)
kubectl get endpoints -n stepflow-demo component-server

# Load Balancer endpoints (should show 2 pod IPs)
kubectl get endpoints -n stepflow-demo stepflow-load-balancer
```

### Test Component Server Directly
```bash
# From within cluster
kubectl run -it --rm debug --image=curlimages/curl --restart=Never -- \
  curl http://component-server.stepflow-demo.svc.cluster.local:8080/health
```

### Test Load Balancer Load Balancer
```bash
# From within cluster
kubectl run -it --rm debug --image=curlimages/curl --restart=Never -- \
  curl http://stepflow-load-balancer.stepflow-demo.svc.cluster.local:8080/health
```s