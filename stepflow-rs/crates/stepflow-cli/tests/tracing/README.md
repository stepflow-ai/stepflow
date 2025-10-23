# Distributed Tracing Integration Tests

This directory contains integration tests for Stepflow's distributed tracing capabilities across Rust runtime and Python component servers.

## Overview

These tests verify end-to-end distributed tracing using:
- **Subprocess-based testing**: Uses `stepflow run` CLI command instead of in-process execution
- **Environment variable configuration**: Configures observability like production environments
- **OTLP collector**: Docker-based OpenTelemetry collector for trace capture
- **Comprehensive verification**: Validates trace hierarchies, parent-child relationships, and trace propagation

## Architecture

### Key Components

1. **`otlp_types.rs`**: Minimal OTLP trace format types
   - `OtlpTrace`, `Span`, `ResourceSpans`, `ScopeSpans`
   - Only models fields needed for verification (not full OTLP spec)

2. **`trace_analysis.rs`**: Trace analysis utilities
   - `read_traces()`: Parse JSONL trace files
   - `find_spans_by_name()`: Find spans by name
   - `find_root_span()`: Find root span by trace ID
   - `find_child_spans()`: Find children of a span
   - `verify_trace_tree()`: Comprehensive trace hierarchy verification

3. **`collector.rs`**: OTLP collector management
   - `start_otlp_collector()`: Start testcontainers-based collector
   - `CollectorGuard`: RAII guard for automatic cleanup

4. **`workflows/`**: Test workflow definitions
   - `simple_workflow.yaml`: Basic multi-step workflow (builtin components only)
   - `bidirectional_workflow.yaml`: Python components with blob operations
   - `stepflow-config.yml`: Test configuration with Python SDK

5. **`mod.rs`**: Test implementations
   - `test_simple_workflow_tracing`: Basic tracing verification
   - `test_bidirectional_workflow_tracing`: Rust ↔ Python distributed tracing

## Test Structure

### Simple Workflow Test
```bash
# Start OTLP collector
# Run: stepflow run --flow=simple_workflow.yaml --input-json='{...}'
#      with STEPFLOW_TRACE_ENABLED=true
# Verify:
#   - Root flow_execution span exists
#   - Step execution spans are children
#   - Trace ID propagates correctly
```

### Bidirectional Workflow Test
```bash
# Start OTLP collector
# Run: stepflow run --flow=bidirectional_workflow.yaml --input-json='{...}'
#      with STEPFLOW_TRACE_ENABLED=true (Rust and Python)
# Verify:
#   - Root flow_execution span (Rust)
#   - Step execution spans (Rust)
#   - Component execution spans (Python)
#   - Bidirectional call spans (Python → Rust)
#   - Parent-child relationships across boundaries
#   - Trace ID matches run_id
```

## Running Tests

Tests are marked with `#[ignore]` because they require Docker:

```bash
# Run all tracing tests
cargo test -p stepflow-cli --test test_tracing -- --ignored --nocapture

# Run specific test
cargo test -p stepflow-cli --test test_tracing test_simple_workflow_tracing -- --ignored --nocapture
```

## Environment Variables

Tests set the following environment variables for subprocess execution:

- `STEPFLOW_TRACE_ENABLED=true`: Enable distributed tracing
- `STEPFLOW_OTLP_ENDPOINT=http://localhost:<port>`: OTLP collector endpoint
- `STEPFLOW_LOG_DESTINATION=otlp`: Send logs to OTLP (not file)
- `STEPFLOW_LOG_LEVEL=info`: Log level
- `STEPFLOW_SERVICE_NAME=stepflow-test-python`: Service name for Python SDK

## Trace Verification

The tests perform comprehensive verification:

1. **Trace Collection**: Read traces from JSONL file
2. **Root Span**: Find `flow_execution` span with trace_id == run_id
3. **Hierarchy**: Verify all spans are reachable from root
4. **Parent-Child**: Verify correct parent_span_id relationships
5. **Trace Propagation**: Verify trace_id matches across all spans
6. **Span Types**: Verify expected span types exist
   - `flow_execution` (Rust root)
   - `step_execution` (Rust steps)
   - `component_execute` (Python components)
   - Bidirectional call spans (Python ↔ Rust)

## Improvements Over Previous Tests

Compared to `stepflow-server/tests/tracing_integration_tests.rs`:

1. **No in-process observability setup**: Uses environment variables like production
2. **Subprocess execution**: Uses `stepflow run` CLI command
3. **Strongly-typed parsing**: Minimal OTLP structs instead of `serde_json::Value`
4. **Comprehensive verification**: Checks parent-child relationships and trace propagation
5. **Better isolation**: Each test has its own OTLP collector instance
6. **Proper test location**: CLI tests in CLI crate

## Docker Requirements

Tests require Docker (or Colima) for the OTLP collector container:

- Automatically detects Colima: `~/.colima/default/docker.sock`
- Pulls `otel/opentelemetry-collector:latest`
- Exposes ports 4317 (gRPC) and 4318 (HTTP)
- Waits for "Everything is ready" message

## Trace Output

Traces are collected in:
```
target/tracing-tests/<test_name>-<timestamp>/traces.jsonl
```

Each line is a JSON object containing an OTLP trace batch.

## Debugging

To debug failing tests:

1. Run with `--nocapture` to see output
2. Look for trace tree in output (calls `print_trace_tree()`)
3. Check trace file manually: `cat target/tracing-tests/*/traces.jsonl | jq`
4. Inspect `TraceVerificationResult::errors` for specific issues
