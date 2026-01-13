# Observability Stack

This document describes the observability infrastructure for the Stepflow deployment, including setup, configuration, and usage.

## Overview

The observability stack provides comprehensive monitoring, tracing, and logging for all Stepflow components running in Kubernetes. It consists of five main components that work together to provide full-stack observability.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Stepflow Services                        │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐     │
│  │   Stepflow   │  │    Load      │  │   Langflow   │     │
│  │    Server    │  │  Balancer    │  │    Worker    │     │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘     │
│         │                  │                  │              │
│         └──────────────────┼──────────────────┘              │
│                            │ OTLP/gRPC                       │
│                            │                                 │
│  Infrastructure Services (optional OTLP):                    │
│  ┌──────────────┐  ┌──────────────┐                        │
│  │  OpenSearch  │  │   Docling    │                        │
│  └──────────────┘  └──────────────┘                        │
└────────────────────────────┼─────────────────────────────────┘
                             │
                    ┌────────▼────────┐
                    │  OpenTelemetry  │
                    │    Collector    │
                    └────────┬────────┘
                             │
         ┌───────────────────┼───────────────────┐
         │                   │                   │
    ┌────▼─────┐      ┌─────▼─────┐      ┌─────▼─────┐
    │  Jaeger  │      │Prometheus │      │   Loki    │
    │ (Traces) │      │ (Metrics) │      │  (Logs)   │
    └──────────┘      └───────────┘      └───────────┘
         │                   │                   │
         └───────────────────┼───────────────────┘
                             │
                      ┌──────▼──────┐
                      │   Grafana   │
                      │ (Unified UI)│
                      └─────────────┘
```

## Components

### 1. OpenTelemetry Collector (OTel Collector)
- **Purpose**: Receives, processes, and routes telemetry data
- **Version**: 0.95.0
- **Ports**:
  - 4317: OTLP gRPC endpoint
  - 4318: OTLP HTTP endpoint
  - 8888: Prometheus metrics (self-monitoring)
  - 8889: Prometheus exporter
  - 13133: Health check endpoint
- **Configuration**: `k8s/observability/otel-collector/configmap.yaml`
- **Features**:
  - Batch processing for efficiency
  - Memory limiting to prevent OOM
  - Resource attribute enrichment
  - Zstd compression for OTLP exports

**Key Configuration:**
- Receives OTLP (traces, metrics, logs) from Stepflow services
- Exports traces to Jaeger via OTLP
- Exports metrics to Prometheus
- Exports logs to Loki
- Includes health_check extension for Kubernetes probes

### 2. Jaeger
- **Purpose**: Distributed tracing backend
- **Version**: Latest
- **Access**: http://localhost:30686
- **Ports**:
  - 16686: UI and API
  - 4317: OTLP gRPC receiver
  - 14250: gRPC collector
- **Configuration**: `k8s/observability/jaeger/deployment.yaml`
- **Storage**: In-memory (ephemeral)

**Usage:**
- View distributed traces across services
- Analyze request latency and dependencies
- Search by service name, operation, tags, or trace ID
- Linked from Grafana dashboards

### 3. Prometheus
- **Purpose**: Metrics collection and storage
- **Version**: v2.45.0
- **Access**: http://localhost:30090
- **Ports**:
  - 9090: Web UI and API
- **Configuration**: `k8s/observability/prometheus/configmap.yaml`
- **Storage**: 10Gi PersistentVolume

**Scrape Targets:**
- OTel Collector self-metrics (port 8888)
- Prometheus exporter from OTel Collector (port 8889)

### 4. Loki
- **Purpose**: Log aggregation and storage
- **Version**: 2.9.0
- **Ports**:
  - 3100: HTTP API
  - 9096: gRPC
- **Configuration**: `k8s/observability/loki/configmap.yaml`
- **Storage**: 10Gi PersistentVolume

**Features:**
- LogQL query language for powerful log queries
- Label-based indexing for fast filtering
- Automatic trace ID extraction and linking
- Integration with Grafana for visualization

### 5. Promtail
- **Purpose**: Log collection agent for Kubernetes
- **Version**: 2.9.0
- **Deployment**: DaemonSet (runs on each node)
- **Configuration**: `k8s/observability/promtail/configmap.yaml`

**Key Features:**
- Scrapes logs from all pods in `stepflow-demo` namespace
- Parses CRI (containerd) log format
- Extracts trace context using regex:
  - `trace_id` - Links logs to traces
  - `span_id` - Links logs to specific spans
  - `run_id` - Groups logs by workflow execution
  - `flow_id` - Groups logs by workflow definition
- Automatically adds Kubernetes metadata labels:
  - `namespace`, `pod`, `container`, `app`, `component`, `node`
- Ships logs to Loki in real-time

**Log Collection Path:**
```
Pod stdout/stderr → Containerd → /var/log/pods → Promtail → Loki
```

### 6. Grafana
- **Purpose**: Unified observability dashboard
- **Version**: 10.3.3
- **Access**: http://localhost:30001 (via port-forward)
- **Credentials**: admin/admin
- **Configuration**: `k8s/observability/grafana/`
- **Storage**: 5Gi PersistentVolume

**Pre-configured Datasources:**
- Prometheus (metrics) - default datasource
- Loki (logs)
- Jaeger (traces)

**Pre-provisioned Dashboards:**
1. **Stepflow Folder**:
   - Workflow Overview: Active workflows, execution times, traces, logs

2. **Infrastructure Folder**:
   - OpenTelemetry Collector Health: Collector metrics and performance

**Cross-linking Features:**
- Metrics → Traces (via exemplars)
- Logs → Traces (via trace_id extraction)
- Traces → Logs (via service/step tags)

## Deployment

### Namespace
All observability components run in the `observability` namespace.

### Installation

1. **Deploy observability stack:**
```bash
cd k8s/observability

# Create namespace
kubectl apply -f namespace.yaml

# Deploy components
kubectl apply -f otel-collector/
kubectl apply -f jaeger/
kubectl apply -f prometheus/
kubectl apply -f loki/
kubectl apply -f grafana/
```

2. **Verify deployment:**
```bash
kubectl get pods -n observability
```

Expected output:
```
NAME                              READY   STATUS    RESTARTS   AGE
grafana-xxxxxxxxxx-xxxxx          1/1     Running   0          5m
jaeger-xxxxxxxxxx-xxxxx           1/1     Running   0          5m
loki-xxxxxxxxxx-xxxxx             1/1     Running   0          5m
otel-collector-xxxxxxxxxx-xxxxx   1/1     Running   0          5m
prometheus-xxxxxxxxxx-xxxxx       1/1     Running   0          5m
```

3. **Setup port forwarding:**
```bash
# Grafana (unified UI)
kubectl port-forward -n observability svc/grafana 30001:3000 &

# Jaeger (optional - traces accessible via Grafana)
kubectl port-forward -n observability svc/jaeger 30686:16686 &

# Prometheus (optional - metrics accessible via Grafana)
kubectl port-forward -n observability svc/prometheus 30090:9090 &
```

## Stepflow Service Configuration

All Stepflow services are configured to send telemetry to the OTel Collector via OTLP.

### Environment Variables

Each service has the following OTLP configuration:

```yaml
env:
  - name: OTEL_EXPORTER_OTLP_ENDPOINT
    value: "http://otel-collector.observability.svc.cluster.local:4317"
  - name: OTEL_EXPORTER_OTLP_PROTOCOL
    value: "grpc"
  - name: OTEL_SERVICE_NAME
    value: "<service-name>"  # Unique per service
  - name: OTEL_TRACES_EXPORTER
    value: "otlp"
  - name: OTEL_METRICS_EXPORTER
    value: "otlp"
  - name: OTEL_LOGS_EXPORTER
    value: "otlp"
```

### Service-Specific Configuration

**Stepflow Server** (`k8s/stepflow-server/stepflow-server-deployment.yaml`):
- Service name: `stepflow-server`
- Traces workflow execution
- Emits metrics for workflow performance

**Stepflow Load Balancer** (`k8s/load-balancer/deployment.yaml`):
- Service name: `stepflow-load-balancer`
- Traces request routing
- Emits metrics for load distribution

**Langflow Worker** (`k8s/langflow-worker/deployment.yaml`):
- Service name: `langflow-worker`
- Python-based service with auto-instrumentation
- Additional env: `OTEL_PYTHON_LOGGING_AUTO_INSTRUMENTATION_ENABLED=true`
- Traces component execution
- Emits metrics for component performance

**Docling** (`k8s/stepflow/docling/deployment.yaml`):
- Service name: `docling-serve`
- Infrastructure service for document processing
- Configured with standard OTEL_* environment variables
- **Note**: OpenTelemetry support in docling-serve may vary. The deployment includes standard OTEL configuration which will be used if the service supports it. Verify docling-serve documentation for current OTLP support status.

## Accessing the Observability Stack

### Grafana (Primary Interface)

**URL**: http://localhost:30001
**Credentials**: admin/admin

**Key Features:**
1. **Explore**: Ad-hoc queries across all datasources
2. **Dashboards**: Pre-built visualizations
3. **Alerting**: Configure alerts on metrics/logs
4. **Unified View**: Switch between traces, logs, metrics seamlessly

**Navigation:**
- Home → Dashboards → Browse → Stepflow (workflow dashboards)
- Home → Dashboards → Browse → Infrastructure (system dashboards)
- Home → Explore → Select datasource (Prometheus/Loki/Jaeger)

## Single Pane of Glass View

Grafana provides multiple ways to achieve a unified observability view across traces, logs, and metrics.

### Method 1: Grafana Explore (Recommended for Investigation)

**URL**: http://localhost:30001/explore

Explore is your primary single pane of glass interface for ad-hoc investigation:

**Features:**
1. **Multi-datasource queries**: Query Jaeger, Loki, and Prometheus simultaneously
2. **Split view**: View multiple datasources side-by-side
3. **Time synchronization**: All panels share the same time range for correlation
4. **Context preservation**: Switch datasources without losing query context
5. **Automatic linking**: Click trace IDs, span IDs, or derived fields to navigate

**How to use:**
1. Navigate to Home → Explore (or click compass icon in left sidebar)
2. Select a datasource from the dropdown (Jaeger, Loki, or Prometheus)
3. Build and run your query
4. Click "Split" button to add another panel
5. Select a different datasource in the new panel
6. Both panels stay synchronized by time range

**Example workflow - Trace + Logs correlation:**
```
Left panel:  Jaeger → Search service: "stepflow-server" → Select trace
Right panel: Loki → Query: {trace_id="<id>"}
```

**Example workflow - Metrics + Traces correlation:**
```
Left panel:  Prometheus → Query: rate(stepflow_workflow_duration_seconds[5m])
Right panel: Jaeger → Search for slow traces in the same time range
```

**Loki Query Examples:**

Basic queries:
```logql
# All logs from stepflow-server
{app="stepflow-server"}

# All logs from any Stepflow service
{namespace="stepflow-demo"}

# Logs for a specific trace
{trace_id="87964655711625122597312503349329414284"}

# Logs for a specific workflow run
{run_id="d11d0586-d3c1-4969-a6fb-7fc55a3d20ef"}

# Logs for a specific flow definition
{flow_id="e777118c6ff9e5f0df74ef27bb9f8f4dccf64bab21639c3c21ce7a1dbb4da42f"}

# Filter by multiple labels
{app="stepflow-server", run_id=~".+"}

# Search for specific text in logs
{app="stepflow-server"} |= "Component"

# Search for errors (case-insensitive)
{namespace="stepflow-demo"} |~ "(?i)error"

# All logs with trace context (any service)
{trace_id=~".+"}
```

Advanced filtering with LogQL:
```logql
# Count errors per service
sum by (app) (count_over_time({namespace="stepflow-demo"} |~ "(?i)error" [5m]))

# View logs around a specific workflow execution
{run_id="<run-id>"} | line_format "{{.app}}: {{.log}}"

# Filter by time and service
{app="stepflow-load-balancer"} | json | level="INFO"
```

### Method 2: Stepflow Workflow Overview Dashboard (Recommended for Monitoring)

**URL**: http://localhost:30001/d/stepflow-workflow-overview

Or navigate: Home → Dashboards → Browse → Stepflow → Workflow Overview

This pre-built dashboard provides a comprehensive single pane of glass:

**Dashboard Layout:**
1. **Top Section - Metrics**:
   - Active Workflows stat panel
   - Workflow Duration time series chart

2. **Middle Section - Trace Navigation**:
   - Direct links to Jaeger UI
   - Links to Grafana Explore with Jaeger datasource
   - Instructions for trace correlation

3. **Bottom Section - Live Logs**:
   - Real-time log stream from all Stepflow services
   - Automatic trace_id extraction and linking
   - Click any trace_id to jump to full trace view

**Key Feature**: This dashboard is designed for continuous monitoring with all telemetry types visible at once.

### Method 3: Trace → Logs Correlation

When viewing traces, navigate directly to related logs:

1. Open Explore → Select Jaeger datasource
2. Search for service: `stepflow-server`, `stepflow-load-balancer`, or `langflow-worker`
3. Click on any trace to open trace details
4. Click on any span in the trace timeline
5. Look for "Logs for this span" button in span details
6. Click to automatically query Loki for logs matching the trace_id

**How it works**: The Jaeger datasource configuration includes `tracesToLogs` mapping that automatically generates Loki queries based on trace metadata.

### Method 4: Logs → Traces Correlation

When viewing logs, navigate directly to related traces:

1. Open Explore → Select Loki datasource
2. Query logs: `{app=~"stepflow.*"}` or filter by specific service
3. Logs are automatically tagged with trace context labels:
   - `trace_id` - Unique trace identifier
   - `span_id` - Span identifier
   - `run_id` - Workflow run identifier
   - `flow_id` - Workflow definition identifier
4. Click on any log line in the results
5. Look for the "TraceID" derived field link in the log details
6. Click to automatically open the corresponding trace in Jaeger

**How it works**:
- Promtail extracts trace context from structured logs using regex: `trace_id=(?P<trace_id>\S+).*span_id=(?P<span_id>\S+).*run_id=(?P<run_id>\S+).*flow_id=(?P<flow_id>\S+)`
- These are indexed as labels in Loki for fast filtering
- The Loki datasource configuration includes `derivedFields` that create clickable links from trace_id to Jaeger

### Method 5: Metrics → Traces Correlation (Exemplars)

When viewing metrics, jump to example traces:

1. Open Explore → Select Prometheus datasource
2. Query metrics with exemplars support
3. Look for trace markers on the time series graph
4. Click on an exemplar point
5. Grafana opens the corresponding trace in Jaeger

**Note**: Requires metrics to be instrumented with trace context (exemplar support).

## Quick Access URLs

Once port-forward is running (http://localhost:30001):

- **Main Dashboard**: http://localhost:30001
- **Explore (single pane of glass)**: http://localhost:30001/explore
- **Stepflow Dashboard**: http://localhost:30001/d/stepflow-workflow-overview
- **Infrastructure Dashboard**: http://localhost:30001/d/otel-collector-health
- **Dashboards Browser**: http://localhost:30001/dashboards

## Pro Tips for Single Pane of Glass Usage

### Tip 1: Use Split View in Explore
The split view allows you to correlate data across different telemetry types:
- Top: Trace timeline from Jaeger
- Bottom: Related logs from Loki filtered by trace_id
- Both synchronized by time range

### Tip 2: Start with the Workflow Dashboard
For day-to-day monitoring, the Stepflow Workflow Overview dashboard is your best starting point:
- Metrics show overall health
- Logs provide real-time activity
- Trace links enable deep dives when needed

### Tip 3: Use Derived Fields for Navigation
Loki's derived field extraction automatically makes trace_id, run_id, and other IDs clickable:
- Click trace_id → Jump to Jaeger trace
- Click run_id → Filter logs by workflow run
- No manual query construction needed

### Tip 4: Bookmark Your Most Used Queries
Grafana Explore supports saved queries:
1. Build a useful query in Explore
2. Click "Add to dashboard" or save the query
3. Reuse across different time ranges and investigations

### Tip 5: Use Time Range Picker
All Grafana views share a time range picker:
- Top right corner of every view
- Set absolute time range for specific incidents
- Set relative range (Last 1h, Last 6h) for monitoring
- Use "Zoom to data" to focus on relevant timeframe

### Direct Access (Optional)

**Jaeger UI**: http://localhost:30686
- Search traces by service: `stepflow-server`, `stepflow-load-balancer`, `langflow-worker`
- Filter by operation, tags, duration
- View trace timeline and span details

**Prometheus UI**: http://localhost:30090
- Execute PromQL queries
- View time-series graphs
- Check target health

**Loki** (no direct UI):
- Access via Grafana Explore
- Use LogQL for log queries
- Example: `{job="stepflow-server"} |= "error"`

## Telemetry Flow

### Traces
1. Stepflow service creates spans using fastrace
2. Spans exported to OTel Collector (OTLP/gRPC, port 4317)
3. OTel Collector batches and forwards to Jaeger (OTLP, port 4317)
4. Jaeger stores and indexes traces
5. Grafana queries Jaeger for trace visualization

**Trace ID = Run ID**: Each workflow execution uses its `run_id` (UUID) as the OpenTelemetry `trace_id`, providing a 1:1 mapping between business workflows and distributed traces.

### Metrics
1. Stepflow service emits metrics via OpenTelemetry SDK
2. Metrics exported to OTel Collector (OTLP/gRPC, port 4317)
3. OTel Collector exposes Prometheus exporter (port 8889)
4. Prometheus scrapes metrics from OTel Collector
5. Grafana queries Prometheus for metric visualization

### Logs
1. Stepflow service writes structured logs to stdout with trace context (trace_id, span_id, run_id, flow_id)
2. Kubernetes captures logs and writes to `/var/log/pods`
3. Promtail (DaemonSet) scrapes logs from all pods in `stepflow-demo` namespace
4. Promtail parses CRI format and extracts trace context using regex
5. Promtail ships logs to Loki with labels (HTTP, port 3100)
6. Loki indexes logs by labels (app, namespace, pod, trace_id, etc.)
7. Grafana queries Loki for log visualization

**Current Implementation**: Logs are collected via Promtail scraping pod logs, not via OTLP export. This provides immediate log collection without requiring application code changes.

## Cross-Linking

The observability stack provides automatic cross-linking between telemetry types:

### Logs ↔ Traces
- **Logs → Traces**: Logs are labeled with `trace_id` extracted by Promtail. Grafana's derived fields feature creates clickable links from trace_id to Jaeger.
- **Traces → Logs**: Once traces are exported, clicking a span in Jaeger will query Loki for logs matching the trace_id.
- **Mechanism**:
  - Stepflow services include trace context (trace_id, span_id) in structured logs
  - Promtail extracts these using regex: `trace_id=(?P<trace_id>\S+)`
  - Loki indexes trace_id as a label for fast filtering
  - Grafana datasource configuration includes derived fields for automatic linking

### Metrics ↔ Traces
- **Metrics → Traces**: Click exemplar in Prometheus metrics to view trace
- **Mechanism**: Prometheus exemplar support with trace_id labels

### Unified Navigation
- **Grafana Explore**: Switch datasources while preserving time range and context
- **Dashboard Links**: Pre-configured links between related views
- **Service Map**: Visualize service dependencies from traces

## Troubleshooting

### OTel Collector Issues

**Symptom**: OTel Collector in CrashLoopBackOff

**Check health endpoint:**
```bash
kubectl logs -n observability -l app=otel-collector --tail=50
```

**Common causes:**
- Missing health_check extension (fixed in configmap.yaml)
- Invalid YAML configuration
- Insufficient resources

**Resolution:**
```bash
# Restart collector
kubectl rollout restart deployment/otel-collector -n observability

# Verify health
kubectl get pods -n observability -l app=otel-collector
```

### Missing Telemetry Data

**Check OTel Collector is receiving data:**
```bash
kubectl logs -n observability -l app=otel-collector --tail=100 | grep -i "received"
```

**Check Stepflow services have correct OTLP config:**
```bash
kubectl describe pod -n stepflow-demo <pod-name> | grep OTEL_
```

**Verify network connectivity:**
```bash
kubectl exec -n stepflow-demo <pod-name> -- nc -zv otel-collector.observability.svc.cluster.local 4317
```

### Grafana Dashboard Issues

**Symptom**: Dashboards not appearing

**Check ConfigMaps:**
```bash
kubectl get configmaps -n observability | grep grafana
```

Expected:
- grafana-datasources
- grafana-dashboards-provisioning
- grafana-dashboard-stepflow
- grafana-dashboard-infrastructure

**Check Grafana logs:**
```bash
kubectl logs -n observability -l app=grafana --tail=100 | grep -i dashboard
```

**Reload dashboards:**
```bash
kubectl rollout restart deployment/grafana -n observability
```

### Port-Forward Connection Issues

**Symptom**: Cannot access http://localhost:30001

**Check if port-forward is running:**
```bash
ps aux | grep "kubectl port-forward" | grep -v grep
```

**Restart port-forward:**
```bash
# Kill existing
pkill -f "kubectl port-forward.*grafana"

# Start new
kubectl port-forward -n observability svc/grafana 30001:3000 &
```

**Verify connection:**
```bash
curl -I http://localhost:30001
```

### Promtail Log Collection Issues

**Symptom**: No logs appearing in Loki/Grafana

**Check Promtail pods:**
```bash
kubectl get pods -n observability -l app=promtail
```

**Check Promtail logs:**
```bash
kubectl logs -n observability -l app=promtail --tail=50
```

Look for:
- "Adding target" messages (shows log files being discovered)
- "Seeked" messages (shows log files being read)
- Any error messages about permissions or paths

**Verify Loki has data:**
```bash
curl -s "http://localhost:3100/loki/api/v1/label/app/values" | jq
```

Should return service names like: `stepflow-server`, `stepflow-load-balancer`, `langflow-worker`

**Check trace_id labels are extracted:**
```bash
curl -s 'http://localhost:3100/loki/api/v1/labels' | jq -r '.data[] | select(. == "trace_id" or . == "span_id" or . == "run_id")'
```

**Restart Promtail if needed:**
```bash
kubectl delete pods -n observability -l app=promtail
```

## Configuration Files

All configuration files are located in `k8s/observability/`:

```
observability/
├── namespace.yaml                                    # observability namespace
├── otel-collector/
│   ├── configmap.yaml                               # Collector pipeline config
│   ├── deployment.yaml                              # Collector deployment
│   └── service.yaml                                 # Collector services
├── jaeger/
│   ├── deployment.yaml                              # Jaeger all-in-one
│   └── service.yaml                                 # Jaeger services
├── prometheus/
│   ├── configmap.yaml                               # Prometheus scrape config
│   └── deployment.yaml                              # Prometheus deployment
├── loki/
│   ├── configmap.yaml                               # Loki storage config
│   └── deployment.yaml                              # Loki deployment
├── promtail/
│   ├── configmap.yaml                               # Promtail pipeline config
│   └── daemonset.yaml                               # Promtail DaemonSet + RBAC
└── grafana/
    ├── configmap-datasources.yaml                   # Datasource provisioning
    ├── configmap-dashboards-provisioning.yaml       # Dashboard provider config
    ├── configmap-dashboard-stepflow.yaml            # Stepflow dashboards
    ├── configmap-dashboard-infrastructure.yaml      # Infrastructure dashboards
    └── deployment.yaml                              # Grafana deployment
```

## Best Practices

### Development
1. Use Grafana Explore for ad-hoc queries during development
2. Check OTel Collector logs when adding new services
3. Verify trace context propagation across service boundaries
4. Use descriptive span names and attributes

### Production Considerations
1. **Storage**: Replace in-memory Jaeger with persistent backend (Elasticsearch, Cassandra)
2. **Retention**: Configure data retention policies for Prometheus and Loki
3. **Sampling**: Enable trace sampling in OTel Collector for high-traffic environments
4. **Security**: Add authentication to Grafana (LDAP, OAuth, etc.)
5. **Alerting**: Configure Grafana alerts for critical metrics
6. **Backup**: Regular backups of Grafana dashboards and Prometheus data

### Monitoring the Monitor
- OTel Collector exports its own metrics to Prometheus
- Infrastructure dashboard shows collector health
- Set up alerts for collector errors, dropped spans, or memory issues

## Performance Impact

The observability stack has minimal performance impact on Stepflow services:

- **Fastrace**: Zero-cost tracing abstraction (no overhead when disabled)
- **OTLP**: Efficient binary protocol with Zstd compression
- **Batching**: OTel Collector batches telemetry to reduce network calls
- **Async Export**: Telemetry exported asynchronously, non-blocking

## Further Reading

- [OpenTelemetry Documentation](https://opentelemetry.io/docs/)
- [Jaeger Documentation](https://www.jaegertracing.io/docs/)
- [Prometheus Documentation](https://prometheus.io/docs/)
- [Loki Documentation](https://grafana.com/docs/loki/latest/)
- [Grafana Documentation](https://grafana.com/docs/grafana/latest/)
- [Stepflow Observability Design](https://github.com/stepflow-ai/stepflow/blob/main/CLAUDE.md#logging-and-tracing)
