# Stepflow Observability Stack

Complete observability solution for Stepflow with metrics, logs, and distributed tracing.

## Components

| Component | Purpose | License | UI Port |
|-----------|---------|---------|---------|
| **OpenTelemetry Collector** | Telemetry ingestion & routing | Apache 2.0 | - |
| **Jaeger** | Distributed tracing | Apache 2.0 | 16686 |
| **Prometheus** | Metrics storage & queries | Apache 2.0 | 9090 |
| **Loki** | Log aggregation | AGPL 3.0* | 3100 |
| **Grafana** | Unified observability UI | AGPL 3.0* | 3000 |

*AGPL-licensed components are provided as official Docker images for local development. See [License Notes](#license-notes) below.

## Quick Start

### 1. Start the Observability Stack

```bash
cd docker/observability
docker compose up -d
```

### 2. Configure Stepflow to Export Telemetry

Set these environment variables when running Stepflow:

```bash
export STEPFLOW_TRACE_ENABLED=true
export STEPFLOW_OTLP_ENDPOINT=http://localhost:4317
export STEPFLOW_LOG_DESTINATION=otlp  # Send logs to OTLP endpoint
export STEPFLOW_LOG_LEVEL=info
```

Or to keep logs local while sending traces:

```bash
export STEPFLOW_TRACE_ENABLED=true
export STEPFLOW_OTLP_ENDPOINT=http://localhost:4317
export STEPFLOW_LOG_DESTINATION=stdout  # Keep logs local
export STEPFLOW_LOG_FORMAT=json
export STEPFLOW_LOG_LEVEL=info
```

### 3. Run a Workflow

```bash
cd stepflow-rs
cargo run --bin=stepflow -- \
  run --flow ../examples/basic/workflow.yaml \
  --input ../examples/basic/input1.json \
  --config ../examples/basic/stepflow-config.yml
```

### 4. Access the UIs

- **Grafana** (unified dashboard): http://localhost:3001
  - Username: `admin`
  - Password: `admin`
- **Jaeger** (traces only): http://localhost:16686
- **Prometheus** (metrics only): http://localhost:9090

## Architecture

```
┌─────────────┐
│  Stepflow   │  (Rust application)
└──────┬──────┘
       │ OTLP (gRPC:4317 or HTTP:4318)
       ↓
┌──────────────────────────────┐
│  OpenTelemetry Collector     │  (Receives & routes telemetry)
│  - Receives: OTLP            │
│  - Exports:                  │
│    • Traces → Jaeger         │
│    • Metrics → Prometheus    │
│    • Logs → Loki             │
└────┬─────────┬─────────┬─────┘
     │         │         │
     ↓         ↓         ↓
┌────────┐ ┌──────────┐ ┌──────┐
│ Jaeger │ │Prometheus│ │ Loki │  (Storage backends)
└────┬───┘ └────┬─────┘ └───┬──┘
     │          │           │
     └──────────┴───────────┘
                │
                ↓
         ┌───────────┐
         │  Grafana  │  (Unified UI)
         └───────────┘
```

## Features

### 📊 Metrics (Prometheus + Grafana)
- Workflow execution counts and durations
- Step-level performance metrics
- Plugin call statistics
- System resource usage
- Custom application metrics

**Access:** Grafana → Explore → Prometheus datasource

### 📝 Logs (Loki + Grafana)
- Structured JSON logs from Stepflow
- Automatic trace context injection (trace_id, span_id)
- Workflow execution context (run_id, step_id)
- Full-text log search
- Log-to-trace correlation

**Access:** Grafana → Explore → Loki datasource

### 🔍 Traces (Jaeger + Grafana)
- End-to-end workflow execution traces
- Distributed tracing across plugin boundaries
- Performance bottleneck identification
- Service dependency mapping
- Trace-to-log correlation

**Access:** Grafana → Explore → Jaeger datasource or Jaeger UI directly

### 🔗 Correlation
- **Traces → Logs**: Click trace ID to see related logs
- **Logs → Traces**: Click log entry to see full trace
- **Metrics → Traces**: Exemplars link metrics to traces

## Configuration

### Stepflow Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `STEPFLOW_TRACE_ENABLED` | `false` | Enable distributed tracing |
| `STEPFLOW_OTLP_ENDPOINT` | - | OTLP endpoint (e.g., `http://localhost:4317`). Required when `trace_enabled=true` or `log_destination=otlp` |
| `STEPFLOW_LOG_DESTINATION` | `stdout` | Where to send logs: `stdout`, `file`, or `otlp` |
| `STEPFLOW_LOG_LEVEL` | `info` | Log level: off, error, warn, info, debug, trace |
| `STEPFLOW_OTHER_LOG_LEVEL` | - | Log level for dependencies (separate from Stepflow logs) |
| `STEPFLOW_LOG_FORMAT` | `text` | Log format: `text` or `json` (only applies to stdout/file destinations) |
| `STEPFLOW_LOG_FILE` | - | Log file path (required when `log_destination=file`) |

### CLI Arguments

```bash
# Send both logs and traces to OTLP
stepflow run --flow workflow.yaml \
  --log-destination otlp \
  --log-level debug \
  --trace-enabled \
  --otlp-endpoint http://localhost:4317

# Or keep logs local while sending traces
stepflow run --flow workflow.yaml \
  --log-destination stdout \
  --log-format json \
  --log-level debug \
  --trace-enabled \
  --otlp-endpoint http://localhost:4317
```

### Component Ports

| Component | Port | Protocol | Purpose |
|-----------|------|----------|---------|
| OTel Collector | 4317 | gRPC | OTLP ingestion (recommended) |
| OTel Collector | 4318 | HTTP | OTLP ingestion (alternative) |
| OTel Collector | 8888 | HTTP | Prometheus metrics (collector itself) |
| OTel Collector | 8889 | HTTP | Prometheus exporter (Stepflow metrics) |
| Jaeger | 16686 | HTTP | Jaeger UI |
| Jaeger | 14250 | gRPC | Jaeger ingestion from OTel |
| Prometheus | 9090 | HTTP | Prometheus UI & API |
| Loki | 3100 | HTTP | Loki API |
| Grafana | 3000 | HTTP | Grafana UI |

## Usage Examples

### View Logs for a Specific Workflow Run

1. Open Grafana: http://localhost:3000
2. Go to Explore → Select "Loki" datasource
3. Query:
   ```logql
   {service_name="stepflow"} |= "run_id=<your-run-id>"
   ```

### View Trace for a Workflow Execution

1. Open Jaeger UI: http://localhost:16686
2. Select Service: `stepflow`
3. Search by `run_id` tag

Or in Grafana:
1. Explore → Select "Jaeger" datasource
2. Search by tag: `run_id=<your-run-id>`

### Query Workflow Execution Metrics

1. Open Grafana → Explore → Select "Prometheus"
2. Query example:
   ```promql
   # Workflow execution rate
   rate(stepflow_workflow_executions_total[5m])

   # Average step duration
   avg(stepflow_step_duration_seconds) by (step_name)
   ```

### Correlate Logs and Traces

1. View logs in Grafana (Loki datasource)
2. Click on a log entry with a `trace_id`
3. Grafana automatically shows the related trace
4. Or click "TraceID" link to jump to full trace view

## Dashboards

Pre-configured Grafana dashboards (auto-loaded on startup):

### Stepflow Folder
- **Workflow Overview**: High-level workflow execution metrics
- **Step Performance**: Detailed step-by-step analysis
- **Plugin Statistics**: Plugin call patterns and performance
- (TODO: More dashboards to be added)

### Infrastructure Folder
- **OTel Collector**: Collector health and throughput
- **Observability Stack**: Jaeger, Prometheus, Loki health

## Troubleshooting

### No Traces Appearing in Jaeger

**Check:**
1. Verify `STEPFLOW_TRACE_ENABLED=true`
2. Verify `STEPFLOW_OTLP_ENDPOINT=http://localhost:4317`
3. Check OTel Collector logs: `docker-compose logs otel-collector`
4. Ensure Stepflow can reach localhost:4317 (not blocked by firewall)

### No Logs in Loki

**Check:**
1. Verify `STEPFLOW_LOG_DESTINATION=otlp` (logs must be sent to OTLP endpoint)
2. Verify `STEPFLOW_OTLP_ENDPOINT=http://localhost:4317`
3. Check Loki logs: `docker-compose logs loki`
4. Verify OTel Collector → Loki pipeline: `docker-compose logs otel-collector | grep loki`

**Note:** When `log_destination=stdout` or `log_destination=file`, logs are NOT sent to OTLP and won't appear in Loki.

### No Metrics in Prometheus

**Check:**
1. Verify Stepflow is exporting metrics via OTLP
2. Check if Prometheus is scraping OTel Collector: http://localhost:9090/targets
3. Look for target: `stepflow-via-otel` (should be UP)

### Grafana Dashboards Not Loading

**Check:**
1. Ensure dashboard JSON files are in `grafana/provisioning/dashboards/stepflow/`
2. Check Grafana logs: `docker-compose logs grafana`
3. Restart Grafana: `docker-compose restart grafana`

## Performance Impact

The observability stack has minimal performance impact:

- **Tracing**: ~1-5% CPU overhead when enabled
- **Logging**: Depends on log level (info: ~2%, debug: ~5-10%)
- **Metrics**: <1% overhead
- **Network**: OTLP uses efficient gRPC protocol

**Recommendations for production:**
- Use `log-level=info` or `warn`
- Enable sampling for high-volume traces (TODO: configure in OTel Collector)
- Use remote storage for Prometheus/Loki for long-term retention

## Data Retention

| Component | Default Retention | Configurable In |
|-----------|-------------------|----------------|
| Jaeger | In-memory (ephemeral) | Switch to persistent backend in `docker-compose.yml` |
| Prometheus | 15 days | `prometheus/prometheus.yml` |
| Loki | 31 days | `loki/config.yaml` |

## Customization

### Add Custom Metrics

In your Rust code:
```rust
// TODO: Example using metrics crate with OTLP exporter
```

### Add Custom Dashboard

1. Create dashboard in Grafana UI
2. Export JSON: Dashboard → Settings → JSON Model → Copy
3. Save to `grafana/provisioning/dashboards/stepflow/<name>.json`
4. Restart Grafana or wait 30s for auto-reload

### Modify Log Parsing

Edit `otel-collector/config.yaml`:
```yaml
processors:
  attributes:
    actions:
      - key: custom_field
        action: insert
        value: "custom_value"
```

## License Notes

### AGPL Components

This observability stack includes the following AGPL 3.0 licensed components:

- **Grafana**: https://github.com/grafana/grafana
- **Loki**: https://github.com/grafana/loki

These components are provided as **official Docker images** (`grafana/grafana`, `grafana/loki`) for local development and testing.

**Compliance for Local Development:** No action required. AGPL does not apply to local use.

**Compliance for Distribution:** When distributing Stepflow with this observability stack:
- The Docker Compose file references official images (not modified binaries)
- This README provides links to source code (above)
- Users download images from Grafana Labs' Docker registry

**Compliance for SaaS/Hosted Services:** If you run Stepflow as a service where users access Grafana over the network:
- Using unmodified official images: Link to https://github.com/grafana/grafana in your UI
- If you modify Grafana/Loki source: You must provide source code to users

**Alternatives:** If AGPL is not suitable for your use case, consider:
- Prometheus + custom UI (Apache 2.0)
- Chronograf instead of Grafana (MIT)
- Fluentd + S3 instead of Loki (Apache 2.0)

### Apache 2.0 Components

- OpenTelemetry Collector: https://github.com/open-telemetry/opentelemetry-collector
- Jaeger: https://github.com/jaegertracing/jaeger
- Prometheus: https://github.com/prometheus/prometheus

## Further Reading

- [Stepflow Observability Crate](../../stepflow-rs/crates/stepflow-observability/)
- [OpenTelemetry Documentation](https://opentelemetry.io/docs/)
- [Jaeger Documentation](https://www.jaegertracing.io/docs/)
- [Prometheus Documentation](https://prometheus.io/docs/)
- [Loki Documentation](https://grafana.com/docs/loki/)
- [Grafana Documentation](https://grafana.com/docs/grafana/)

## Contributing

To improve the observability stack:

1. Test with real workflows and report issues
2. Add new Grafana dashboards for specific use cases
3. Improve alert rules in Prometheus
4. Enhance log parsing in OTel Collector
5. Document best practices for production deployments
