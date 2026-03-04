# Kubernetes Demo & Docker Development Guide

## Conventions

### Log Level Values
- Use **lowercase**: `debug`, `info`, `warn`, `error`, `trace`
- Parsed case-insensitively by both Rust and Python, but lowercase is canonical
- Matches Rust runtime default and README documentation

### Container Users
- Dockerfiles: Create named user `stepflow` with UID 1000 (`useradd -m -u 1000 stepflow`)
- K8s manifests: Use numeric `runAsUser: 1000` + `runAsNonRoot: true` at pod level
- Exception: Base images with existing users (e.g., langflow uses `USER 1000` directly)

### SecurityContext Tiers
- **Rust services** (stateless binaries): Full lockdown
  `allowPrivilegeEscalation: false` + `readOnlyRootFilesystem: true`
- **Python workers** (need temp dirs): Partial lockdown
  `allowPrivilegeEscalation: false` only
- **Data stores** (OpenSearch): Minimal
  Container-level `allowPrivilegeEscalation: false` only
- **O11y stack**: No changes (third-party images)

### Docker Build Patterns
- Use multi-stage builds (deps → builder → runtime for Python; builder → runtime for Rust)
- Pin uv version: `COPY --from=ghcr.io/astral-sh/uv:0.6.14 /uv /bin/uv`
- Use `uv sync --frozen` for reproducible builds
- Handle `.python-version` conflicts: `rm -f .../.python-version` before `uv sync` when
  base image Python differs from dev pin
- Use `COPY --chown=1000:0` instead of `RUN chown -R` to avoid layer duplication
- Use `UV_PYTHON_PREFERENCE=only-system` when base image provides the target Python

### OTEL_RESOURCE_ATTRIBUTES
All application pods should include the full set of resource attributes:
```yaml
- name: OTEL_RESOURCE_ATTRIBUTES
  value: "deployment.environment=kubernetes,service.namespace=stepflow,service.instance.id=$(POD_NAME),k8s.pod.name=$(POD_NAME),k8s.namespace.name=$(POD_NAMESPACE)"
```
Requires POD_NAME and POD_NAMESPACE from the downward API.

### Observability Env Vars
Rust services (server, load-balancer):
- `STEPFLOW_LOG_LEVEL`, `STEPFLOW_OTHER_LOG_LEVEL`, `STEPFLOW_LOG_FORMAT=json`
- `STEPFLOW_OTLP_ENDPOINT`, `STEPFLOW_TRACE_ENABLED`, `STEPFLOW_METRICS_ENABLED`

Python workers (langflow, docling):
- `STEPFLOW_LOG_LEVEL`, `STEPFLOW_SERVICE_NAME`
- `STEPFLOW_OTLP_ENDPOINT`, `STEPFLOW_TRACE_ENABLED`, `STEPFLOW_LOG_DESTINATION=stderr,otlp`
- `OTEL_PYTHON_LOGGING_AUTO_INSTRUMENTATION_ENABLED=true`

### imagePullPolicy
- Locally-built images (`localhost/*`): `imagePullPolicy: Never`
- Registry images (`ghcr.io/*`, `quay.io/*`): `imagePullPolicy: IfNotPresent`

---

## Metrics Naming & Label Scoping

### OTel → Prometheus Metric Name Conventions

OTel metrics are exported through the Collector's Prometheus exporter and follow specific naming rules:

- **Namespace prefix**: All application metrics get the `stepflow_` prefix (set via the OTel Collector `resource` processor or `namespace` on the Prometheus exporter).
- **Unit suffix**: OTel metric units are appended to the name. A metric defined as `http.server.duration` with unit `ms` becomes `stepflow_http_server_duration_milliseconds`. Check the actual metric name in Prometheus — don't assume `_seconds` vs `_milliseconds`.
- **Histogram suffixes**: OTel histograms produce `_bucket`, `_count`, and `_sum` series in Prometheus.
- **Counter suffix**: OTel counters get `_total` appended.

### Prometheus Label Mapping (exported_job vs job)

When the OTel Collector Prometheus exporter emits metrics with a `job` label (derived from `service.namespace/service.name`), Prometheus **renames** it to `exported_job` to avoid collision with its own scrape-config `job` label.

| Label in Prometheus | Source | Example Value |
|---------------------|--------|---------------|
| `exported_job` | OTel resource: `{service.namespace}/{service.name}` | `stepflow-docling/docling-step-worker` |
| `exported_instance` | OTel resource: `service.instance.id` | `docling-worker-596f96cb4d-72vvb` |
| `job` | Prometheus scrape config job name | `stepflow-via-otel` |
| `instance` | Prometheus scrape target address | `otel-collector.stepflow-o11y:8889` |

**Dashboard queries must use `exported_job`** for filtering OTel-sourced metrics, not `job` or `service_name`.

Example: `stepflow_http_server_duration_milliseconds_count{exported_job=~".*docling-step-worker.*"}`

### resource_to_telemetry_conversion — Keep Disabled

The OTel Collector Prometheus exporter has a `resource_to_telemetry_conversion` option that promotes resource attributes (like `service.name`, `deployment.environment`) to metric labels. **This must remain disabled** (v0.95.0+) because:

1. The `resource` processor already inserts `deployment.environment` and `service.namespace` as resource attributes
2. When promoted to labels, they conflict with existing attributes of the same name
3. The Prometheus exporter silently drops the entire metric — no error logged, metrics just vanish

If you need resource attributes as labels, use the Collector's `transform` processor to selectively copy specific attributes instead.

### Metric Names by Component

| Component | Metric Pattern | Label Filter |
|-----------|---------------|--------------|
| Python worker HTTP | `stepflow_http_server_duration_milliseconds_*` | `exported_job=~".*worker-name.*"` |
| Python worker active | `stepflow_http_server_active_requests` | `exported_job=~".*worker-name.*"` |
| Load balancer | `stepflow_lb_requests_total`, `stepflow_lb_request_duration_ms_*` | `app="lb-pod-label"` |
| LB health | `stepflow_lb_backend_health`, `stepflow_lb_active_connections` | `app="lb-pod-label"` |
| Blob storage | `stepflow_blob_store_{put,get}_duration_seconds_*` | (via OTel, uses `exported_job`) |
| Rust orchestrator | `stepflow_step_executions_total`, `stepflow_step_retries_total` | (via OTel, uses `exported_job`) |

### HTTP Label Names (Python auto-instrumentation)

Python OTel auto-instrumentation (opentelemetry-instrumentation-fastapi) uses **old** OTel semantic conventions:

| Label | Example |
|-------|---------|
| `http_method` | `POST` |
| `http_status_code` | `200` |
| `http_target` | `/` |

**Do not** use the newer conventions (`http_request_method`, `http_response_status_code`) — they won't match.

---

## Observability Debugging

### Quick Diagnostic Steps

When metrics aren't appearing in Grafana, diagnose layer by layer:

```
App → OTel Collector → Prometheus exporter → Prometheus scrape → Grafana
```

1. **Check scrape targets**: `curl http://localhost:9090/api/v1/targets` — all should be `UP`
2. **Check metric exists**: `curl --data-urlencode 'query=stepflow_http_server_duration_milliseconds_count' http://localhost:9090/api/v1/query`
3. **Check labels**: Inspect a metric's labels to find the correct filter values
4. **Check OTel Collector**: Verify `sent_metric_points` is increasing:
   ```bash
   curl --data-urlencode 'query=increase(stepflow_otelcol_exporter_sent_metric_points_total{exporter="prometheus"}[5m])' \
     http://localhost:9090/api/v1/query
   ```

### Prometheus Restart Strategy

Use scale-down/scale-up instead of `rollout restart` to avoid TSDB storage lock contention:

```bash
kubectl scale deployment prometheus -n stepflow-o11y --replicas=0
sleep 5
kubectl scale deployment prometheus -n stepflow-o11y --replicas=1
```

### Checking What the OTel Collector Receives

Query the Collector's own metrics to see if data is flowing:

```bash
# Metrics received by the collector
curl --data-urlencode 'query=stepflow_otelcol_receiver_accepted_metric_points_total' http://localhost:9090/api/v1/query

# Metrics sent by the prometheus exporter
curl --data-urlencode 'query=stepflow_otelcol_exporter_sent_metric_points_total{exporter="prometheus"}' http://localhost:9090/api/v1/query

# Metrics dropped (should be 0)
curl --data-urlencode 'query=stepflow_otelcol_exporter_send_failed_metric_points_total' http://localhost:9090/api/v1/query
```

### Docker Image Freshness

When Python SDK changes don't take effect in the worker, the Docker image may be stale:

1. Rebuild with a new tag: `docker build -t localhost/stepflow-docling-worker:o11y-fix-vN ...`
2. Load into Kind: `kind load docker-image localhost/stepflow-docling-worker:o11y-fix-vN --name stepflow`
3. Update the deployment YAML image tag
4. Apply: `kubectl apply -f worker/deployment.yaml`
5. Verify the new pod starts: `kubectl get pods -n stepflow-docling -w`

### Health Endpoint Trace Exclusion

K8s liveness/readiness probes generate high-frequency traces. Exclude them:

- **Python worker**: `FastAPIInstrumentor.instrument_app(app, excluded_urls="health")`
- **Deployment env var**: `OTEL_PYTHON_FASTAPI_EXCLUDED_URLS=health`

Both must be set — the env var is the runtime config, and the code parameter is the fallback.

### Grafana Dashboard Query Checklist

When creating or updating Grafana dashboard panels for OTel-sourced metrics:

- [ ] Use `exported_job` not `job` or `service_name` for filtering
- [ ] Verify metric name includes correct unit suffix (`_milliseconds` vs `_seconds`)
- [ ] Use `http_status_code` not `http_response_status_code` (old OTel conventions)
- [ ] Use `http_method` not `http_request_method`
- [ ] For LB metrics (direct Prometheus scrape), use `app` label from pod metadata
- [ ] Test queries directly in Prometheus before embedding in dashboard JSON
