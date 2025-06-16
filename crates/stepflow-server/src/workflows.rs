use axum::{
    extract::{Path, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_analysis::StepDependency;
use stepflow_core::workflow::{Flow, FlowHash, ValueRef};
use stepflow_execution::StepFlowExecutor;
use stepflow_plugin::Context as _;
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

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

/// Response for listing workflow names
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListWorkflowNamesResponse {
    /// List of workflow names
    pub names: Vec<String>,
}

/// Query parameters for workflow name operations
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkflowNameQuery {
    /// Optional label to filter by
    pub label: Option<String>,
}

/// Response for workflows by name
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkflowsByNameResponse {
    /// The workflow name
    pub name: String,
    /// List of workflows with this name (newest first)
    pub workflows: Vec<WorkflowVersionInfo>,
}

/// Information about a specific version of a named workflow
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkflowVersionInfo {
    /// The workflow hash
    pub workflow_hash: FlowHash,
    /// When this version was created
    pub created_at: String,
}

/// Request to create or update a workflow label
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateLabelRequest {
    /// The workflow hash to point the label to
    pub workflow_hash: FlowHash,
}

/// Response for workflow label operations
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkflowLabelResponse {
    /// The workflow name
    pub name: String,
    /// The label name
    pub label: String,
    /// The workflow hash this label points to
    pub workflow_hash: FlowHash,
    /// When the label was created
    pub created_at: String,
    /// When the label was last updated
    pub updated_at: String,
}

/// Response for listing labels
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListLabelsResponse {
    /// List of labels for the workflow name
    pub labels: Vec<WorkflowLabelResponse>,
}

/// Response for workflow execution
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExecuteWorkflowResponse {
    /// The execution ID
    pub execution_id: Uuid,
    /// The execution status
    pub status: String,
    /// Optional result if execution completed synchronously
    pub result: Option<ValueRef>,
}

/// Request for workflow execution
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExecuteWorkflowRequest {
    /// Input data for the workflow
    pub input: ValueRef,
    /// Whether to run in debug mode
    #[serde(default)]
    pub debug: bool,
}

/// Workflow management API
#[derive(OpenApi)]
#[openapi(
    paths(
        store_workflow,
        get_workflow,
        execute_workflow_by_hash,
        list_workflows,
        delete_workflow,
        get_workflow_dependencies,
        list_workflow_names,
        get_workflows_by_name,
        get_latest_workflow_by_name,
        execute_workflow_by_name,
        list_labels_for_name,
        create_or_update_label,
        get_workflow_by_label,
        execute_workflow_by_label,
        delete_label
    ),
    components(schemas(
        StoreWorkflowRequest,
        StoreWorkflowResponse,
        WorkflowResponse,
        ListWorkflowsResponse,
        WorkflowDependenciesResponse,
        ListWorkflowNamesResponse,
        WorkflowNameQuery,
        WorkflowsByNameResponse,
        WorkflowVersionInfo,
        CreateLabelRequest,
        WorkflowLabelResponse,
        ListLabelsResponse,
        ExecuteWorkflowRequest,
        ExecuteWorkflowResponse
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

/// Execute a workflow by its hash
#[utoipa::path(
    post,
    path = "/workflows/{workflow_hash}/execute",
    params(
        ("workflow_hash" = String, Path, description = "Workflow hash to execute")
    ),
    request_body = ExecuteWorkflowRequest,
    responses(
        (status = 200, description = "Workflow execution started", body = ExecuteWorkflowResponse),
        (status = 404, description = "Workflow not found"),
        (status = 400, description = "Invalid input")
    )
)]
pub async fn execute_workflow_by_hash(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(workflow_hash): Path<FlowHash>,
    Json(req): Json<ExecuteWorkflowRequest>,
) -> Result<Json<ExecuteWorkflowResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    // Get the workflow
    let workflow = state_store
        .get_workflow(&workflow_hash)
        .await?
        .ok_or_else(|| {
            error_stack::report!(ServerError::WorkflowNotFound(workflow_hash.clone()))
        })?;

    // Create execution
    let execution_id = Uuid::new_v4();
    state_store
        .create_execution(
            execution_id,
            workflow_hash.clone(),
            workflow.name.as_deref(), // Use workflow name if available
            None,                     // No label for direct hash execution
            req.debug,
            req.input.clone(),
        )
        .await?;

    // Submit the workflow for execution
    let internal_execution_id = executor
        .submit_flow(workflow, workflow_hash, req.input)
        .await?;

    // Get the execution result
    let flow_result = executor.flow_result(internal_execution_id).await?;

    // Convert FlowResult to response
    let (status, result) = match flow_result {
        stepflow_core::FlowResult::Success { result } => ("completed", Some(result)),
        stepflow_core::FlowResult::Skipped => ("skipped", None),
        stepflow_core::FlowResult::Failed { error } => {
            return Err(error_stack::report!(ServerError::WorkflowExecutionFailed(
                error.to_string()
            ))
            .into());
        }
    };

    Ok(Json(ExecuteWorkflowResponse {
        execution_id,
        status: status.to_string(),
        result,
    }))
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

/// List all workflow names
#[utoipa::path(
    get,
    path = "/workflows/names",
    responses(
        (status = 200, description = "Workflow names listed successfully", body = ListWorkflowNamesResponse)
    )
)]
pub async fn list_workflow_names(
    State(executor): State<Arc<StepFlowExecutor>>,
) -> Result<Json<ListWorkflowNamesResponse>, ErrorResponse> {
    let state_store = executor.state_store();
    let names = state_store.list_workflow_names().await?;

    Ok(Json(ListWorkflowNamesResponse { names }))
}

/// Get all workflows with a specific name
#[utoipa::path(
    get,
    path = "/workflows/by-name/{name}",
    params(
        ("name" = String, Path, description = "Workflow name to search for")
    ),
    responses(
        (status = 200, description = "Workflows retrieved successfully", body = WorkflowsByNameResponse),
        (status = 404, description = "No workflows found with this name")
    )
)]
pub async fn get_workflows_by_name(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(name): Path<String>,
) -> Result<Json<WorkflowsByNameResponse>, ErrorResponse> {
    let state_store = executor.state_store();
    let workflows = state_store.get_workflows_by_name(&name).await?;

    if workflows.is_empty() {
        return Err(error_stack::report!(ServerError::WorkflowNameNotFound(name)).into());
    }

    let workflow_versions = workflows
        .into_iter()
        .map(|(workflow_hash, created_at)| WorkflowVersionInfo {
            workflow_hash,
            created_at: created_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(WorkflowsByNameResponse {
        name,
        workflows: workflow_versions,
    }))
}

/// Get the latest workflow with a specific name
#[utoipa::path(
    get,
    path = "/workflows/by-name/{name}/latest",
    params(
        ("name" = String, Path, description = "Workflow name to get latest version of")
    ),
    responses(
        (status = 200, description = "Latest workflow retrieved successfully", body = WorkflowResponse),
        (status = 404, description = "No workflows found with this name")
    )
)]
pub async fn get_latest_workflow_by_name(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(name): Path<String>,
) -> Result<Json<WorkflowResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    let workflow_with_metadata = state_store
        .get_named_workflow(&name, None)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::WorkflowNameNotFound(name.clone())))?;

    let workflow = workflow_with_metadata.workflow;
    let workflow_hash = workflow_with_metadata.workflow_hash;

    Ok(Json(WorkflowResponse {
        all_examples: workflow.get_all_examples(),
        workflow,
        workflow_hash,
    }))
}

/// Execute the latest version of a named workflow
#[utoipa::path(
    post,
    path = "/workflows/by-name/{name}/execute",
    params(
        ("name" = String, Path, description = "Workflow name to execute latest version of")
    ),
    request_body = ExecuteWorkflowRequest,
    responses(
        (status = 200, description = "Workflow execution started", body = ExecuteWorkflowResponse),
        (status = 404, description = "No workflows found with this name"),
        (status = 400, description = "Invalid input")
    )
)]
pub async fn execute_workflow_by_name(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(name): Path<String>,
    Json(req): Json<ExecuteWorkflowRequest>,
) -> Result<Json<ExecuteWorkflowResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    let workflow_with_metadata = state_store
        .get_named_workflow(&name, None)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::WorkflowNameNotFound(name.clone())))?;

    let workflow_hash = workflow_with_metadata.workflow_hash;
    let workflow = workflow_with_metadata.workflow;

    // Create execution
    let execution_id = Uuid::new_v4();
    state_store
        .create_execution(
            execution_id,
            workflow_hash.clone(),
            Some(&name),
            None, // No specific label used
            req.debug,
            req.input.clone(),
        )
        .await?;

    // Submit the workflow for execution
    let internal_execution_id = executor
        .submit_flow(workflow, workflow_hash, req.input)
        .await?;

    // Get the execution result
    let flow_result = executor.flow_result(internal_execution_id).await?;

    // Convert FlowResult to response
    let (status, result) = match flow_result {
        stepflow_core::FlowResult::Success { result } => ("completed", Some(result)),
        stepflow_core::FlowResult::Skipped => ("skipped", None),
        stepflow_core::FlowResult::Failed { error } => {
            return Err(error_stack::report!(ServerError::WorkflowExecutionFailed(
                error.to_string()
            ))
            .into());
        }
    };

    Ok(Json(ExecuteWorkflowResponse {
        execution_id,
        status: status.to_string(),
        result,
    }))
}

/// List all labels for a workflow name
#[utoipa::path(
    get,
    path = "/workflows/by-name/{name}/labels",
    params(
        ("name" = String, Path, description = "Workflow name to list labels for")
    ),
    responses(
        (status = 200, description = "Labels listed successfully", body = ListLabelsResponse)
    )
)]
pub async fn list_labels_for_name(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(name): Path<String>,
) -> Result<Json<ListLabelsResponse>, ErrorResponse> {
    let state_store = executor.state_store();
    let workflow_labels = state_store.list_labels_for_name(&name).await?;

    let labels = workflow_labels
        .into_iter()
        .map(|wl| WorkflowLabelResponse {
            name: wl.name,
            label: wl.label,
            workflow_hash: wl.workflow_hash,
            created_at: wl.created_at.to_rfc3339(),
            updated_at: wl.updated_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(ListLabelsResponse { labels }))
}

/// Create or update a workflow label
#[utoipa::path(
    put,
    path = "/workflows/by-name/{name}/labels/{label}",
    params(
        ("name" = String, Path, description = "Workflow name"),
        ("label" = String, Path, description = "Label name")
    ),
    request_body = CreateLabelRequest,
    responses(
        (status = 200, description = "Label created/updated successfully", body = WorkflowLabelResponse),
        (status = 404, description = "Workflow not found")
    )
)]
pub async fn create_or_update_label(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path((name, label)): Path<(String, String)>,
    Json(req): Json<CreateLabelRequest>,
) -> Result<Json<WorkflowLabelResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    // Verify the workflow exists
    state_store
        .get_workflow(&req.workflow_hash)
        .await?
        .ok_or_else(|| {
            error_stack::report!(ServerError::WorkflowNotFound(req.workflow_hash.clone()))
        })?;

    // Create or update the label
    state_store
        .create_or_update_label(&name, &label, req.workflow_hash.clone())
        .await?;

    // Get the updated label to return
    let workflow_with_metadata = state_store
        .get_named_workflow(&name, Some(&label))
        .await?
        .ok_or_else(|| ServerError::internal("created label not retrieved"))?;

    let label_info = workflow_with_metadata
        .label_info
        .ok_or_else(|| ServerError::internal("created label not retrieved"))?;

    Ok(Json(WorkflowLabelResponse {
        name: label_info.name,
        label: label_info.label,
        workflow_hash: label_info.workflow_hash,
        created_at: label_info.created_at.to_rfc3339(),
        updated_at: label_info.updated_at.to_rfc3339(),
    }))
}

/// Get a workflow by name and label
#[utoipa::path(
    get,
    path = "/workflows/by-name/{name}/labels/{label}",
    params(
        ("name" = String, Path, description = "Workflow name"),
        ("label" = String, Path, description = "Label name")
    ),
    responses(
        (status = 200, description = "Workflow retrieved successfully", body = WorkflowResponse),
        (status = 404, description = "Workflow or label not found")
    )
)]
pub async fn get_workflow_by_label(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path((name, label)): Path<(String, String)>,
) -> Result<Json<WorkflowResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    let workflow_with_metadata = state_store
        .get_named_workflow(&name, Some(&label))
        .await?
        .ok_or_else(|| {
            error_stack::report!(ServerError::WorkflowLabelNotFound {
                name: name.clone(),
                label: label.clone()
            })
        })?;

    let workflow = workflow_with_metadata.workflow;
    let workflow_hash = workflow_with_metadata.workflow_hash;

    Ok(Json(WorkflowResponse {
        all_examples: workflow.get_all_examples(),
        workflow,
        workflow_hash,
    }))
}

/// Execute a workflow by name and label
#[utoipa::path(
    post,
    path = "/workflows/by-name/{name}/labels/{label}/execute",
    params(
        ("name" = String, Path, description = "Workflow name"),
        ("label" = String, Path, description = "Label name")
    ),
    request_body = ExecuteWorkflowRequest,
    responses(
        (status = 200, description = "Workflow execution started", body = ExecuteWorkflowResponse),
        (status = 404, description = "Workflow or label not found"),
        (status = 400, description = "Invalid input")
    )
)]
pub async fn execute_workflow_by_label(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path((name, label)): Path<(String, String)>,
    Json(req): Json<ExecuteWorkflowRequest>,
) -> Result<Json<ExecuteWorkflowResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    let workflow_with_metadata = state_store
        .get_named_workflow(&name, Some(&label))
        .await?
        .ok_or_else(|| {
            error_stack::report!(ServerError::WorkflowLabelNotFound {
                name: name.clone(),
                label: label.clone()
            })
        })?;

    let workflow_hash = workflow_with_metadata.workflow_hash;
    let workflow = workflow_with_metadata.workflow;

    // Create execution
    let execution_id = Uuid::new_v4();
    state_store
        .create_execution(
            execution_id,
            workflow_hash.clone(),
            Some(&name),
            Some(&label),
            req.debug,
            req.input.clone(),
        )
        .await?;

    // Submit the workflow for execution
    let internal_execution_id = executor
        .submit_flow(workflow, workflow_hash, req.input)
        .await?;

    // Get the execution result
    let flow_result = executor.flow_result(internal_execution_id).await?;

    // Convert FlowResult to response
    let (status, result) = match flow_result {
        stepflow_core::FlowResult::Success { result } => ("completed", Some(result)),
        stepflow_core::FlowResult::Skipped => ("skipped", None),
        stepflow_core::FlowResult::Failed { error } => {
            return Err(error_stack::report!(ServerError::WorkflowExecutionFailed(
                error.to_string()
            ))
            .into());
        }
    };

    Ok(Json(ExecuteWorkflowResponse {
        execution_id,
        status: status.to_string(),
        result,
    }))
}

/// Delete a workflow label
#[utoipa::path(
    delete,
    path = "/workflows/by-name/{name}/labels/{label}",
    params(
        ("name" = String, Path, description = "Workflow name"),
        ("label" = String, Path, description = "Label name")
    ),
    responses(
        (status = 200, description = "Label deleted successfully"),
        (status = 404, description = "Label not found")
    )
)]
pub async fn delete_label(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path((name, label)): Path<(String, String)>,
) -> Result<(), ErrorResponse> {
    let state_store = executor.state_store();

    // Check if the label exists before deleting
    let _workflow_with_metadata = state_store
        .get_named_workflow(&name, Some(&label))
        .await?
        .ok_or_else(|| {
            error_stack::report!(ServerError::WorkflowLabelNotFound {
                name: name.clone(),
                label: label.clone()
            })
        })?;

    state_store.delete_label(&name, &label).await?;

    Ok(())
}
