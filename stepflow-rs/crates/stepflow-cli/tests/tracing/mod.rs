// Copyright 2025 DataStax Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
// in compliance with the License. You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied. See the License for the specific language governing permissions and limitations under
// the License.

#![allow(clippy::print_stderr)]

//! Distributed tracing integration tests
//!
//! These tests verify end-to-end distributed tracing across Rust runtime and Python component
//! servers using subprocess-based CLI execution with OTLP export.
//!
//! Key improvements over previous tracing tests:
//! - Uses environment variables for observability configuration (no in-process setup)
//! - Uses `stepflow run` CLI command instead of in-process server
//! - Comprehensive trace verification including parent-child relationships
//! - Strongly-typed OTLP trace parsing
//! - Proper trace filtering by run_id

mod collector;
mod log_analysis;
mod otlp_log_types;
mod otlp_types;
mod trace_analysis;

use collector::start_otlp_collector;
use insta_cmd::Command;
use std::path::Path;
use trace_analysis::{
    count_spans_in_trace, find_batch_span, find_child_spans, find_flow_span_by_run_id,
    find_root_span, find_spans_by_name, print_trace_tree, read_traces,
    verify_batch_span_attributes, verify_parent_child_relationship,
};

/// Create a stepflow command for testing
fn stepflow() -> Command {
    let mut command = Command::new(insta_cmd::get_cargo_bin("stepflow"));

    // Locate the cargo workspace
    let path = Path::new(std::env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("Failed to locate workspace root");
    command.current_dir(path);

    // Use default stdout for logging (will be JSON formatted by test config)
    command.arg("--omit-stack-trace");

    command
}

/// Parse run_id from stepflow run command output or stderr
///
/// Expected format: "run_id: <uuid>" or extract from diagnostic context
fn parse_run_id(output: &str) -> Option<String> {
    // Try to find "run_id: <uuid>" format (older format)
    for line in output.lines() {
        if let Some(run_id) = line.strip_prefix("run_id: ") {
            return Some(run_id.trim().to_string());
        }
    }

    // Try to extract from diagnostic context format: "run_id":"<uuid>"
    if let Some(start) = output.find(r#""run_id":""#) {
        let start_idx = start + r#""run_id":""#.len();
        if let Some(end_idx) = output[start_idx..].find('"') {
            return Some(output[start_idx..start_idx + end_idx].to_string());
        }
    }

    None
}

/// Find the most recent trace_id (root span) from the traces
///
/// This is used as a fallback when we can't parse run_id from command output
fn find_most_recent_trace_id(traces: &[otlp_types::OtlpTrace]) -> Option<String> {
    traces
        .iter()
        .flat_map(|t| t.all_spans())
        .filter(|s| s.is_root())
        .map(|s| s.trace_id())
        .next()
}

/// Test: Simple workflow execution with observability
///
/// This test verifies that:
/// - Workflow execution creates proper trace spans
/// - Step execution spans are children of workflow span
/// - All spans are exported to OTLP correctly
/// - Trace IDs propagate correctly
#[tokio::test]
#[ignore] // Requires Docker for OTLP collector
async fn test_simple_workflow_tracing() {
    let collector = start_otlp_collector("simple_workflow").await;

    // Run workflow with tracing enabled
    // Note: We use stderr for logs (not OTLP) to avoid synchronous connection issues.
    // We'll verify diagnostic context in stderr output instead.
    let mut cmd = stepflow();
    cmd.arg("run")
        .arg("--flow=stepflow-rs/crates/stepflow-cli/tests/tracing/workflows/simple_workflow.yaml")
        .arg("--config=stepflow-rs/crates/stepflow-cli/tests/tracing/workflows/stepflow-config.yml")
        .arg(r#"--input-json={"message1": "Hello", "message2": "World"}"#)
        .env("STEPFLOW_TRACE_ENABLED", "true")
        .env("STEPFLOW_OTLP_ENDPOINT", collector.grpc_endpoint())
        .env("STEPFLOW_LOG_LEVEL", "info")
        .env("STEPFLOW_LOG_FORMAT", "json"); // Use JSON format for easier parsing

    eprintln!("🚀 Running simple workflow with tracing...");
    let output = cmd.output().expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        eprintln!("❌ Command failed with status: {:?}", output.status);
        eprintln!("STDOUT:\n{}", stdout);
        eprintln!("STDERR:\n{}", stderr);
    }

    assert!(
        output.status.success(),
        "Command failed with status {:?}\nSTDERR:\n{}",
        output.status,
        stderr
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    eprintln!("Command STDOUT:\n{}", stdout);
    eprintln!("Command STDERR:\n{}", stderr);

    // Wait for traces to be exported
    eprintln!("⏳ Waiting for traces to export...");
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // Read and verify traces
    let trace_file = collector.trace_file();
    eprintln!("📁 Reading traces from: {}", trace_file.display());

    let traces = read_traces(&trace_file);
    eprintln!("📦 Found {} trace records", traces.len());

    if traces.is_empty() {
        panic!("No traces collected");
    }

    // Parse run_id from output
    let run_id = parse_run_id(&stderr)
        .or_else(|| parse_run_id(&stdout))
        .expect("Failed to find run_id from output");
    eprintln!("📊 Run ID: {}", run_id);

    // Find the flow execution span by run_id attribute
    let root_span = find_flow_span_by_run_id(&traces, &run_id);
    assert!(
        root_span.is_some(),
        "Should find flow_execution span with run_id attribute"
    );
    let root_span = root_span.unwrap();

    eprintln!("📊 Trace ID: {}", root_span.trace_id());

    // Print trace tree for debugging
    print_trace_tree(&traces, &root_span.trace_id());

    // Verify the semantic structure of the trace matches workflow execution:
    // - Run (flow_execution with run_id attribute)
    //   - Step 1 (create_msg1)
    //   - Step 2 (create_msg2)
    //   - Step 3 (store_result)

    // 1. Verify root span attributes
    assert_eq!(
        root_span.name, "flow_execution",
        "Root span should be named 'flow_execution'"
    );
    assert_eq!(
        root_span.get_string_attribute("run_id"),
        Some(run_id.as_str()),
        "Root span should have run_id attribute"
    );

    // 2. Verify step spans: 3 steps as direct children of flow_execution
    let step_children = find_child_spans(&traces, &root_span.span_id());
    assert_eq!(
        step_children.len(),
        3,
        "Should have exactly 3 step spans as children of flow_execution (create_msg1, create_msg2, store_result)"
    );

    // All children should be named "step"
    for child in &step_children {
        assert_eq!(child.name, "step", "Child span should be named 'step'");
    }

    eprintln!("✅ Simple workflow tracing test passed");
    eprintln!("   - Verified flow_execution root span");
    eprintln!("   - Verified run_id as span attribute");
    eprintln!("   - Verified 3 step spans as direct children");
}

/// Test: Bidirectional workflow observability
///
/// This test verifies that:
/// - Multi-step workflows with Python components create proper trace spans
/// - Distributed tracing works across Rust ↔ Python boundaries
/// - Bidirectional calls (context.get_blob, context.put_blob) are traced
/// - All spans maintain correct parent-child relationships
/// - Trace IDs propagate across the entire execution graph
#[tokio::test]
#[ignore] // Requires Docker for OTLP collector
async fn test_bidirectional_workflow_tracing() {
    let collector = start_otlp_collector("bidirectional_workflow").await;

    // Run workflow with tracing enabled (both Rust and Python)
    // Note: We use stderr for logs (not OTLP) to avoid synchronous connection issues.
    // We'll verify diagnostic context in stderr output instead.
    let mut cmd = stepflow();
    cmd.arg("run")
        .arg("--flow=stepflow-rs/crates/stepflow-cli/tests/tracing/workflows/bidirectional_workflow.yaml")
        .arg("--config=stepflow-rs/crates/stepflow-cli/tests/tracing/workflows/stepflow-config.yml")
        .arg(r#"--input-json={"data": [1, 2, 3, 4, 5], "multiplier": 2}"#)
        .env("STEPFLOW_TRACE_ENABLED", "true")
        .env("STEPFLOW_OTLP_ENDPOINT", collector.grpc_endpoint())
        .env("STEPFLOW_LOG_LEVEL", "info")
        .env("STEPFLOW_LOG_FORMAT", "json") // Use JSON format for easier parsing
        .env("STEPFLOW_SERVICE_NAME", "stepflow-test-python"); // For Python SDK

    eprintln!("🚀 Running bidirectional workflow with tracing...");
    let output = cmd.output().expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        eprintln!("❌ Command failed with status: {:?}", output.status);
        eprintln!("STDOUT:\n{}", stdout);
        eprintln!("STDERR:\n{}", stderr);
    }

    assert!(
        output.status.success(),
        "Command failed with status {:?}\nSTDERR:\n{}",
        output.status,
        stderr
    );

    eprintln!("Command STDOUT:\n{}", stdout);
    eprintln!("Command STDERR:\n{}", stderr);

    // Wait longer for traces (Python subprocess needs more time)
    eprintln!("⏳ Waiting for traces to export...");
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // Read and verify traces
    let trace_file = collector.trace_file();
    eprintln!("📁 Reading traces from: {}", trace_file.display());

    let traces = read_traces(&trace_file);
    eprintln!("📦 Found {} trace records", traces.len());

    if traces.is_empty() {
        panic!("No traces collected");
    }

    // Parse run_id from output or find it from traces
    // Prefer stderr (simple text format) over stdout (JSON with UUID format)
    let run_id = parse_run_id(&stderr)
        .or_else(|| parse_run_id(&stdout))
        .or_else(|| find_most_recent_trace_id(&traces))
        .expect("Failed to find run_id (trace_id) from output or traces");
    eprintln!("📊 Run ID (trace_id): {}", run_id);

    // Convert trace_id (hex without hyphens) to UUID format (hex with hyphens)
    // UUID format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx (8-4-4-4-12)
    let run_id_with_hyphens = if run_id.len() == 32 && run_id.chars().all(|c| c.is_ascii_hexdigit())
    {
        format!(
            "{}-{}-{}-{}-{}",
            &run_id[0..8],
            &run_id[8..12],
            &run_id[12..16],
            &run_id[16..20],
            &run_id[20..32]
        )
    } else {
        run_id.clone()
    };

    // Print trace tree for debugging
    print_trace_tree(&traces, &run_id);

    // Verify the semantic structure of the trace matches workflow execution:
    // - Run (flow_execution, trace_id == run_id)
    //   - Step 1 (store_input_data) - builtin put_blob
    //   - Step 2 (create_processor_function) - builtin put_blob
    //   - Step 3 (process_data) - Python UDF that calls context.get_blob() and context.put_blob()
    //     - component:/udf (Python SDK component execution)
    //       - get_blob (bidirectional call to Rust)
    //       - get_blob (bidirectional call to Rust)
    //       - put_blob (bidirectional call to Rust)
    //       - compile_function (Python internal span)
    //   - Step 4 (get_result) - builtin get_blob

    // 1. Verify root span: flow_execution with trace_id == run_id
    let root_span = find_root_span(&traces, &run_id);
    assert!(
        root_span.is_some(),
        "Should find flow_execution root span with trace_id matching run_id"
    );
    let root_span = root_span.unwrap();
    assert_eq!(
        root_span.name, "flow_execution",
        "Root span should be named 'flow_execution'"
    );
    assert_eq!(
        root_span.trace_id(),
        run_id.to_lowercase(),
        "trace_id should match run_id"
    );

    // 2. Verify step spans: 4 steps as direct children of flow_execution
    let step_children = find_child_spans(&traces, &root_span.span_id());
    assert_eq!(
        step_children.len(),
        4,
        "Should have exactly 4 step spans as children of flow_execution (store_input_data, create_processor_function, process_data, get_result)"
    );

    // All children should be named "step"
    for child in &step_children {
        assert_eq!(child.name, "step", "Child span should be named 'step'");
    }

    // 3. Verify Python SDK component execution span
    let component_spans = find_spans_by_name(&traces, "component:/udf");
    assert_eq!(
        component_spans.len(),
        1,
        "Should find exactly 1 Python component execution span"
    );
    let component_span = component_spans[0];

    // The component span should be a child of one of the step spans
    let parent_step = step_children
        .iter()
        .find(|step| component_span.has_parent(&step.span_id()));
    assert!(
        parent_step.is_some(),
        "Component span should be a child of a step span"
    );

    // 4. Verify bidirectional calls from Python to Rust
    let get_blob_spans = find_spans_by_name(&traces, "get_blob");
    let put_blob_spans = find_spans_by_name(&traces, "put_blob");

    assert!(
        get_blob_spans.len() >= 2,
        "Should find at least 2 get_blob spans from Python component"
    );
    assert!(
        !put_blob_spans.is_empty(),
        "Should find at least 1 put_blob span from Python component"
    );

    // Verify bidirectional call spans are children of the component span
    for span in &get_blob_spans {
        assert!(
            span.has_parent(&component_span.span_id()),
            "get_blob span should be a child of component span"
        );
    }
    for span in &put_blob_spans {
        assert!(
            span.has_parent(&component_span.span_id()),
            "put_blob span should be a child of component span"
        );
    }

    // 5. Verify complete trace structure
    let total_spans = count_spans_in_trace(&traces, &run_id);
    eprintln!("📊 Total spans in trace: {}", total_spans);

    // Print all unique span names for visibility
    let all_span_names: std::collections::HashSet<_> = traces
        .iter()
        .flat_map(|t| t.all_spans())
        .map(|s| &s.name)
        .collect();
    eprintln!("📋 Unique span names in trace:");
    for name in &all_span_names {
        eprintln!("  - {}", name);
    }

    eprintln!("✅ Bidirectional workflow tracing test passed");
    eprintln!("   - Verified flow_execution root span");
    eprintln!("   - Verified 4 step spans as direct children");
    eprintln!("   - Verified Python component execution span (component:/udf)");
    eprintln!(
        "   - Verified {} get_blob bidirectional calls",
        get_blob_spans.len()
    );
    eprintln!(
        "   - Verified {} put_blob bidirectional calls",
        put_blob_spans.len()
    );
    eprintln!("   - Verified trace_id == run_id");
    eprintln!("   - Verified trace propagation across Rust ↔ Python boundary");

    // 6. Verify logs contain diagnostic context (run_id, trace_id, span_id)
    // Parse JSON logs from stdout
    eprintln!("📋 Verifying log diagnostic context...");

    let json_logs: Vec<serde_json::Value> = stdout
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();

    eprintln!("📦 Found {} JSON log records in stdout", json_logs.len());

    // Find logs with run_id matching our workflow (use UUID format with hyphens)
    eprintln!("🔍 Looking for logs with run_id: {}", run_id_with_hyphens);
    let logs_with_run_id: Vec<_> = json_logs
        .iter()
        .filter(|log| {
            log.get("diags")
                .and_then(|d| d.get("run_id"))
                .and_then(|v| v.as_str())
                .map(|rid| rid.to_lowercase() == run_id_with_hyphens.to_lowercase())
                .unwrap_or(false)
        })
        .collect();

    assert!(
        !logs_with_run_id.is_empty(),
        "Should find log records with run_id matching workflow run_id"
    );
    eprintln!(
        "✓ {}/{} log records have run_id attribute",
        logs_with_run_id.len(),
        json_logs.len()
    );

    let logs_with_flow_id: Vec<_> = json_logs
        .iter()
        .filter(|log| {
            log.get("diags")
                .and_then(|d| d.get("flow_id"))
                .and_then(|v| v.as_str())
                .is_some()
        })
        .collect();
    assert!(
        !logs_with_flow_id.is_empty(),
        "Should find log records with flow_id attribute"
    );
    eprintln!(
        "✓ {}/{} log records have flow_id attribute",
        logs_with_flow_id.len(),
        json_logs.len()
    );

    // Verify logs have trace context (trace_id, span_id) in diags field
    let logs_with_trace_context: Vec<_> = logs_with_run_id
        .iter()
        .filter(|log| {
            let diags = log.get("diags");
            let has_trace_id = diags
                .and_then(|d| d.get("trace_id"))
                .and_then(|v| v.as_str())
                .is_some();
            let has_span_id = diags
                .and_then(|d| d.get("span_id"))
                .and_then(|v| v.as_str())
                .is_some();
            has_trace_id && has_span_id
        })
        .collect();

    // Note: Logs that occur within workflow execution should have both run_id and trace context
    // Early initialization logs may only have run_id
    if !logs_with_trace_context.is_empty() {
        eprintln!(
            "✓ {}/{} log records have trace context (trace_id, span_id)",
            logs_with_trace_context.len(),
            logs_with_run_id.len()
        );

        // Verify trace_id matches run_id (without hyphens)
        for log in &logs_with_trace_context {
            if let Some(diags) = log.get("diags")
                && let Some(_log_trace_id) = diags.get("trace_id").and_then(|v| v.as_str())
            {
                // trace_id is stored as decimal in logs, but we can verify run_id matches
                if let Some(log_run_id) = diags.get("run_id").and_then(|v| v.as_str()) {
                    assert_eq!(
                        log_run_id.replace("-", "").to_lowercase(),
                        run_id.to_lowercase(),
                        "Log run_id (without hyphens) should match trace_id"
                    );
                }
            }
        }
    }

    eprintln!("✅ Log verification passed");
    eprintln!(
        "   - Verified {} log records with run_id",
        logs_with_run_id.len()
    );
    if !logs_with_trace_context.is_empty() {
        eprintln!(
            "   - Verified {} logs have trace context (trace_id, span_id)",
            logs_with_trace_context.len()
        );
    }
}

/// Test: Batch execution tracing
///
/// This test verifies that:
/// - Batch execution creates a root batch_execution span
/// - Each batch item creates a child flow_execution span
/// - Batch span has correct attributes (batch.total_items, batch.max_concurrency)
/// - All child spans maintain correct parent-child relationships
/// - Trace IDs propagate correctly through the batch
#[tokio::test]
#[ignore] // Requires Docker for OTLP collector
async fn test_batch_execution_tracing() {
    let collector = start_otlp_collector("batch_execution").await;

    // Run batch with tracing enabled
    let mut cmd = stepflow();
    cmd.arg("run-batch")
        .arg("--flow=stepflow-rs/crates/stepflow-cli/tests/tracing/workflows/simple_workflow.yaml")
        .arg("--config=stepflow-rs/crates/stepflow-cli/tests/tracing/workflows/stepflow-config.yml")
        .arg("--inputs=stepflow-rs/crates/stepflow-cli/tests/tracing/workflows/batch_inputs.jsonl")
        .arg("--max-concurrent=2")
        .env("STEPFLOW_TRACE_ENABLED", "true")
        .env("STEPFLOW_OTLP_ENDPOINT", collector.grpc_endpoint())
        .env("STEPFLOW_LOG_LEVEL", "info")
        .env("STEPFLOW_LOG_FORMAT", "json");

    eprintln!("🚀 Running batch workflow with tracing...");
    let output = cmd.output().expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        eprintln!("❌ Command failed with status: {:?}", output.status);
        eprintln!("STDOUT:\n{}", stdout);
        eprintln!("STDERR:\n{}", stderr);
    }

    assert!(
        output.status.success(),
        "Command failed with status {:?}\nSTDERR:\n{}",
        output.status,
        stderr
    );

    eprintln!("Command STDOUT:\n{}", stdout);
    eprintln!("Command STDERR:\n{}", stderr);

    // Wait longer for traces (batch execution needs more time)
    eprintln!("⏳ Waiting for traces to export...");
    tokio::time::sleep(tokio::time::Duration::from_secs(8)).await;

    // Read and verify traces
    let trace_file = collector.trace_file();
    eprintln!("📁 Reading traces from: {}", trace_file.display());

    let traces = read_traces(&trace_file);
    eprintln!("📦 Found {} trace records", traces.len());

    if traces.is_empty() {
        panic!("No traces collected");
    }

    // Parse batch_id from stderr
    let batch_id = stderr
        .lines()
        .find_map(|line| line.strip_prefix("batch_id: "))
        .map(|id| id.trim().to_string())
        .expect("Failed to find batch_id in output");
    eprintln!("📊 Batch ID: {}", batch_id);

    // Find the batch execution span by batch_id attribute
    let batch_span = find_batch_span(&traces, &batch_id);
    assert!(
        batch_span.is_some(),
        "Should find batch_execution span with batch_id attribute"
    );
    let batch_span = batch_span.unwrap();

    eprintln!("📊 Trace ID: {}", batch_span.trace_id());

    // Print trace tree for debugging
    print_trace_tree(&traces, &batch_span.trace_id());

    // Verify the semantic structure of the trace:
    // - Batch (batch_execution with batch_id attribute)
    //   - Flow 1 (flow_execution, child of batch)
    //   - Flow 2 (flow_execution, child of batch)
    //   - Flow 3 (flow_execution, child of batch)

    // 1. Verify batch root span
    assert_eq!(
        batch_span.name, "batch_execution",
        "Root span should be named 'batch_execution'"
    );
    assert_eq!(
        batch_span.get_string_attribute("batch_id"),
        Some(batch_id.as_str()),
        "Batch span should have batch_id attribute"
    );

    // 2. Verify batch span attributes
    verify_batch_span_attributes(batch_span, 3, 2)
        .expect("Batch span should have correct attributes");

    // 3. Verify flow execution children
    let flow_children = find_child_spans(&traces, &batch_span.span_id());
    assert_eq!(
        flow_children.len(),
        3,
        "Should have exactly 3 flow_execution spans as children of batch_execution"
    );

    // All children should be named "flow_execution"
    for child in &flow_children {
        assert_eq!(
            child.name, "flow_execution",
            "Child span should be named 'flow_execution'"
        );

        // Verify parent-child relationship
        verify_parent_child_relationship(child, batch_span)
            .expect("Flow execution should be child of batch execution");

        // Each flow should have step children
        let step_children = find_child_spans(&traces, &child.span_id());
        assert_eq!(
            step_children.len(),
            3,
            "Each flow should have 3 step spans (create_msg1, create_msg2, store_result)"
        );
    }

    eprintln!("✅ Batch execution tracing test passed");
    eprintln!("   - Verified batch_execution root span");
    eprintln!("   - Verified batch_id as span attribute");
    eprintln!("   - Verified batch span attributes (total_items=3, max_concurrency=2)");
    eprintln!("   - Verified 3 flow_execution spans as direct children");
    eprintln!("   - Verified parent-child relationships");
}
