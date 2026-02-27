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

//! Metrics instrumentation for Stepflow using OpenTelemetry

use opentelemetry::{
    KeyValue, global,
    metrics::{Counter, Histogram},
};
use std::sync::LazyLock;

/// Global metrics instruments
static WORKFLOW_EXECUTIONS: LazyLock<Counter<u64>> = LazyLock::new(|| {
    let meter = global::meter("stepflow");
    meter
        .u64_counter("workflow_executions_total")
        .with_description("Total number of workflow executions")
        .with_unit("executions")
        .build()
});

static WORKFLOW_DURATION: LazyLock<Histogram<f64>> = LazyLock::new(|| {
    let meter = global::meter("stepflow");
    meter
        .f64_histogram("workflow_duration_seconds")
        .with_description("Workflow execution duration in seconds")
        .with_unit("s")
        .build()
});

/// Record a workflow execution
pub fn record_workflow_execution(outcome: &str, duration_seconds: f64) {
    WORKFLOW_EXECUTIONS.add(1, &[KeyValue::new("outcome", outcome.to_string())]);
    WORKFLOW_DURATION.record(
        duration_seconds,
        &[KeyValue::new("outcome", outcome.to_string())],
    );
}

// =========================================================================
// Blob Store Metrics
// =========================================================================

/// Total bytes stored via put_blob
static BLOB_PUT_BYTES: LazyLock<Counter<u64>> = LazyLock::new(|| {
    let meter = global::meter("stepflow");
    meter
        .u64_counter("blob_store.put.bytes")
        .with_description("Total bytes stored in blob store")
        .with_unit("By")
        .build()
});

/// Total bytes retrieved via get_blob
static BLOB_GET_BYTES: LazyLock<Counter<u64>> = LazyLock::new(|| {
    let meter = global::meter("stepflow");
    meter
        .u64_counter("blob_store.get.bytes")
        .with_description("Total bytes retrieved from blob store")
        .with_unit("By")
        .build()
});

/// Put operation duration
static BLOB_PUT_DURATION: LazyLock<Histogram<f64>> = LazyLock::new(|| {
    let meter = global::meter("stepflow");
    meter
        .f64_histogram("blob_store.put.duration")
        .with_description("Blob store put operation duration")
        .with_unit("s")
        .build()
});

/// Get operation duration
static BLOB_GET_DURATION: LazyLock<Histogram<f64>> = LazyLock::new(|| {
    let meter = global::meter("stepflow");
    meter
        .f64_histogram("blob_store.get.duration")
        .with_description("Blob store get operation duration")
        .with_unit("s")
        .build()
});

/// Blob size distribution
static BLOB_SIZE: LazyLock<Histogram<u64>> = LazyLock::new(|| {
    let meter = global::meter("stepflow");
    meter
        .u64_histogram("blob_store.blob_size")
        .with_description("Blob size distribution")
        .with_unit("By")
        .build()
});

/// Record metrics for a blob put operation.
pub fn record_blob_put(blob_type: &str, size_bytes: u64, duration_seconds: f64) {
    let attrs = [KeyValue::new("blob_type", blob_type.to_string())];
    BLOB_PUT_BYTES.add(size_bytes, &attrs);
    BLOB_PUT_DURATION.record(duration_seconds, &attrs);
    BLOB_SIZE.record(size_bytes, &attrs);
}

/// Record metrics for a blob get operation.
pub fn record_blob_get(blob_type: &str, size_bytes: u64, duration_seconds: f64) {
    let attrs = [KeyValue::new("blob_type", blob_type.to_string())];
    BLOB_GET_BYTES.add(size_bytes, &attrs);
    BLOB_GET_DURATION.record(duration_seconds, &attrs);
}

// =========================================================================
// Step Retry Metrics
// =========================================================================

/// Total step retries, labeled by reason and component.
///
/// Reason values:
/// - `"transport_error"`: subprocess crash, network timeout, connection failure
/// - `"component_error"`: component ran and returned an error (step has `onError: retry`)
/// - `"orchestrator_recovery"`: orchestrator crashed; re-executing in-flight tasks
static STEP_RETRIES: LazyLock<Counter<u64>> = LazyLock::new(|| {
    let meter = global::meter("stepflow");
    meter
        .u64_counter("step.retries_total")
        .with_description("Total step execution retries")
        .build()
});

/// Steps that failed after exhausting their retry budget.
static STEP_RETRIES_EXHAUSTED: LazyLock<Counter<u64>> = LazyLock::new(|| {
    let meter = global::meter("stepflow");
    meter
        .u64_counter("step.retries_exhausted_total")
        .with_description("Steps that failed after exhausting retry budget")
        .build()
});

/// Total step execution attempts (initial + retries), by outcome.
static STEP_EXECUTIONS: LazyLock<Counter<u64>> = LazyLock::new(|| {
    let meter = global::meter("stepflow");
    meter
        .u64_counter("step.executions_total")
        .with_description("Total step execution attempts")
        .build()
});

/// Record a step retry.
pub fn record_step_retry(reason: &str, component: &str) {
    STEP_RETRIES.add(
        1,
        &[
            KeyValue::new("reason", reason.to_string()),
            KeyValue::new("component", component.to_string()),
        ],
    );
}

/// Record retry budget exhaustion (step failed after all retries).
pub fn record_step_retries_exhausted(reason: &str, component: &str) {
    STEP_RETRIES_EXHAUSTED.add(
        1,
        &[
            KeyValue::new("reason", reason.to_string()),
            KeyValue::new("component", component.to_string()),
        ],
    );
}

/// Record a step execution attempt.
pub fn record_step_execution(component: &str, outcome: &str) {
    STEP_EXECUTIONS.add(
        1,
        &[
            KeyValue::new("component", component.to_string()),
            KeyValue::new("outcome", outcome.to_string()),
        ],
    );
}
