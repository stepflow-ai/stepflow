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

//! Integration tests that run example binaries and verify diagnostic output

use serde_json::Value;
use std::process::Command;

/// Parse JSON log lines from output
fn parse_json_logs(output: &str) -> Vec<Value> {
    output
        .lines()
        .filter(|line| !line.starts_with("DEBUG:")) // Skip debug output
        .filter(|line| !line.starts_with("SpanRecord")) // Skip span records
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

#[test]
fn test_basic_example_no_run_diagnostic() {
    // Run the basic example which has BinaryObservabilityConfig::default()
    let output = Command::new("cargo")
        .args(["run", "--example", "basic"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run basic example");

    assert!(
        output.status.success(),
        "Basic example failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let logs = parse_json_logs(&stdout);

    // Find a log inside a span (should have trace context but no run context)
    let span_log = logs
        .iter()
        .find(|log| {
            log.get("message")
                .and_then(|m| m.as_str())
                .map(|m| m.contains("Inside root span"))
                .unwrap_or(false)
        })
        .expect("Could not find 'Inside root span' log");

    // Should have trace diagnostic
    let diags = span_log.get("diags").expect("Log should have diags object");
    assert!(
        diags.get("trace_id").is_some(),
        "Should have trace_id in basic example"
    );
    assert!(
        diags.get("span_id").is_some(),
        "Should have span_id in basic example"
    );

    // Should NOT have run diagnostic
    assert!(
        diags.get("run_id").is_none(),
        "Should NOT have run_id in basic example"
    );
    assert!(
        diags.get("step_id").is_none(),
        "Should NOT have step_id in basic example"
    );
}

#[test]
fn test_run_diagnostic_context_example() {
    // Run the run_diagnostic_context example which enables run diagnostic
    let output = Command::new("cargo")
        .args(["run", "--example", "run_diagnostic_context"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run run_diagnostic_context example");

    assert!(
        output.status.success(),
        "run_diagnostic_context example failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let logs = parse_json_logs(&stdout);

    // Test 1: Log without any context (before RunIdGuard)
    let no_context_log = logs
        .iter()
        .find(|log| {
            log.get("message")
                .and_then(|m| m.as_str())
                .map(|m| m.contains("no context yet"))
                .unwrap_or(false)
        })
        .expect("Could not find 'no context yet' log");

    let diags = no_context_log.get("diags");
    assert!(
        diags.is_none() || diags.unwrap().as_object().unwrap().is_empty(),
        "Should have no diagnostic context before guards"
    );

    // Test 2: Log with run_id but no trace context yet
    let run_only_log = logs
        .iter()
        .find(|log| {
            log.get("message")
                .and_then(|m| m.as_str())
                .map(|m| m.contains("Workflow started"))
                .unwrap_or(false)
        })
        .expect("Could not find 'Workflow started' log");

    let diags = run_only_log
        .get("diags")
        .expect("Should have diags for workflow started");
    assert_eq!(
        diags.get("run_id").and_then(|v| v.as_str()),
        Some("run-12345"),
        "Should have run_id"
    );
    assert!(
        diags.get("trace_id").is_none(),
        "Should not have trace_id yet"
    );
    assert!(
        diags.get("step_id").is_none(),
        "Should not have step_id yet"
    );

    // Test 3: Log with both trace and run context
    let trace_and_run_log = logs
        .iter()
        .find(|log| {
            log.get("message")
                .and_then(|m| m.as_str())
                .map(|m| m.contains("Inside trace span"))
                .unwrap_or(false)
        })
        .expect("Could not find 'Inside trace span' log");

    let diags = trace_and_run_log
        .get("diags")
        .expect("Should have diags for trace span");
    assert!(
        diags.get("trace_id").is_some(),
        "Should have trace_id inside span"
    );
    assert!(
        diags.get("span_id").is_some(),
        "Should have span_id inside span"
    );
    assert_eq!(
        diags.get("run_id").and_then(|v| v.as_str()),
        Some("run-12345"),
        "Should still have run_id inside span"
    );
    assert!(
        diags.get("step_id").is_none(),
        "Should not have step_id yet (not in step)"
    );

    // Test 4: Log with trace, run, and step context
    let step1_logs: Vec<_> = logs
        .iter()
        .filter(|log| {
            log.get("diags")
                .and_then(|d| d.get("step_id"))
                .and_then(|s| s.as_str())
                == Some("step1")
        })
        .collect();

    assert!(
        !step1_logs.is_empty(),
        "Should have logs from step1 execution"
    );

    let step1_started_log = step1_logs
        .iter()
        .find(|log| {
            log.get("message")
                .and_then(|m| m.as_str())
                .map(|m| m == "Step started")
                .unwrap_or(false)
        })
        .expect("Could not find 'Step started' log for step1");

    let step1_diags = step1_started_log
        .get("diags")
        .expect("Should have diags for step");
    assert!(
        step1_diags.get("trace_id").is_some(),
        "Should have trace_id in step"
    );
    assert!(
        step1_diags.get("span_id").is_some(),
        "Should have span_id in step"
    );
    assert_eq!(
        step1_diags.get("run_id").and_then(|v| v.as_str()),
        Some("run-12345"),
        "Should have run_id in step"
    );
    assert_eq!(
        step1_diags.get("step_id").and_then(|v| v.as_str()),
        Some("step1"),
        "Should have step_id='step1' in step"
    );

    // Test 5: Verify step2 has same trace_id, same run_id, but different step_id and span_id
    let step2_logs: Vec<_> = logs
        .iter()
        .filter(|log| {
            log.get("diags")
                .and_then(|d| d.get("step_id"))
                .and_then(|s| s.as_str())
                == Some("step2")
        })
        .collect();

    assert!(
        !step2_logs.is_empty(),
        "Should have logs from step2 execution"
    );

    let step2_started_log = step2_logs
        .iter()
        .find(|log| {
            log.get("message")
                .and_then(|m| m.as_str())
                .map(|m| m == "Step started")
                .unwrap_or(false)
        })
        .expect("Could not find 'Step started' log for step2");

    let step2_diags = step2_started_log.get("diags").expect("Should have diags");

    // Same trace_id across all steps in the workflow
    assert_eq!(
        step1_diags.get("trace_id"),
        step2_diags.get("trace_id"),
        "trace_id should be the same across steps (same workflow trace)"
    );

    // Same run_id across all steps in the workflow
    assert_eq!(
        step2_diags.get("run_id").and_then(|v| v.as_str()),
        Some("run-12345"),
        "run_id should persist across steps"
    );

    // Different step_id for different steps
    assert_eq!(
        step2_diags.get("step_id").and_then(|v| v.as_str()),
        Some("step2"),
        "Should have step_id='step2' in second step"
    );

    // Different span_id for different steps (each step has its own span)
    assert_ne!(
        step1_diags.get("span_id"),
        step2_diags.get("span_id"),
        "span_id should be different for different steps"
    );

    // Test 5b: Verify all logs within step1 share the same span_id (if there are multiple)
    let step1_span_ids: Vec<_> = step1_logs
        .iter()
        .filter_map(|log| {
            log.get("diags")
                .and_then(|d| d.get("span_id"))
                .and_then(|s| s.as_str())
        })
        .collect();

    if step1_span_ids.len() > 1 {
        assert!(
            step1_span_ids.iter().all(|&id| id == step1_span_ids[0]),
            "All logs within step1 should share the same span_id"
        );
    }

    // Test 6: Log after span closed but run_id still active
    let after_span_log = logs
        .iter()
        .find(|log| {
            log.get("message")
                .and_then(|m| m.as_str())
                .map(|m| m.contains("After trace span closed"))
                .unwrap_or(false)
        })
        .expect("Could not find 'After trace span closed' log");

    let diags = after_span_log
        .get("diags")
        .expect("Should have diags after span");
    assert!(
        diags.get("trace_id").is_none(),
        "Should not have trace_id after span closes"
    );
    assert_eq!(
        diags.get("run_id").and_then(|v| v.as_str()),
        Some("run-12345"),
        "run_id should persist after span closes"
    );
    assert!(
        diags.get("step_id").is_none(),
        "step_id should be cleared after steps"
    );

    // Test 7: Log after run_id cleared
    let no_run_log = logs
        .iter()
        .find(|log| {
            log.get("message")
                .and_then(|m| m.as_str())
                .map(|m| m.contains("After run_id cleared"))
                .unwrap_or(false)
        })
        .expect("Could not find 'After run_id cleared' log");

    let diags = no_run_log.get("diags");
    assert!(
        diags.is_none() || diags.unwrap().as_object().unwrap().is_empty(),
        "Should have no diagnostic context after all guards dropped"
    );
}
