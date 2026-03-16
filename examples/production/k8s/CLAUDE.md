# Kubernetes Deployment & Docker Development Guide

## Active Deployment: `stepflow-docling-grpc`

The **gRPC pull-based transport** (`stepflow-docling-grpc/`) is the current deployment architecture. Workers are gRPC clients that connect to the orchestrator's `TasksService` and pull tasks from a shared queue. This eliminates the load balancer entirely — the orchestrator manages task distribution.

> The older deployments (`stepflow-docling/`, `stepflow/`) are retained for reference but are not actively maintained. They used HTTP push-based transport with a load balancer.

### Architecture

```
                    ┌─────────────────────────────┐
                    │  docling-orchestrator pod    │
  Client ──5001──▶  │  ┌─────────────────────────┐ │
                    │  │ docling-facade (FastAPI)  │ │
                    │  │    ▼ localhost:7840       │ │
                    │  │ stepflow-server (Rust)    │ │
                    │  │  HTTP:7840 + gRPC:7837    │ │
                    │  └─────────────────────────┘ │
                    └──────────┬──────────────────┘
                               │ gRPC PullTasks
               ┌───────────────┼───────────────┐
               ▼               ▼               ▼
         ┌──────────┐   ┌──────────┐   ┌──────────┐
         │ worker-1 │   │ worker-2 │   │ worker-3 │
         │ (client) │   │ (client) │   │ (client) │
         └──────────┘   └──────────┘   └──────────┘
```

- **Namespace**: `stepflow-docling-grpc`
- **Orchestrator**: 1 pod with 2 containers (facade + stepflow-server)
- **Workers**: 3 pods, each a gRPC client with no exposed ports
- **Plugin type**: `pull` (not `stepflow`)
- **Deploy order**: Orchestrator first, then workers (workers connect to orchestrator)

### Manifest Structure

```
stepflow-docling-grpc/
├── kind-config.yaml              # Kind cluster with NodePort 30501→5001
├── namespaces.yaml               # stepflow-docling-grpc + stepflow-o11y
├── orchestrator/
│   ├── configmap.yaml            # stepflow-config.yml (pull plugin) + flow YAML
│   ├── deployment.yaml           # 2-container pod: facade + stepflow-server
│   └── service.yaml              # NodePort 30501→5001, ClusterIP 7840+7837
├── worker/
│   └── deployment.yaml           # 3 replicas, no ports, file-based probes
├── apply.sh                      # kubectl apply in correct order
├── teardown.sh                   # Clean teardown
└── test-parity.py                # Parity test against docling-serve API
```

### Key Environment Variables

**stepflow-server container:**
| Variable | Value | Purpose |
|----------|-------|---------|
| `STEPFLOW_BIND_ADDRESS` | `0.0.0.0:7837` | gRPC server binds all interfaces for external workers |
| `STEPFLOW_ORCHESTRATOR_URL` | `$(POD_IP):7837` | Advertised callback URL (pod IP, not service DNS) |
| `STEPFLOW_OTLP_ENDPOINT` | `http://otel-collector.stepflow-o11y...:4317` | OTel trace/metric export |

**Worker container:**
| Variable | Value | Purpose |
|----------|-------|---------|
| `STEPFLOW_TRANSPORT` | `grpc` | Selects pull-based gRPC transport |
| `STEPFLOW_TASKS_URL` | `docling-orchestrator...:7837` | Orchestrator gRPC endpoint |
| `STEPFLOW_QUEUE_NAME` | `docling` | Must match plugin config `queueName` |
| `STEPFLOW_BLOB_API_URL` | `http://docling-orchestrator...:7840/api/v1/blobs` | HTTP blob store for `$blob` refs |
| `STEPFLOW_MAX_CONCURRENT` | `1` | Tasks per worker pod (docling is CPU-heavy) |

### Worker Probes

gRPC workers have **no HTTP server**, so K8s probes use a file sentinel:
- The worker's `entry_point()` warms up docling models and writes `/tmp/worker-ready`
- `startupProbe`, `livenessProbe`, and `readinessProbe` all check `test -f /tmp/worker-ready`
- `startupProbe` has `failureThreshold: 30` (150s) to accommodate model loading

---

## Building Images

All three images are built from the repo root. The Kind cluster name is `stepflow`.

### 1. stepflow-server (Rust orchestrator)

Multi-stage build: `rust:latest` with musl cross-compile → `alpine:3.19` runtime.

```bash
# From repo root — copies entire repo for Cargo workspace resolution
docker build --load \
  -f stepflow-rs/release/Dockerfile.stepflow-server.build \
  -t localhost/stepflow-server:alpine-0.10.0 .

kind load docker-image localhost/stepflow-server:alpine-0.10.0 --name stepflow
```

Build takes 5-10 minutes from scratch; subsequent builds use Docker layer caching. The Dockerfile auto-detects architecture (x86_64/aarch64).

### 2. docling-facade (lightweight HTTP proxy)

Thin `python:3.12-slim` image with FastAPI + httpx. No docling library, no stepflow-py.

```bash
cd integrations/docling-proto-step-worker
docker build --load \
  -f docker/Dockerfile.facade \
  -t localhost/docling-facade:grpc-v1 .

kind load docker-image localhost/docling-facade:grpc-v1 --name stepflow
```

### 3. docling-proto-step-worker (Python gRPC worker)

Based on `quay.io/docling-project/docling-serve-cpu:v1.14.3` (~4GB, pre-loaded models). Layers `stepflow-py[http]` + gRPC deps on top.

```bash
# From repo root — copies sdks/python/stepflow-py/ and integrations/.../src/
docker build --load \
  -f integrations/docling-proto-step-worker/docker/Dockerfile \
  -t localhost/stepflow-docling-proto-worker:latest .

kind load docker-image localhost/stepflow-docling-proto-worker:latest --name stepflow
```

Build is fast (~1 min) if the base image is cached. Worker runs as UID 1001 (matches base image).

### Deploying After Image Rebuild

```bash
# Restart deployments to pick up new images (imagePullPolicy: Never)
kubectl rollout restart deployment/docling-orchestrator -n stepflow-docling-grpc
kubectl rollout restart deployment/docling-worker -n stepflow-docling-grpc

# Watch rollout
kubectl rollout status deployment/docling-orchestrator -n stepflow-docling-grpc
kubectl rollout status deployment/docling-worker -n stepflow-docling-grpc
```

### Running the Parity Test

```bash
# Port-forward the facade (or use NodePort 30501 if Kind config supports it)
kubectl port-forward -n stepflow-docling-grpc svc/docling-orchestrator 5001:5001 &

cd examples/production/k8s/stepflow-docling-grpc
python test-parity.py
```

---

## Observability Stack (`stepflow-o11y/`)

Shared across all deployments. Namespace: `stepflow-o11y`.

| Component | Purpose | Access |
|-----------|---------|--------|
| OTel Collector | Receives OTLP, exports to Prometheus/Jaeger/Loki | `:4317` (gRPC), `:4318` (HTTP) |
| Prometheus | Metrics storage, scraped from OTel Collector `:8889` | `localhost:9090` (port-forward) |
| Jaeger | Distributed traces | `localhost:16686` (port-forward) |
| Loki + Promtail | Log aggregation | Queried via Grafana |
| Grafana | Dashboards | `localhost:30001` (NodePort) or port-forward `:3000` |

### Grafana Dashboards

Dashboards are provisioned via ConfigMaps mounted into the Grafana pod:

| Dashboard | ConfigMap | Folder | Notes |
|-----------|-----------|--------|-------|
| Docling gRPC Worker | `grafana-dashboard-docling-grpc` | Stepflow | **Current** — queue health, task lifecycle, retries, traces |
| Docling Step Worker | `grafana-dashboard-docling-worker` | Stepflow | Legacy HTTP push worker |
| Stepflow | `grafana-dashboard-stepflow` | Stepflow | Core orchestrator metrics |
| Load Balancer | `grafana-dashboard-loadbalancer` | Infrastructure | Legacy LB metrics |
| Docling | `grafana-dashboard-docling` | Infrastructure | Legacy Langflow docling integration |
| Infrastructure | `grafana-dashboard-infrastructure` | Infrastructure | Cluster resource usage |

### Metrics Available for gRPC Pull Transport

**Orchestrator-side (Rust, `stepflow_` prefix):**
| Metric | Type | Description |
|--------|------|-------------|
| `stepflow_task_queue_depth` | UpDownCounter | Tasks waiting in queue |
| `stepflow_task_connected_workers` | UpDownCounter | Workers connected to queue |
| `stepflow_task_dispatched_total` | Counter | Total tasks pushed to queue |
| `stepflow_task_queue_duration_seconds` | Histogram | Time from dispatch to StartTask |
| `stepflow_task_execution_duration_seconds` | Histogram | Time from StartTask to CompleteTask |
| `stepflow_task_success_total` | Counter | Tasks completed successfully |
| `stepflow_task_failure_total` | Counter | Tasks completed with error |
| `stepflow_task_queue_timeout_total` | Counter | Tasks expired before pickup |
| `stepflow_task_execution_timeout_total` | Counter | Tasks expired during execution |
| `stepflow_step_retries_total` | Counter | Step retries by reason + component |
| `stepflow_step_retries_exhausted_total` | Counter | Steps that exhausted retry budget |

**Worker-side (Python, `stepflow_worker_` prefix):**
| Metric | Type | Description |
|--------|------|-------------|
| `stepflow_worker_tasks_pulled_total` | Counter | Tasks received from queue |
| `stepflow_worker_tasks_completed_total` | Counter | Tasks completed (labeled `outcome=success\|error`) |
| `stepflow_worker_heartbeats_sent_total` | Counter | Heartbeats sent to orchestrator |
| `stepflow_worker_connection_status` | UpDownCounter | Active pull connections (0 or 1 per worker) |

All metrics carry a `queue_name` label.

---

## Conventions

### Container Users
- Dockerfiles: Create named user `stepflow` with UID 1000 (`useradd -m -u 1000 stepflow`)
- K8s manifests: Use numeric `runAsUser: 1000` + `runAsNonRoot: true` at pod level
- Exception: docling base image uses UID 1001 — worker manifest uses `runAsUser: 1001`

### SecurityContext Tiers
- **Rust services** (stateless binaries): Full lockdown — `allowPrivilegeEscalation: false` + `readOnlyRootFilesystem: true`
- **Python workers** (need temp dirs): Partial lockdown — `allowPrivilegeEscalation: false` only
- **O11y stack**: No changes (third-party images)

### imagePullPolicy
- Locally-built images (`localhost/*`): `imagePullPolicy: Never`
- Registry images (`ghcr.io/*`, `quay.io/*`): `imagePullPolicy: IfNotPresent`

### Log Level Values
- Use **lowercase**: `debug`, `info`, `warn`, `error`, `trace`
- Parsed case-insensitively by both Rust and Python, but lowercase is canonical

### Docker Build Patterns
- Use multi-stage builds (deps → builder → runtime for Python; builder → runtime for Rust)
- Pin uv version: `COPY --from=ghcr.io/astral-sh/uv:0.6.14 /uv /bin/uv`
- Use `uv sync --frozen` for reproducible builds
- Handle `.python-version` conflicts: `rm -f .../.python-version` before `uv sync` when base image Python differs from dev pin
- Use `COPY --chown=UID:0` instead of `RUN chown -R` to avoid layer duplication

### OTEL_RESOURCE_ATTRIBUTES
All application pods should include:
```yaml
- name: OTEL_RESOURCE_ATTRIBUTES
  value: "deployment.environment=kubernetes,service.namespace=stepflow-docling-grpc,service.instance.id=$(POD_NAME),k8s.pod.name=$(POD_NAME),k8s.namespace.name=$(POD_NAMESPACE)"
```
Requires POD_NAME and POD_NAMESPACE from the downward API.

### Observability Env Vars

**Rust services** (server):
- `STEPFLOW_LOG_LEVEL`, `STEPFLOW_OTHER_LOG_LEVEL`, `STEPFLOW_LOG_FORMAT=json`
- `STEPFLOW_OTLP_ENDPOINT`, `STEPFLOW_TRACE_ENABLED`, `STEPFLOW_METRICS_ENABLED`

**Python workers** (docling-proto-step-worker):
- `STEPFLOW_LOG_LEVEL`, `STEPFLOW_SERVICE_NAME`
- `STEPFLOW_OTLP_ENDPOINT`, `STEPFLOW_TRACE_ENABLED`, `STEPFLOW_LOG_DESTINATION=stderr,otlp`
- `OTEL_PYTHON_LOGGING_AUTO_INSTRUMENTATION_ENABLED=true`

---

## Metrics Naming & Label Scoping

### OTel → Prometheus Metric Name Conventions

OTel metrics are exported through the Collector's Prometheus exporter:

- **Namespace prefix**: All metrics get the `stepflow_` prefix (set via `namespace` on the Prometheus exporter)
- **Unit suffix**: OTel units are appended. `http.server.duration` with unit `ms` → `stepflow_http_server_duration_milliseconds`
- **Histogram suffixes**: `_bucket`, `_count`, `_sum`
- **Counter suffix**: `_total`

### Prometheus Label Mapping

| Label in Prometheus | Source | Example Value |
|---------------------|--------|---------------|
| `exported_job` | OTel: `{service.namespace}/{service.name}` | `stepflow-docling-grpc/stepflow-server` |
| `exported_instance` | OTel: `service.instance.id` | `docling-orchestrator-5c96688ccf-tmr5s` |
| `job` | Prometheus scrape config | `stepflow-via-otel` |
| `instance` | Prometheus scrape target | `otel-collector.stepflow-o11y:8889` |

**Dashboard queries must use `exported_job`** for filtering OTel-sourced metrics.

### resource_to_telemetry_conversion — Keep Disabled

The OTel Collector Prometheus exporter's `resource_to_telemetry_conversion` must remain **disabled**. Enabling it causes duplicate label conflicts and silently drops metrics. Use the `transform` processor to selectively copy specific resource attributes instead.

### Python OTel Metrics: Lazy Initialization

Python OTel metric instruments created via `get_meter()` before `setup_observability()` sets the `MeterProvider` are permanently no-op. Instruments must be created **after** the MeterProvider is configured. Use a lazy-init pattern:

```python
_initialized = False
_counter = None

def _ensure_metrics():
    global _initialized, _counter
    if _initialized:
        return
    _initialized = True
    meter = metrics.get_meter("my-service")
    _counter = meter.create_counter("my.counter")
```

Call `_ensure_metrics()` from the entry point that runs after `setup_observability()`.

---

## Observability Debugging

### Quick Diagnostic Steps

When metrics aren't appearing in Grafana, diagnose layer by layer:

```
App → OTel Collector → Prometheus exporter → Prometheus scrape → Grafana
```

1. **Check scrape targets**: `curl http://localhost:9090/api/v1/targets` — all should be `UP`
2. **Check metric exists**: `curl --data-urlencode 'query=stepflow_task_connected_workers' http://localhost:9090/api/v1/query`
3. **Check labels**: Inspect a metric's labels to find the correct filter values
4. **Check OTel Collector**: `curl --data-urlencode 'query=increase(stepflow_otelcol_exporter_sent_metric_points_total{exporter="prometheus"}[5m])' http://localhost:9090/api/v1/query`

### Prometheus Restart Strategy

Use scale-down/scale-up instead of `rollout restart` to avoid TSDB lock contention:

```bash
kubectl scale deployment prometheus -n stepflow-o11y --replicas=0
sleep 5
kubectl scale deployment prometheus -n stepflow-o11y --replicas=1
```

### Docker Image Freshness

When code changes don't take effect in a pod:

1. Rebuild with `--load`: `docker build --load -t localhost/image:tag ...`
2. Load into Kind: `kind load docker-image localhost/image:tag --name stepflow`
3. Restart deployment: `kubectl rollout restart deployment/name -n namespace`
4. Verify new pod starts: `kubectl get pods -n namespace -w`

### Grafana Dashboard Query Checklist

- [ ] Use `exported_job` not `job` for OTel-sourced metrics
- [ ] Verify metric name includes correct unit suffix (`_milliseconds` vs `_seconds`)
- [ ] Use `http_status_code` not `http_response_status_code` (old OTel conventions)
- [ ] Use `http_method` not `http_request_method`
- [ ] Test queries directly in Prometheus before embedding in dashboard JSON

---

## Legacy Deployments (Reference Only)

The following directories predate the gRPC pull transport and are **not actively maintained**:

- **`stepflow-docling/`** — HTTP push-based transport with load balancer. Used `type: stepflow` plugin, required a `stepflow-load-balancer` sidecar, and health-check init containers. Superseded by `stepflow-docling-grpc/`.
- **`stepflow/`** — Basic Stepflow deployment without docling integration.

These manifests may reference older image tags and configuration patterns. Consult `stepflow-docling-grpc/` for current patterns.
