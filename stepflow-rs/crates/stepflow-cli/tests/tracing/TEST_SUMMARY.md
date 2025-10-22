# Tracing Integration Tests - Summary

## Overview

These tests verify end-to-end distributed tracing across the Rust runtime and Python SDK using subprocess-based CLI execution with OTLP export.

## Key Design Decisions

### Environment-Based Configuration
- Uses environment variables (`STEPFLOW_TRACE_ENABLED`, `STEPFLOW_OTLP_ENDPOINT`) instead of in-process observability setup
- Subprocess execution via `stepflow run` CLI command
- Separate OTLP collector instance per test for isolation

### Semantic Structure Validation
Tests focus on verifying the trace structure matches workflow execution semantics:
- Root `flow_execution` span with `trace_id == run_id`
- Correct number of `step` spans matching workflow definition
- Proper parent-child relationships

**What we DON'T test:**
- Generic trace hierarchy properties (reachability, orphaned spans)
- These are the responsibility of the tracing library (fastrace/OpenTelemetry)

## Test 1: `test_simple_workflow_tracing`

**Workflow:** `simple_workflow.yaml` (3 steps, all builtin components)

**Expected Trace Structure:**
```
flow_execution (ROOT, trace_id == run_id)
├─ step (create_msg1)
├─ step (create_msg2)
└─ step (store_result)
```

**Assertions:**
1. Workflow execution succeeds
2. Traces are collected (not empty)
3. Root span exists with name "flow_execution"
4. Root span trace_id matches run_id
5. Exactly 3 step spans as direct children of root
6. All child spans have name "step"

## Test 2: `test_bidirectional_workflow_tracing`

**Workflow:** `bidirectional_workflow.yaml` (4 steps, includes Python UDF with bidirectional calls)

**Expected Trace Structure:**
```
flow_execution (ROOT, trace_id == run_id)
├─ step (store_input_data)
├─ step (create_processor_function)
├─ step (process_data) - Python UDF that calls context.get_blob/put_blob
│  └─ component:/udf (Python SDK component execution)
│     ├─ get_blob (bidirectional call to Rust)
│     ├─ get_blob (bidirectional call to Rust)
│     ├─ put_blob (bidirectional call to Rust)
│     └─ compile_function (Python internal span)
└─ step (get_result)
```

**Assertions:**
1. Workflow execution succeeds
2. Traces are collected from both Rust and Python processes
3. Root span exists with name "flow_execution"
4. Root span trace_id matches run_id
5. Exactly 4 step spans as direct children of root
6. All child spans have name "step"
7. Python component execution span (component:/udf) exists as child of step span
8. At least 2 get_blob spans exist as children of component span
9. At least 1 put_blob span exists as child of component span
10. Traces demonstrate cross-process propagation (Python subprocess shares trace_id)
11. Logs contain run_id attribute in diagnostic context
12. Logs with trace context have trace_id and span_id attributes
13. Log run_id matches workflow run_id

## Test Infrastructure

### Components
- **OTLP Collector:** Docker container (`otel/opentelemetry-collector:latest`)
- **Trace Storage:** JSONL files in `target/tracing-tests/<test>-<timestamp>/`
- **CLI Execution:** Subprocess with environment variables
- **Python SDK:** Subprocess for Python components (bidirectional test)

### Environment Variables

**Rust Runtime (CLI):**
- `STEPFLOW_TRACE_ENABLED=true` - Enable distributed tracing
- `STEPFLOW_OTLP_ENDPOINT=http://localhost:<port>` - Collector endpoint
- `STEPFLOW_LOG_LEVEL=info` - Log level
- **NOT set:** `STEPFLOW_LOG_DESTINATION=otlp` (causes synchronous connection issues)

**Python SDK (Component Server):**
- Environment variables are passed through `stepflow-config.yml`:
  ```yaml
  python:
    type: stepflow
    transport: stdio
    env:
      STEPFLOW_TRACE_ENABLED: "${STEPFLOW_TRACE_ENABLED:-false}"
      STEPFLOW_OTLP_ENDPOINT: "${STEPFLOW_OTLP_ENDPOINT}"
      STEPFLOW_SERVICE_NAME: "${STEPFLOW_SERVICE_NAME:-stepflow-python}"
  ```
- This enables Python SDK spans to be exported to the same OTLP collector

### Utilities

**`otlp_types.rs`:** Minimal OTLP type definitions for parsing
- `OtlpTrace`, `ResourceSpans`, `ScopeSpans`, `Span`
- Helper methods: `is_root()`, `has_parent()`, `belongs_to_trace()`

**`trace_analysis.rs`:** Trace querying and validation utilities
- `read_traces()` - Parse JSONL trace file
- `find_root_span()` - Find root span by trace_id
- `find_child_spans()` - Find children by parent_span_id
- `count_spans_in_trace()` - Count spans in a trace
- `print_trace_tree()` - Debug visualization

**`collector.rs`:** OTLP collector lifecycle management
- `start_otlp_collector()` - Spawn Docker container
- `CollectorGuard` - RAII cleanup on drop

## Running the Tests

```bash
# Run all tracing tests (requires Docker)
cargo test -p stepflow-cli --test test_tracing -- --ignored --nocapture

# Run specific test
cargo test -p stepflow-cli --test test_tracing test_simple_workflow_tracing -- --ignored --nocapture
cargo test -p stepflow-cli --test test_tracing test_bidirectional_workflow_tracing -- --ignored --nocapture
```

Tests are marked with `#[ignore]` because they require Docker for the OTLP collector.

## Key Fixes During Development

1. **OTLP Connection Issue:** Removed `STEPFLOW_LOG_DESTINATION=otlp` which caused synchronous connection during CLI startup
2. **Blob Type Values:** Fixed invalid blob_type values ("result"/"function" → "data")
3. **File Paths:** Added `stepflow-rs/` prefix for workspace-relative paths
4. **Run ID Parsing:** Parse from stderr (where run_id is logged) with fallback to trace extraction
5. **Span Names:** Changed expectations from "step_execution" to "step"
6. **Step Ordering:** Fixed dependency order in bidirectional_workflow.yaml
7. **Test Focus:** Removed generic hierarchy validation, focused on semantic structure

## Observed Trace Structure

**Simple Workflow (builtin components only):**
- Root: `flow_execution` span
- Children: All `step` spans as direct children (flat hierarchy)

**Bidirectional Workflow (with Python SDK):**
- Root: `flow_execution` span
- Children: `step` spans as direct children
- Python step has nested structure:
  - `component:/udf` (Python SDK component execution)
    - `get_blob` spans (bidirectional calls to Rust)
    - `put_blob` spans (bidirectional calls to Rust)
    - `compile_function` (Python internal operation)

**Key Observations:**
- Python SDK creates rich trace hierarchy with component execution and bidirectional call spans
- Rust runtime creates flat step hierarchy
- Trace context propagates correctly across process boundaries (same trace_id)
- Python SDK service name can be configured via `STEPFLOW_SERVICE_NAME` environment variable
- Logs include diagnostic context (`diags` field) with run_id, trace_id, span_id, flow_id, step_id
- Log format is JSON for easier parsing and verification
- Logs are written to stdout along with workflow output
- Not all logs have trace context (early initialization logs before workflow starts)
- Logs that occur during workflow execution include full trace context
