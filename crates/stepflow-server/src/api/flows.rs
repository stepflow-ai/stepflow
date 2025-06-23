use axum::{
    extract::{Path, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_analysis::{AnalysisResult, FlowAnalysis, analyze_workflow_dependencies};
use stepflow_core::workflow::{Flow, FlowHash};
use stepflow_execution::StepFlowExecutor;
use utoipa::ToSchema;

use crate::error::{ErrorResponse, ServerError};

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
    /// The hash of the stored flow (only present if no fatal diagnostics)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow_hash: Option<FlowHash>,
    /// Analysis result with diagnostics and optional analysis
    #[serde(flatten)]
    pub analysis_result: AnalysisResult,
}

/// Response containing a flow definition and its hash
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FlowResponse {
    /// The flow definition
    pub flow: Arc<Flow>,
    /// The flow hash
    pub flow_hash: FlowHash,
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
    let flow_hash = Flow::hash(&flow);

    // First validate the workflow
    let analysis_result = analyze_workflow_dependencies(flow.clone(), flow_hash.clone())?;

    // Determine if we can store the flow (no fatal diagnostics)
    let stored_flow_hash = if analysis_result.has_fatal_diagnostics() {
        // Validation failed: don't store the flow
        None
    } else {
        // Store the flow
        let state_store = executor.state_store();
        state_store.store_workflow(flow.clone()).await?;
        Some(flow_hash)
    };

    Ok(Json(StoreFlowResponse {
        flow_hash: stored_flow_hash,
        analysis_result,
    }))
}

/// Get a flow by its hash
#[utoipa::path(
    get,
    path = "/flows/{flow_hash}",
    params(
        ("flow_hash" = String, Path, description = "Flow hash to retrieve")
    ),
    responses(
        (status = 200, description = "Flow retrieved successfully", body = FlowResponse),
        (status = 404, description = "Flow not found")
    ),
    tag = crate::api::FLOW_TAG,
)]
pub async fn get_flow(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(flow_hash): Path<FlowHash>,
) -> Result<Json<FlowResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    let flow = state_store
        .get_workflow(&flow_hash)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::WorkflowNotFound(flow_hash.clone())))?;

    // Generate analysis for the flow.
    // TODO: Cache this to avoid re-analysis.
    let analysis_result = analyze_workflow_dependencies(flow.clone(), flow_hash.clone())?;

    let analysis = match &analysis_result.analysis {
        Some(analysis) => analysis.clone(),
        None => {
            // If validation fails, return a 400 error with diagnostic details
            let (fatal, error, _warning) = analysis_result.diagnostic_counts();
            return Err(ErrorResponse {
                code: axum::http::StatusCode::BAD_REQUEST,
                message: format!(
                    "Workflow validation failed with {} fatal and {} error diagnostics",
                    fatal, error
                ),
            });
        }
    };

    Ok(Json(FlowResponse {
        all_examples: flow.get_all_examples(),
        flow,
        flow_hash,
        analysis,
    }))
}

/// Delete a flow by hash
#[utoipa::path(
    delete,
    path = "/flows/{flow_hash}",
    params(
        ("flow_hash" = String, Path, description = "Flow hash to delete")
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
    Path(_flow_hash): Path<FlowHash>,
) -> Result<(), ErrorResponse> {
    // TODO: Implement proper flow deletion with run checks
    // For now, just return success since the state store doesn't have delete_workflow method
    Ok(())
}
