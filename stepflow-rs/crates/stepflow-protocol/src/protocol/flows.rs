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
use stepflow_core::workflow::ValueRef;
use stepflow_core::{BlobId, FlowResult};

use crate::protocol::Method;

use super::ProtocolMethod;

/// Sent from the component server to the Stepflow to evaluate a flow with the provided input.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct EvaluateFlowParams {
    /// The ID of the flow to evaluate (blob ID of the flow).
    pub flow_id: BlobId,
    /// The input to provide to the flow.
    pub input: ValueRef,
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
