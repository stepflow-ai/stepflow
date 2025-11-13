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

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use stepflow_core::workflow::{ValueRef, WorkflowOverrides};
use stepflow_core::{BlobId, FlowResult};

use crate::protocol::Method;

use super::{ObservabilityContext, ProtocolMethod};

/// Sent from the component server to the Stepflow to evaluate a flow with the provided input.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct EvaluateFlowParams {
    /// The ID of the flow to evaluate (blob ID of the flow).
    pub flow_id: BlobId,
    /// The input to provide to the flow.
    pub input: ValueRef,
    /// Optional workflow overrides to apply before execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overrides: Option<WorkflowOverrides>,
    /// Observability context for tracing nested flow execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observability: Option<ObservabilityContext>,
}

/// Sent from the Stepflow back to the component server with the result of the flow evaluation.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct EvaluateFlowResult {
    /// The result of the flow evaluation.
    pub result: FlowResult,
}

impl ProtocolMethod for EvaluateFlowParams {
    const METHOD_NAME: Method = Method::FlowsEvaluate;
    type Response = EvaluateFlowResult;
}

/// Sent from the component server to Stepflow to get flow and step metadata.
///
/// This request allows components to access workflow-level metadata and step-specific metadata
/// during execution. The metadata can contain arbitrary JSON values defined in the workflow
/// YAML/JSON.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetFlowMetadataParams {
    /// The flow to retrieve metadata for.
    pub flow_id: BlobId,

    /// The ID of the step to get metadata for (optional).
    ///
    /// If not provided, only flow-level metadata is returned.
    /// If provided, both flow metadata and the specified step's metadata are returned.
    /// If the step_id doesn't exist, step_metadata will be None in the response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_id: Option<String>,

    /// Observability context for tracing metadata requests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observability: Option<ObservabilityContext>,
}

/// Sent from Stepflow back to the component server with the requested metadata.
///
/// Contains the flow metadata and step metadata if a specific step was requested.
/// The metadata values are arbitrary JSON objects that can be accessed by components during
/// workflow execution.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetFlowMetadataResult {
    /// Metadata for the current flow.
    ///
    /// This always contains the flow-level metadata defined in the workflow file.
    /// Common fields include name, description, version, but can contain any
    /// arbitrary JSON structure defined by the workflow author.
    pub flow_metadata: HashMap<String, serde_json::Value>,

    /// Metadata for the specified step (only present if step_id was provided and found).
    ///
    /// This contains step-specific metadata defined in the workflow file.
    /// Will be None if no step_id was provided in the request
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_metadata: Option<HashMap<String, serde_json::Value>>,
}

impl ProtocolMethod for GetFlowMetadataParams {
    const METHOD_NAME: Method = Method::FlowsGetMetadata;
    type Response = GetFlowMetadataResult;
}

/// Sent from the component server to Stepflow to submit a batch execution.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SubmitBatchParams {
    /// The ID of the flow to evaluate (blob ID of the flow).
    pub flow_id: BlobId,
    /// The inputs to provide to the flow for each run.
    pub inputs: Vec<ValueRef>,
    /// Optional workflow overrides to apply to all runs before execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overrides: Option<WorkflowOverrides>,
    /// Maximum number of concurrent executions (defaults to number of inputs if not specified).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrency: Option<usize>,
    /// Observability context for tracing batch submission.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observability: Option<ObservabilityContext>,
}

/// Sent from Stepflow back to the component server with the batch submission result.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SubmitBatchResult {
    /// The batch ID (UUID).
    pub batch_id: String,
    /// Total number of runs in the batch.
    pub total_runs: usize,
}

impl ProtocolMethod for SubmitBatchParams {
    const METHOD_NAME: Method = Method::FlowsSubmitBatch;
    type Response = SubmitBatchResult;
}

/// Sent from the component server to Stepflow to get batch status and optionally results.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetBatchParams {
    /// The batch ID to query.
    pub batch_id: String,
    /// If true, wait for batch completion before returning.
    #[serde(default)]
    pub wait: bool,
    /// If true, include full outputs in response.
    #[serde(default)]
    pub include_results: bool,
    /// Observability context for tracing batch queries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observability: Option<ObservabilityContext>,
}

/// Output information for a single run in a batch.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BatchOutputInfo {
    /// Position in the batch input array.
    pub batch_input_index: usize,
    /// The execution status.
    pub status: String,
    /// The flow result (if completed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<FlowResult>,
}

/// Batch details including metadata and statistics.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BatchDetails {
    /// The batch ID.
    pub batch_id: String,
    /// The flow ID.
    pub flow_id: BlobId,
    /// The flow name (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow_name: Option<String>,
    /// Total number of runs in the batch.
    pub total_runs: usize,
    /// Batch status (running | cancelled).
    pub status: String,
    /// Timestamp when the batch was created.
    pub created_at: String,
    /// Statistics for the batch.
    pub completed_runs: usize,
    pub running_runs: usize,
    pub failed_runs: usize,
    pub cancelled_runs: usize,
    pub paused_runs: usize,
    /// Completion timestamp (if all runs complete).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
}

/// Sent from Stepflow back to the component server with batch details and optional outputs.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetBatchResult {
    /// Always included: batch details with metadata and statistics.
    pub details: BatchDetails,
    /// Only included if include_results=true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outputs: Option<Vec<BatchOutputInfo>>,
}

impl ProtocolMethod for GetBatchParams {
    const METHOD_NAME: Method = Method::FlowsGetBatch;
    type Response = GetBatchResult;
}
