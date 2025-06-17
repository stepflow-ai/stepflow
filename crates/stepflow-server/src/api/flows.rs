use axum::{
    extract::{Path, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_analysis::StepDependency;
use stepflow_core::workflow::{Flow, FlowHash};
use stepflow_execution::StepFlowExecutor;
use utoipa::ToSchema;

use crate::error::{ErrorResponse, ServerError};

/// Request to store a flow
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StoreFlowRequest {
    /// The flow to store
    pub flow: Arc<Flow>,
}

/// Response when a flow is stored
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StoreFlowResponse {
    /// The hash of the stored flow
    pub flow_hash: FlowHash,
}

/// Response containing a flow definition and its hash
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FlowResponse {
    /// The flow definition
    pub flow: Arc<Flow>,
    /// The flow hash
    pub flow_hash: FlowHash,
    /// All available examples (includes both examples and test cases)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub all_examples: Vec<stepflow_core::workflow::ExampleInput>,
}

/// Response for flow dependencies
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FlowDependenciesResponse {
    /// The flow hash
    pub flow_hash: FlowHash,
    /// The flow dependencies
    pub dependencies: Vec<StepDependency>,
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

    let state_store = executor.state_store();
    state_store.store_workflow(flow.clone()).await?;

    Ok(Json(StoreFlowResponse { flow_hash }))
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

    Ok(Json(FlowResponse {
        all_examples: flow.get_all_examples(),
        flow,
        flow_hash,
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

/// Get flow dependencies analysis by hash
#[utoipa::path(
    get,
    path = "/flows/{flow_hash}/dependencies",
    params(
        ("flow_hash" = String, Path, description = "Flow hash to analyze")
    ),
    responses(
        (status = 200, description = "Flow dependencies retrieved successfully", body = FlowDependenciesResponse),
        (status = 404, description = "Flow not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::FLOW_TAG,
)]
pub async fn get_flow_dependencies(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(flow_hash): Path<FlowHash>,
) -> Result<Json<FlowDependenciesResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    let flow = state_store
        .get_workflow(&flow_hash)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::WorkflowNotFound(flow_hash.clone())))?;

    // TODO: We could cache the analysis based on the flow ID.
    let analysis = stepflow_analysis::analyze_workflow_dependencies(flow, flow_hash.clone())?;

    Ok(Json(FlowDependenciesResponse {
        flow_hash,
        dependencies: analysis.dependencies,
    }))
}
