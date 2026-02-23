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

use aide::transform::TransformOperation;
use axum::{
    extract::{Path, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_analysis::{Diagnostics, validate};
use stepflow_core::{BlobId, BlobType, workflow::Flow};
use stepflow_plugin::StepflowEnvironment;
use stepflow_state::BlobStoreExt as _;

use crate::error::ErrorResponse;

/// Path parameters for flow endpoints
#[derive(Deserialize, schemars::JsonSchema)]
pub struct FlowPath {
    /// The flow's content-based hash ID
    pub flow_id: BlobId,
}

/// Request to store a flow
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StoreFlowRequest {
    /// The flow to store
    pub flow: Arc<Flow>,
    /// If true, only validate the flow without storing it
    #[serde(default)]
    pub dry_run: bool,
}

/// Response when a flow is stored
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StoreFlowResponse {
    /// The ID of the flow (computed from content hash, always present)
    pub flow_id: BlobId,
    /// Whether the flow was actually stored (false for dry_run or if validation has fatal errors)
    pub stored: bool,
    /// Validation diagnostics
    pub diagnostics: Diagnostics,
}

/// Response containing a flow definition and its ID
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FlowResponse {
    /// The flow definition
    pub flow: Arc<Flow>,
    /// The flow ID
    pub flow_id: BlobId,
    /// All available examples (includes both examples and test cases)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub all_examples: Vec<stepflow_core::workflow::ExampleInput>,
}

pub fn store_flow_docs(op: TransformOperation<'_>) -> TransformOperation<'_> {
    op.id("storeFlow")
        .summary("Store a flow")
        .description("Store a flow and return its content-based hash ID.")
        .tag("Flow")
        .response_with::<400, ErrorResponse, _>(|res| res.description("Invalid flow definition"))
}

/// Store a flow and return its hash
pub async fn store_flow(
    State(executor): State<Arc<StepflowEnvironment>>,
    Json(req): Json<StoreFlowRequest>,
) -> Result<Json<StoreFlowResponse>, ErrorResponse> {
    let flow = req.flow;

    // Validate the workflow
    let diagnostics = validate(&flow)?;

    // Always compute the flow_id (content-based hash)
    let flow_id = BlobId::from_flow(&flow).map_err(|_| ErrorResponse {
        code: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        message: "Failed to compute flow ID".to_string(),
        stack: vec![],
    })?;

    // Only store the flow if no fatal diagnostics and not dry_run
    let stored = if !diagnostics.has_fatal() && !req.dry_run {
        // Store the flow as a blob
        let blob_store = executor.blob_store();
        let content = serde_json::to_vec(flow.as_ref()).map_err(|_| ErrorResponse {
            code: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            message: "Failed to serialize flow".to_string(),
            stack: vec![],
        })?;
        blob_store
            .put_blob(&content, BlobType::Flow, Default::default())
            .await
            .map_err(|_| ErrorResponse {
                code: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                message: "Failed to store flow".to_string(),
                stack: vec![],
            })?;
        true
    } else {
        false
    };

    Ok(Json(StoreFlowResponse {
        flow_id,
        stored,
        diagnostics,
    }))
}

pub fn get_flow_docs(op: TransformOperation<'_>) -> TransformOperation<'_> {
    op.id("getFlow")
        .summary("Get a flow by ID")
        .description("Retrieve a flow definition by its content-based hash ID.")
        .tag("Flow")
        .response_with::<404, ErrorResponse, _>(|res| res.description("Flow not found"))
}

/// Get a flow by its ID
pub async fn get_flow(
    State(executor): State<Arc<StepflowEnvironment>>,
    Path(FlowPath { flow_id }): Path<FlowPath>,
) -> Result<Json<FlowResponse>, ErrorResponse> {
    let blob_store = executor.blob_store();

    // Retrieve the flow from the blob store
    let raw = blob_store
        .get_blob(&flow_id)
        .await
        .map_err(|_| ErrorResponse {
            code: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            message: "Failed to retrieve flow".to_string(),
            stack: vec![],
        })?
        .ok_or_else(|| ErrorResponse {
            code: axum::http::StatusCode::NOT_FOUND,
            message: "Flow not found".to_string(),
            stack: vec![],
        })?;

    // Check if it's a flow blob and deserialize
    if raw.blob_type != BlobType::Flow {
        return Err(ErrorResponse {
            code: axum::http::StatusCode::BAD_REQUEST,
            message: "Blob is not a flow".to_string(),
            stack: vec![],
        });
    }

    let flow: Flow = serde_json::from_slice(&raw.content).map_err(|_| ErrorResponse {
        code: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        message: "Failed to deserialize flow".to_string(),
        stack: vec![],
    })?;

    let flow = Arc::new(flow);

    // Don't need to validate -- we validated before saving.

    Ok(Json(FlowResponse {
        all_examples: flow.get_all_examples(),
        flow,
        flow_id,
    }))
}

pub fn delete_flow_docs(op: TransformOperation<'_>) -> TransformOperation<'_> {
    op.id("deleteFlow")
        .summary("Delete a flow by ID")
        .description("Delete a flow by its content-based hash ID.")
        .tag("Flow")
        .response_with::<404, ErrorResponse, _>(|res| res.description("Flow not found"))
}

/// Delete a flow by ID
pub async fn delete_flow(
    State(_executor): State<Arc<StepflowEnvironment>>,
    Path(_path): Path<FlowPath>,
) -> Result<(), ErrorResponse> {
    // TODO: Implement proper flow deletion with run checks
    // For now, just return success since blobs are content-addressed and can't be easily deleted
    Ok(())
}
