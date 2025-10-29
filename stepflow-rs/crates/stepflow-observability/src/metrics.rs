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
