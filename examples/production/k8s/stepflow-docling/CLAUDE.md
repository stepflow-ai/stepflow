# Stepflow-Docling Parity Testbed

Self-contained K8s deployment that presents a docling-serve compatible HTTP API
backed by Stepflow orchestration. The facade translates docling-serve v1 requests
into Stepflow flow submissions; the worker runs docling's `DocumentConverter`
in-process (no sidecar). See `docs/proposals/docling-step-worker.md` for the
full design and `#683` for the tracking issue.

## Architecture

```
localhost:5001 (NodePort 30501 or port-forward)
      │
      ▼
┌─────────────────────────────────────────┐
│  docling-orchestrator pod               │
│  ├─ docling-facade (port 5001)          │  FastAPI, translates HTTP → Stepflow
│  ├─ stepflow-server (port 7840)         │  Rust orchestrator, executes flows
│  └─ init: wait-for-worker               │  Blocks until LB/worker healthy
└─────────────┬───────────────────────────┘
              │ routes /docling/* via config
              ▼
┌─────────────────────────────────────────┐
│  docling-lb pod (port 8080)             │  Least-connections load balancer
│  Discovers workers via headless service │
└─────────────┬───────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────┐
│  docling-worker pod(s) (port 8080)      │  stepflow-py HTTP component server
│  Components: /classify, /convert, /chunk│  docling library in-process
│  Base image: docling-serve-cpu:v1.14.0  │  Models pre-loaded
└─────────────────────────────────────────┘
```

All pods export traces, metrics, and logs to the shared `stepflow-o11y` namespace
(OTel Collector → Jaeger/Prometheus/Loki/Grafana).

## Namespaces

- `stepflow-docling` — application pods (orchestrator, LB, worker)
- `stepflow-o11y` — shared observability stack

## Prerequisites

- Kind cluster with port mappings (see below)
- Docker (for building images)
- kubectl configured to the cluster
- Python 3.10+ (for the test script)
- Rust toolchain (only if rebuilding stepflow-server from source)

## Protocol Compatibility

The stepflow-server and stepflow-py versions must be compatible. The `initialize`
handshake includes `runtimeProtocolVersion` (added in 0.10.0). Mixing 0.9.0
server with 0.10.0 worker causes `Object missing required field` errors.

| Component | Image | Protocol |
|-----------|-------|----------|
| stepflow-server | `localhost/stepflow-server:alpine-0.10.0` | v1 |
| stepflow-load-balancer | `ghcr.io/.../stepflow-load-balancer:alpine-0.9.0` | HTTP proxy (version-agnostic) |
| docling-worker | `localhost/stepflow-docling-worker:parity-v1` | stepflow-py 0.10.0 |
| docling-facade | `localhost/docling-facade:parity-v1` | HTTP client only |

The load balancer is a transparent HTTP proxy and does not participate in
protocol negotiation. It can run at any version.

## Build Commands (in order)

All commands run from the repository root.

### 1. Build stepflow-server from source (Rust, ~5 min)

Required because the published 0.9.0 images are protocol-incompatible with
stepflow-py 0.10.0. Build a static musl binary, then package into Alpine.

```bash
# Build the binary
cd stepflow-rs
cargo build --release --target x86_64-unknown-linux-musl --bin stepflow-server

# Package into Alpine image
cp target/x86_64-unknown-linux-musl/release/stepflow-server release/
docker build -f release/Dockerfile.stepflow-server.alpine \
  -t localhost/stepflow-server:alpine-0.10.0 release/
```

Alternative: use the multi-stage Debian Dockerfile (slower, larger image):

```bash
docker build -f docker/Dockerfile.server -t localhost/stepflow-server:alpine-0.10.0 .
```

### 2. Build docling-facade image (~10 sec)

Lightweight Python image with FastAPI, httpx, pyyaml. No docling library.

```bash
cd integrations/docling-step-worker
docker build --load -f docker/Dockerfile.facade -t localhost/docling-facade:parity-v1 .
```

### 3. Build docling-worker image (~2 min)

Based on `docling-serve-cpu:v1.14.0` (4GB, includes models). Adds stepflow-py
and the worker source code.

```bash
cd integrations/docling-step-worker
docker build --load -f docker/Dockerfile -t localhost/stepflow-docling-worker:parity-v1 .
```

### 4. Load images into Kind

```bash
kind load docker-image localhost/stepflow-server:alpine-0.10.0 --name stepflow
kind load docker-image localhost/docling-facade:parity-v1 --name stepflow
kind load docker-image localhost/stepflow-docling-worker:parity-v1 --name stepflow
```

Adjust `--name` to match your Kind cluster name.

## Kind Cluster Setup

### Option A: Dedicated cluster with NodePort mappings

```bash
kind create cluster --config kind-config.yaml
```

This creates a `stepflow-docling` cluster with ports:
- `localhost:5001` → NodePort 30501 (facade)
- `localhost:3000` → NodePort 30001 (Grafana)
- `localhost:16686` → NodePort 30686 (Jaeger)
- `localhost:9090` → NodePort 30090 (Prometheus)

### Option B: Existing cluster with port-forward

If using an existing Kind cluster without the port mappings:

```bash
kubectl port-forward -n stepflow-docling svc/docling-orchestrator 5001:5001
```

## Deploy

```bash
cd examples/production/k8s/stepflow-docling
./apply.sh
```

The script deploys in dependency order:
1. Namespaces (`stepflow-docling`, `stepflow-o11y`)
2. Observability stack (OTel Collector, Jaeger, Prometheus, Loki, Promtail, Grafana)
3. Workers (must be discoverable before LB starts)
4. Load balancer
5. Orchestrator (facade + stepflow-server, with init container that waits for LB)

Verify all pods are healthy:

```bash
kubectl get pods -n stepflow-docling
kubectl get pods -n stepflow-o11y
```

Expected: orchestrator 2/2, worker 1/1, LB 1/1, all o11y pods 1/1.

## Teardown

```bash
cd examples/production/k8s/stepflow-docling
./teardown.sh
```

Removes resources in reverse dependency order. Prompts for confirmation.

## Test Script

The parity test exercises the deployed facade against the docling-serve v1 API
contract using the Docling paper from arXiv (https://arxiv.org/pdf/2501.17887).

```bash
python test-parity.py [--base-url URL] [--timeout SECONDS]
```

Defaults: `--base-url http://localhost:5001`, `--timeout 300`.

Uses httpx if available, falls back to urllib. No other dependencies.

### Test cases (6 total)

| # | Test | What it validates |
|---|------|-------------------|
| 1 | Health check | Facade + server reachable, retries for 60s |
| 2 | Convert URL source (v1) | `POST /v1/convert/source` with arXiv URL, checks md_content contains "Docling" |
| 3 | Convert file upload (v1) | `POST /v1/convert/file` multipart upload of same PDF |
| 4 | v1alpha backward compat | `POST /v1alpha/convert/source` with `http_sources` shape |
| 5 | Options passthrough | `do_ocr: true, table_mode: accurate` accepted without error |
| 6 | Error handling | Invalid URL returns structured error (4xx/5xx with JSON body) |

### Interpreting results

- **Test 2 is the critical path**: facade → stepflow-server → classify → convert → chunk → response
- First run is slow (~30s) due to model loading; subsequent runs use the LRU converter cache (~10s)
- If test 2 fails with `$input` errors, check that `translate.py` populates all optional flow input fields

## Flow Definition

The flow `docling-process-document` runs three steps sequentially:

```
classify → convert → chunk
```

Defined in `integrations/docling-step-worker/flows/docling-process-document.yaml`
and duplicated in the ConfigMap at `orchestrator/configmap.yaml`. The facade
registers the flow with stepflow-server at startup and receives a `flowId` for
subsequent submissions.

**Important**: All `$input` references in the flow must resolve. The facade's
`translate.py` must always include every referenced field in the flow input,
using defaults for optional fields (`to_formats: ["md"]`,
`image_export_mode: "embedded"`, `chunk_options: null`, `options: null`).

All YAML description values containing colons must be quoted (e.g.,
`description: "Image reference mode (default: embedded)"`).

## Debugging

### Pod logs

```bash
# Stepflow server (protocol, routing, execution)
kubectl logs -n stepflow-docling deployment/docling-orchestrator -c stepflow-server

# Facade (HTTP requests, flow registration)
kubectl logs -n stepflow-docling deployment/docling-orchestrator -c docling-facade

# Worker (component execution, docling conversion)
kubectl logs -n stepflow-docling deployment/docling-worker

# Load balancer (backend discovery, health checks)
kubectl logs -n stepflow-docling deployment/docling-lb
```

### Common issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| Server CrashLoopBackOff with `missing field runtimeProtocolVersion` | Server 0.9.0 / worker 0.10.0 mismatch | Rebuild server from source at 0.10.0 |
| Facade crash with `mapping values are not allowed here` | Unquoted YAML string with `:` in flow | Quote all description values |
| `Input path $.image_export_mode not found` | Optional field missing from flow input | Ensure translate.py sets all fields |
| Init container stuck at "waiting..." | Worker or LB not ready | Check worker/LB pod status and logs |
| Worker `permission denied` on model files | Wrong `runAsUser` for base image | Use `runAsUser: 1001` (matches docling-serve-cpu) |

### Observability access

| Tool | URL | Credentials |
|------|-----|-------------|
| Grafana | http://localhost:3000 | admin / admin |
| Jaeger | http://localhost:16686 | — |
| Prometheus | http://localhost:9090 | — |

Requires NodePort mappings (Option A) or additional port-forwards.

## Files

```
stepflow-docling/
├── CLAUDE.md                          # This file
├── apply.sh                           # Deploy script (dependency-ordered)
├── teardown.sh                        # Teardown script (reverse order)
├── kind-config.yaml                   # Kind cluster with port mappings
├── test-parity.py                     # E2e parity test (6 tests)
├── namespaces.yaml                    # stepflow-docling + stepflow-o11y
├── orchestrator/
│   ├── configmap.yaml                 # stepflow-config.yml + flow YAML
│   ├── deployment.yaml                # facade + server + init container
│   └── service.yaml                   # NodePort 30501 → facade 5001
├── loadbalancer/
│   ├── deployment.yaml                # Least-connections LB
│   └── service.yaml                   # ClusterIP 8080
└── worker/
    ├── deployment.yaml                # docling-step-worker pod
    ├── service.yaml                   # ClusterIP 8080
    └── service-headless.yaml          # Headless for LB discovery
```
