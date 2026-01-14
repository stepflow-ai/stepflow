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

use axum::{
    extract::{Path, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_analysis::{Diagnostics, validate};
use stepflow_core::{
    BlobId, BlobType,
    workflow::{Flow, ValueRef},
};
use stepflow_plugin::StepflowEnvironment;
use stepflow_state::StateStoreExt as _;
use utoipa::ToSchema;

use crate::error::ErrorResponse;

/// Request to store a flow
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StoreFlowRequest {
    /// The flow to store
    pub flow: Arc<Flow>,
}

/// Response when a flow is stored
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StoreFlowResponse {
    /// The ID of the stored flow (only present if no fatal diagnostics)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow_id: Option<BlobId>,
    /// Validation diagnostics
    pub diagnostics: Diagnostics,
}

/// Response containing a flow definition and its ID
#[derive(Debug, Clone, Serialize, ToSchema)]
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

/// Store a flow and return its hash
#[utoipa::path(
    post,
    path = "/flows",
    request_body = StoreFlowRequest,
    responses(
        (status = 200, description = "Flow stored successfully", body = StoreFlowResponse),
        (status = 400, description = "Invalid flow")
    ),
    tag = crate::api::FLOW_TAG,
)]
pub async fn store_flow(
    State(executor): State<Arc<StepflowEnvironment>>,
    Json(req): Json<StoreFlowRequest>,
) -> Result<Json<StoreFlowResponse>, ErrorResponse> {
    let flow = req.flow;

    // Validate the workflow
    let diagnostics = validate(&flow)?;

    // Determine if we can store the flow (no fatal diagnostics)
    let flow_id = if diagnostics.has_fatal() {
        // Validation failed: don't store the flow
        None
    } else {
        // Store the flow as a blob
        let state_store = executor.state_store();
        let flow_data = ValueRef::new(serde_json::to_value(flow.as_ref()).unwrap());
        let flow_id = state_store
            .put_blob(flow_data, BlobType::Flow)
            .await
            .map_err(|_| ErrorResponse {
                code: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                message: "Failed to store flow".to_string(),
                stack: vec![],
            })?;
        Some(flow_id)
    };

    Ok(Json(StoreFlowResponse {
        flow_id,
        diagnostics,
    }))
}

/// Get a flow by its ID
#[utoipa::path(
    get,
    path = "/flows/{flow_id}",
    params(
        ("flow_id" = String, Path, description = "Flow ID to retrieve")
    ),
    responses(
        (status = 200, description = "Flow retrieved successfully", body = FlowResponse),
        (status = 404, description = "Flow not found")
    ),
    tag = crate::api::FLOW_TAG,
)]
pub async fn get_flow(
    State(executor): State<Arc<StepflowEnvironment>>,
    Path(flow_id): Path<BlobId>,
) -> Result<Json<FlowResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    // Retrieve the flow from the blob store
    let blob_data = state_store
        .get_blob(&flow_id)
        .await
        .map_err(|_| ErrorResponse {
            code: axum::http::StatusCode::NOT_FOUND,
            message: "Flow not found".to_string(),
            stack: vec![],
        })?;

    // Check if it's a flow blob and deserialize
    if blob_data.blob_type() != BlobType::Flow {
        return Err(ErrorResponse {
            code: axum::http::StatusCode::BAD_REQUEST,
            message: "Blob is not a flow".to_string(),
            stack: vec![],
        });
    }

    let flow: Flow =
        serde_json::from_value(blob_data.data().as_ref().clone()).map_err(|_| ErrorResponse {
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

/// Delete a flow by ID
#[utoipa::path(
    delete,
    path = "/flows/{flow_id}",
    params(
        ("flow_id" = String, Path, description = "Flow ID to delete")
    ),
    responses(
        (status = 204, description = "Flow deleted successfully"),
        (status = 404, description = "Flow not found"),
        (status = 409, description = "Flow has active runs")
    ),
    tag = crate::api::FLOW_TAG,
)]
pub async fn delete_flow(
    State(_executor): State<Arc<StepflowEnvironment>>,
    Path(_flow_id): Path<BlobId>,
) -> Result<(), ErrorResponse> {
    // TODO: Implement proper flow deletion with run checks
    // For now, just return success since blobs are content-addressed and can't be easily deleted
    Ok(())
}
