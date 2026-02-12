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
