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

//! Observability context for distributed tracing and logging.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use stepflow_core::BlobId;

/// Observability context for distributed tracing and logging.
///
/// This context is passed with protocol requests to enable trace correlation
/// and structured logging across the Stepflow runtime and component servers.
///
/// # Field Presence
///
/// - `trace_id` and `span_id`: Present when tracing is enabled, None otherwise
/// - `run_id` and `flow_id`: Present for workflow execution requests, None for init/discovery
/// - `step_id`: Present for step-level execution, None for workflow-level operations
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ObservabilityContext {
    /// OpenTelemetry trace ID (128-bit, hex encoded).
    ///
    /// Present when tracing is enabled, None otherwise.
    /// Used to correlate all operations within a single trace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,

    /// OpenTelemetry span ID (64-bit, hex encoded).
    ///
    /// Used to establish parent-child span relationships.
    /// Component servers should use this as the parent span when creating their spans.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,

    /// The ID of the workflow run.
    ///
    /// Present for workflow execution requests, None for initialization/discovery.
    /// Used for filtering logs and associating operations with specific workflow runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,

    /// The ID of the flow being executed.
    ///
    /// Present for workflow execution requests, None for initialization/discovery.
    /// Used for filtering logs and understanding which workflow is being executed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow_id: Option<BlobId>,

    /// The ID of the step being executed.
    ///
    /// Present for step-level execution, None for workflow-level operations.
    /// Used for filtering logs and associating operations with specific workflow steps.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_id: Option<String>,
}

impl ObservabilityContext {
    /// Create observability context from current span and execution context.
    ///
    /// This extracts trace context from the current fastrace span and combines it
    /// with execution context (run_id, flow_id, step_id) to create a complete
    /// observability context for component execution.
    pub fn from_execution_context(execution_context: &stepflow_plugin::ExecutionContext) -> Self {
        // Extract trace context from current fastrace span
        let (trace_id, span_id) = Self::extract_trace_context();

        Self {
            trace_id,
            span_id,
            run_id: Some(execution_context.run_id().to_string()),
            flow_id: execution_context.flow_id().cloned(),
            step_id: execution_context.step_id().map(|s| s.to_owned()),
        }
    }

    /// Create observability context with only trace context (no flow/run).
    ///
    /// This is used for operations that are traced but not associated with
    /// a specific workflow execution, such as server initialization or
    /// component discovery.
    pub fn from_current_span() -> Self {
        let (trace_id, span_id) = Self::extract_trace_context();

        Self {
            trace_id,
            span_id,
            run_id: None,
            flow_id: None,
            step_id: None,
        }
    }

    /// Extract trace context from the current fastrace span.
    ///
    /// Returns (trace_id, span_id) as hex-encoded strings, or (None, None) if no span is active.
    fn extract_trace_context() -> (Option<String>, Option<String>) {
        // Check if there's a current span context
        if let Some(span_context) = fastrace::prelude::SpanContext::current_local_parent() {
            (
                Some(format!("{:032x}", span_context.trace_id.0)),
                Some(format!("{:016x}", span_context.span_id.0)),
            )
        } else {
            (None, None)
        }
    }
}
