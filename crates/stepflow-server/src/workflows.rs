use axum::{
    extract::{Path, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_analysis::StepDependency;
use stepflow_core::workflow::{Flow, FlowHash};
use stepflow_execution::StepFlowExecutor;
use utoipa::{OpenApi, ToSchema};

use crate::error::{ErrorResponse, ServerError};

/// Request to store a workflow
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StoreWorkflowRequest {
    /// The workflow to store
    pub workflow: Arc<Flow>,
}

/// Response when a workflow is stored
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StoreWorkflowResponse {
    /// The hash of the stored workflow
    pub workflow_hash: FlowHash,
}

/// Response containing a workflow definition and its hash
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkflowResponse {
    /// The workflow definition
    pub workflow: Arc<Flow>,
    /// The workflow hash
    pub workflow_hash: FlowHash,
    /// All available examples (includes both examples and test cases)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub all_examples: Vec<stepflow_core::workflow::ExampleInput>,
}

/// Response for listing workflows
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListWorkflowsResponse {
    /// List of workflow hashes
    pub workflow_hashes: Vec<FlowHash>,
}

/// Response for workflow dependencies
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkflowDependenciesResponse {
    /// The workflow hash
    pub workflow_hash: FlowHash,
    /// The workflow dependencies
    pub dependencies: Vec<StepDependency>,
}

/// Workflow management API
#[derive(OpenApi)]
#[openapi(
    paths(
        store_workflow,
        get_workflow,
        list_workflows,
        delete_workflow,
        get_workflow_dependencies
    ),
    components(schemas(
        StoreWorkflowRequest,
        StoreWorkflowResponse,
        WorkflowResponse,
        ListWorkflowsResponse,
        WorkflowDependenciesResponse
    ))
)]
pub struct WorkflowApi;

/// Store a workflow and return its hash
#[utoipa::path(
    post,
    path = "/workflows",
    request_body = StoreWorkflowRequest,
    responses(
        (status = 200, description = "Workflow stored successfully", body = StoreWorkflowResponse),
        (status = 400, description = "Invalid workflow")
    )
)]
pub async fn store_workflow(
    State(executor): State<Arc<StepFlowExecutor>>,
    Json(req): Json<StoreWorkflowRequest>,
) -> Result<Json<StoreWorkflowResponse>, ErrorResponse> {
    let workflow = req.workflow;
    let workflow_hash = Flow::hash(&workflow);

    let state_store = executor.state_store();
    state_store.store_workflow(workflow.clone()).await?;

    Ok(Json(StoreWorkflowResponse { workflow_hash }))
}

/// Get a workflow by its hash
#[utoipa::path(
    get,
    path = "/workflows/{workflow_hash}",
    params(
        ("workflow_hash" = String, Path, description = "Workflow hash to retrieve")
    ),
    responses(
        (status = 200, description = "Workflow retrieved successfully", body = WorkflowResponse),
        (status = 404, description = "Workflow not found")
    )
)]
pub async fn get_workflow(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(workflow_hash): Path<FlowHash>,
) -> Result<Json<WorkflowResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    let workflow = state_store
        .get_workflow(&workflow_hash)
        .await?
        .ok_or_else(|| {
            error_stack::report!(ServerError::WorkflowNotFound(workflow_hash.clone()))
        })?;

    Ok(Json(WorkflowResponse {
        all_examples: workflow.get_all_examples(),
        workflow,
        workflow_hash,
    }))
}

/// List all stored workflows
#[utoipa::path(
    get,
    path = "/workflows",
    responses(
        (status = 200, description = "Workflows listed successfully", body = ListWorkflowsResponse)
    )
)]
pub async fn list_workflows(
    State(_executor): State<Arc<StepFlowExecutor>>,
) -> Result<Json<ListWorkflowsResponse>, ErrorResponse> {
    // For now, return empty list since the state store doesn't have list_workflows method
    Ok(Json(ListWorkflowsResponse {
        workflow_hashes: vec![],
    }))
}

/// Delete a workflow by hash
#[utoipa::path(
    delete,
    path = "/workflows/{workflow_hash}",
    params(
        ("workflow_hash" = String, Path, description = "Workflow hash to delete")
    ),
    responses(
        (status = 200, description = "Workflow deleted successfully"),
        (status = 404, description = "Workflow not found")
    )
)]
pub async fn delete_workflow(
    State(_executor): State<Arc<StepFlowExecutor>>,
    Path(_workflow_hash): Path<String>,
) -> Result<(), ErrorResponse> {
    // For now, just return success since the state store doesn't have delete_workflow method
    Ok(())
}

/// Get workflow dependencies analysis by hash
#[utoipa::path(
    get,
    path = "/workflows/{workflow_hash}/dependencies",
    params(
        ("workflow_hash" = String, Path, description = "Workflow hash to analyze")
    ),
    responses(
        (status = 200, description = "Workflow dependencies retrieved successfully", body = WorkflowDependenciesResponse),
        (status = 404, description = "Workflow not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_workflow_dependencies(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(workflow_hash): Path<FlowHash>,
) -> Result<Json<WorkflowDependenciesResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    let workflow = state_store
        .get_workflow(&workflow_hash)
        .await?
        .ok_or_else(|| {
            error_stack::report!(ServerError::WorkflowNotFound(workflow_hash.clone()))
        })?;

    // TODO: We could cache the analysis based on the workflow ID.
    let analysis =
        stepflow_analysis::analyze_workflow_dependencies(workflow, workflow_hash.clone())?;

    Ok(Json(WorkflowDependenciesResponse {
        workflow_hash,
        dependencies: analysis.dependencies,
    }))
}
