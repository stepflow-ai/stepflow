// Licensed to the Apache Software Foundation (ASF) under one or more contributor license agreements.
// See the NOTICE file distributed with this work for additional information regarding copyright
// ownership.  The ASF licenses this file to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance with the License.  You may obtain a
// copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied.  See the License for the specific language governing permissions and limitations under
// the License.

use axum::{
    extract::{Path, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_analysis::{AnalysisResult, FlowAnalysis, analyze_flow_dependencies};
use stepflow_core::{
    BlobId, BlobType,
    workflow::{Flow, ValueRef},
};
use stepflow_execution::StepFlowExecutor;
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
    /// Analysis result with diagnostics and optional analysis
    #[serde(flatten)]
    pub analysis_result: AnalysisResult,
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
    /// Optional analysis of the flow
    pub analysis: FlowAnalysis,
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
    State(executor): State<Arc<StepFlowExecutor>>,
    Json(req): Json<StoreFlowRequest>,
) -> Result<Json<StoreFlowResponse>, ErrorResponse> {
    let flow = req.flow;

    // First validate the workflow - we need a temporary ID for analysis
    let temp_flow_id = BlobId::from_flow(flow.as_ref()).unwrap();
    let analysis_result = analyze_flow_dependencies(flow.clone(), temp_flow_id.clone())?;

    // Determine if we can store the flow (no fatal diagnostics)
    let stored_flow_id = if analysis_result.has_fatal_diagnostics() {
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
            })?;
        Some(flow_id)
    };

    Ok(Json(StoreFlowResponse {
        flow_id: stored_flow_id,
        analysis_result,
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
    State(executor): State<Arc<StepFlowExecutor>>,
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
        })?;

    // Check if it's a flow blob and deserialize
    if blob_data.blob_type() != BlobType::Flow {
        return Err(ErrorResponse {
            code: axum::http::StatusCode::BAD_REQUEST,
            message: "Blob is not a flow".to_string(),
        });
    }

    let flow: Flow =
        serde_json::from_value(blob_data.data().as_ref().clone()).map_err(|_| ErrorResponse {
            code: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            message: "Failed to deserialize flow".to_string(),
        })?;

    let flow = Arc::new(flow);

    // Generate analysis for the flow.
    // TODO: Cache this to avoid re-analysis.
    let analysis_result = analyze_flow_dependencies(flow.clone(), flow_id.clone())?;

    let analysis = match &analysis_result.analysis {
        Some(analysis) => analysis.clone(),
        None => {
            // If validation fails, return a 400 error with diagnostic details
            let (fatal, error, _warning) = analysis_result.diagnostic_counts();
            return Err(ErrorResponse {
                code: axum::http::StatusCode::BAD_REQUEST,
                message: format!(
                    "Workflow validation failed with {fatal} fatal and {error} error diagnostics"
                ),
            });
        }
    };

    Ok(Json(FlowResponse {
        all_examples: flow.get_all_examples(),
        flow,
        flow_id,
        analysis,
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
    State(_executor): State<Arc<StepFlowExecutor>>,
    Path(_flow_id): Path<BlobId>,
) -> Result<(), ErrorResponse> {
    // TODO: Implement proper flow deletion with run checks
    // For now, just return success since blobs are content-addressed and can't be easily deleted
    Ok(())
}
