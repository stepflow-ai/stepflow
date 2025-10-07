# Testing Guide for Kubernetes Batch Demo

This guide walks through testing the complete Kubernetes batch execution demo with Pingora load balancer and Stepflow runtime.

## Prerequisites

- Lima + k3s cluster running (see [docs/SETUP.md](docs/SETUP.md))
- kubectl configured

## Quick Start (Recommended)

For first-time setup, use the automated deployment script:

```bash
cd examples/kubernetes-batch-demo

# 1. Start k3s cluster
./scripts/start-lima-k3s.sh

# 2. Configure kubectl
./scripts/setup-kubectl.sh
export KUBECONFIG=$(pwd)/kubeconfig

# 3. Deploy everything (builds images + deploys to k8s)
./scripts/deploy-all.sh

# 4. Verify deployment
kubectl get pods -n stepflow-demo
```

**Expected output:**
```
NAME                                READY   STATUS    RESTARTS   AGE
component-server-xxx-xxx            1/1     Running   0          30s
component-server-xxx-yyy            1/1     Running   0          30s
component-server-xxx-zzz            1/1     Running   0          30s
pingora-lb-xxx-xxx                  1/1     Running   0          30s
pingora-lb-xxx-yyy                  1/1     Running   0          30s
stepflow-server-xxx-xxx             1/1     Running   0          30s
```

**Time:** ~10-15 minutes for first run (includes image builds and k3s startup)

---

## Setup Steps (Manual)

<details>
<summary>Click to expand manual setup steps (alternative to Quick Start)</summary>

### 1. Start Lima VM with k3s

```bash
cd examples/kubernetes-batch-demo
./scripts/start-lima-k3s.sh
```

Wait for VM to be ready (~5-10 minutes on first run).

### 2. Configure kubectl

```bash
./scripts/setup-kubectl.sh
export KUBECONFIG=$(pwd)/kubeconfig
```

Verify cluster access:
```bash
kubectl get nodes
```

### 3. Build Docker Images

Build all images:
```bash
./scripts/build-and-push.sh          # Component server
./scripts/build-pingora.sh           # Pingora load balancer
./scripts/build-stepflow-server.sh   # Stepflow runtime server
```

This takes 5-10 minutes total (Rust compilation in multi-stage builds).

### 4. Deploy to Kubernetes

Deploy all services:
```bash
kubectl apply -f k8s/namespace.yaml
kubectl apply -k k8s/component-server/
kubectl apply -k k8s/pingora-lb/
kubectl apply -k k8s/stepflow-server/
```

Wait for pods to be ready:
```bash
kubectl wait --for=condition=Ready pods -l app=component-server -n stepflow-demo --timeout=120s
kubectl wait --for=condition=Ready pods -l app=pingora-lb -n stepflow-demo --timeout=120s
kubectl wait --for=condition=Ready pods -l app=stepflow-server -n stepflow-demo --timeout=120s
```

Verify deployment:
```bash
kubectl get pods -n stepflow-demo
```

</details>

## Testing the System

### Quick Test (Recommended)

Run the complete test suite with one command:

```bash
# In terminal 1: Start port-forward to Stepflow server
./scripts/start-port-forward.sh stepflow

# In terminal 2: Run all tests
./workflows/test-workflows.sh
```

**What this tests:**
- ✅ 10 simple workflows (load distribution)
- ✅ 5 bidirectional workflows (blob storage + instance affinity)
- ✅ 5 parallel workflows (concurrent execution)
- ✅ Total: 20 workflow executions with 35 component calls

**Expected output:**
```
🚀 Testing workflows via Stepflow server...
Server URL: http://localhost:7840/api/v1

📝 Test 1: Simple workflows (10 sequential submissions)
  ✅ Success (for all 10 workflows)

📝 Test 2: Bidirectional workflows (5 sequential submissions)
  ✅ Success (for all 5 workflows)

📝 Test 3: Parallel bidirectional workflows (5 workflows, 3 parallel components each)
  ✅ Success (for all 5 workflows)

📊 Analyzing execution distribution...
Component execution distribution:
  component-server-xxx: 12 executions
  component-server-yyy: 11 executions
  component-server-zzz: 12 executions

Pingora routing decisions:
  10.42.0.41:8080: 23 requests
  10.42.0.42:8080: 22 requests
  10.42.0.43:8080: 25 requests

✅ Workflow tests complete!
```

**Time:** ~30 seconds to run all tests

## Debugging

### Check Component Server Health

```bash
# Via k8s Service
kubectl run -it --rm debug --image=curlimages/curl --restart=Never -- \
  curl http://component-server.stepflow-demo.svc.cluster.local:8080/health

# Via Pingora
kubectl run -it --rm debug --image=curlimages/curl --restart=Never -- \
  curl http://pingora-lb.stepflow-demo.svc.cluster.local:8080/health

# Via port-forward to Pingora (requires start-pingora-port-forward.sh)
curl http://localhost:8080/health
```

### Check Pingora Backend Discovery

```bash
kubectl exec -n stepflow-demo -it deployment/pingora-lb -- sh
# Inside pod:
curl http://component-server.stepflow-demo.svc.cluster.local:8080/health
```

### Inspect Traffic

```bash
# Component server logs
kubectl logs -n stepflow-demo -l app=component-server -f

# Pingora logs
kubectl logs -n stepflow-demo -l app=pingora-lb -f

# Follow specific pod
kubectl logs -n stepflow-demo component-server-abc-12345678 -f
```

### Check Instance IDs

```bash
# Get all instance IDs
kubectl get pods -n stepflow-demo -l app=component-server -o json | \
  jq -r '.items[] | .status.podIP + " " + .metadata.name' | \
  while read ip name; do
    echo "$name: $(curl -s http://$ip:8080/health | jq -r .instanceId)"
  done
```

### Verify Stepflow-Instance-Id Headers

Add debug to Pingora logs:
```bash
# Edit pingora deployment to set RUST_LOG=debug
kubectl set env deployment/pingora-lb -n stepflow-demo RUST_LOG=debug

# Restart to apply
kubectl rollout restart deployment/pingora-lb -n stepflow-demo

# Check logs for instance ID headers
kubectl logs -n stepflow-demo -l app=pingora-lb --tail=100 | grep "Instance ID header"
```

## Common Issues

### Pingora can't discover backends
- Check DNS: `nslookup component-server.stepflow-demo.svc.cluster.local`
- Check health endpoint: Pod may not be ready
- Check Pingora logs for health check failures

### 503 Instance Not Available
- Instance ID mismatch (pod restarted)
- Pingora hasn't discovered new backend yet (wait 10s)
- Client cached old instance ID

### SSE Stream Buffering
- Check response headers include `X-Accel-Buffering: no`
- Verify Pingora isn't buffering (check logs for "SSE stream detected")

### Uneven Load Distribution
- Check connection counts in Pingora logs
- Verify least-connections algorithm is working
- Long-lived connections may cause imbalance

## Next Steps

After successful testing:
1. Add HPA for auto-scaling (Phase 6)
2. Complete architecture documentation (Phase 7)
3. Create production deployment guide
4. Add monitoring and metrics
