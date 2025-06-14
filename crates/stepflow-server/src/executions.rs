use axum::{
    extract::{Path, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_core::status::{ExecutionStatus, StepStatus};
use stepflow_core::{
    FlowResult,
    workflow::{Flow, FlowHash, ValueRef},
};
use stepflow_execution::StepFlowExecutor;
use stepflow_state::{ExecutionDetails, ExecutionSummary};
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

use crate::error::{ErrorResponse, ServerError};

/// Request to create/execute a workflow ad-hoc
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateExecutionRequest {
    /// The workflow to execute
    pub workflow: Arc<Flow>,
    /// Input data for the workflow
    pub input: ValueRef,
    /// Whether to run in debug mode (pauses execution for step-by-step control)
    #[serde(default)]
    pub debug: bool,
}

/// Response for create execution operations
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateExecutionResponse {
    /// The execution ID
    pub execution_id: Uuid,
    /// The result of the workflow execution (if completed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<FlowResult>,
    /// The execution status
    pub status: ExecutionStatus,
    /// Whether this execution is in debug mode
    pub debug: bool,
}

/// Execution summary for API responses
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExecutionSummaryResponse {
    /// The execution ID (UUID string)
    pub execution_id: String,
    /// The endpoint name (if executed via endpoint)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint_name: Option<String>,
    /// The endpoint label (if executed via endpoint)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint_label: Option<String>,
    /// The workflow hash
    pub workflow_hash: FlowHash,
    /// Current status of the execution
    pub status: ExecutionStatus,
    /// Whether execution is in debug mode
    pub debug_mode: bool,
    /// When the execution was created (RFC3339 format)
    pub created_at: String,
    /// When the execution was completed (if applicable, RFC3339 format)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
}

/// Detailed execution information for API responses
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExecutionDetailsResponse {
    /// The execution ID (UUID string)
    pub execution_id: String,
    /// The endpoint name (if executed via endpoint)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint_name: Option<String>,
    /// The endpoint label (if executed via endpoint)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint_label: Option<String>,
    /// The workflow hash
    pub workflow_hash: FlowHash,
    /// Current status of the execution
    pub status: ExecutionStatus,
    /// Whether execution is in debug mode
    pub debug_mode: bool,
    /// Input data (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<ValueRef>,
    /// Result data (if completed and available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<FlowResult>,
    /// When the execution was created (RFC3339 format)
    pub created_at: String,
    /// When the execution was completed (if applicable, RFC3339 format)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
}

/// Response for listing executions
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListExecutionsResponse {
    /// List of execution summaries
    pub executions: Vec<ExecutionSummaryResponse>,
}

/// Response for step execution details
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StepExecutionResponse {
    /// Step index in the workflow
    pub step_index: usize,
    /// Step ID (if provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_id: Option<String>,
    /// Component name/URL that this step executes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    /// Current status of the step
    pub state: StepStatus,
    /// The result of the step execution (if completed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<FlowResult>,
}

/// Response for listing step executions
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListStepExecutionsResponse {
    /// List of step execution results
    pub steps: Vec<StepExecutionResponse>,
}

/// Response containing a workflow definition and its hash
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkflowResponse {
    /// The workflow definition
    pub workflow: Arc<Flow>,
    /// The workflow hash
    pub workflow_hash: FlowHash,
}

/// Executions API
#[derive(OpenApi)]
#[openapi(
    paths(
        create_execution,
        get_execution,
        get_execution_workflow,
        list_executions,
        get_execution_steps
    ),
    components(schemas(
        CreateExecutionRequest,
        CreateExecutionResponse,
        ExecutionSummaryResponse,
        ExecutionDetailsResponse,
        ListExecutionsResponse,
        StepExecutionResponse,
        ListStepExecutionsResponse,
        WorkflowResponse
    ))
)]
pub struct ExecutionsApi;

/// Create and execute a workflow ad-hoc
#[utoipa::path(
    post,
    path = "/executions",
    request_body = CreateExecutionRequest,
    responses(
        (status = 200, description = "Workflow execution created successfully", body = CreateExecutionResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn create_execution(
    State(executor): State<Arc<StepFlowExecutor>>,
    Json(req): Json<CreateExecutionRequest>,
) -> Result<Json<CreateExecutionResponse>, ErrorResponse> {
    let execution_id = Uuid::new_v4();
    let state_store = executor.state_store();

    // Store the workflow in the state store
    let workflow = req.workflow;
    let workflow_hash = Flow::hash(&workflow);

    state_store.store_workflow(workflow.clone()).await?;

    // Create execution record
    state_store
        .create_execution(
            execution_id,
            None, // No endpoint name for ad-hoc execution
            None, // No endpoint label for ad-hoc execution
            workflow_hash.clone(),
            req.debug,
            req.input.clone(),
        )
        .await?;

    let debug_mode = req.debug;
    let input = req.input;

    if debug_mode {
        // In debug mode, return immediately without executing
        // The execution will be controlled via debug endpoints
        state_store
            .update_execution_status(execution_id, ExecutionStatus::Running, None)
            .await?;

        return Ok(Json(CreateExecutionResponse {
            execution_id,
            result: None,
            status: ExecutionStatus::Running,
            debug: debug_mode,
        }));
    }

    // Execute the workflow using the Context trait methods
    use stepflow_plugin::Context as _;

    // Submit the workflow for execution
    let submitted_execution_id = executor.submit_flow(workflow, workflow_hash, input).await?;

    // Wait for the result (synchronous execution for the HTTP endpoint)
    let flow_result = executor.flow_result(submitted_execution_id).await?;

    // Check if the workflow execution was successful
    match &flow_result {
        FlowResult::Success { result } => {
            // Update execution status to completed
            state_store
                .update_execution_status(
                    execution_id,
                    ExecutionStatus::Completed,
                    Some(result.clone()),
                )
                .await?;

            Ok(Json(CreateExecutionResponse {
                execution_id,
                result: Some(flow_result),
                status: ExecutionStatus::Completed,
                debug: debug_mode,
            }))
        }
        FlowResult::Failed { .. } | FlowResult::Skipped => {
            // Update execution status to failed
            state_store
                .update_execution_status(execution_id, ExecutionStatus::Failed, None)
                .await?;

            Ok(Json(CreateExecutionResponse {
                execution_id,
                result: Some(flow_result),
                status: ExecutionStatus::Failed,
                debug: debug_mode,
            }))
        }
    }
}

/// Get execution details by ID
#[utoipa::path(
    get,
    path = "/executions/{execution_id}",
    params(
        ("execution_id" = Uuid, Path, description = "Execution ID (UUID)")
    ),
    responses(
        (status = 200, description = "Execution details retrieved successfully", body = ExecutionDetailsResponse),
        (status = 400, description = "Invalid execution ID format"),
        (status = 404, description = "Execution not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_execution(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(execution_id): Path<Uuid>,
) -> Result<Json<ExecutionDetailsResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    // Get execution details
    let details = state_store
        .get_execution(execution_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::ExecutionNotFound(execution_id)))?;

    let response = ExecutionDetailsResponse::from(details);

    // TODO: Populate input and result from blobs if available

    Ok(Json(response))
}

/// Get the workflow definition for an execution
#[utoipa::path(
    get,
    path = "/executions/{execution_id}/workflow",
    params(
        ("execution_id" = Uuid, Path, description = "Execution ID (UUID)")
    ),
    responses(
        (status = 200, description = "Execution workflow retrieved successfully", body = WorkflowResponse),
        (status = 400, description = "Invalid execution ID format"),
        (status = 404, description = "Execution or workflow not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_execution_workflow(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(execution_id): Path<Uuid>,
) -> Result<Json<WorkflowResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    // Get execution details to retrieve the workflow hash
    let execution = state_store
        .get_execution(execution_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::ExecutionNotFound(execution_id)))?;

    let workflow_hash = execution.workflow_hash;
    // Get the workflow from the state store
    let workflow = state_store
        .get_workflow(&workflow_hash)
        .await?
        .ok_or_else(|| {
            error_stack::report!(ServerError::WorkflowNotFound(workflow_hash.clone()))
        })?;

    Ok(Json(WorkflowResponse {
        workflow,
        workflow_hash,
    }))
}

/// List executions with optional filtering
#[utoipa::path(
    get,
    path = "/executions",
    responses(
        (status = 200, description = "Executions listed successfully", body = ListExecutionsResponse),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn list_executions(
    State(executor): State<Arc<StepFlowExecutor>>,
) -> Result<Json<ListExecutionsResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    // TODO: Add query parameters for filtering (status, endpoint_name, limit, offset)
    let filters = stepflow_state::ExecutionFilters::default();

    let executions = state_store.list_executions(&filters).await?;

    let execution_responses: Vec<ExecutionSummaryResponse> = executions
        .into_iter()
        .map(ExecutionSummaryResponse::from)
        .collect();

    Ok(Json(ListExecutionsResponse {
        executions: execution_responses,
    }))
}

/// Get step-level execution details for a specific execution
#[utoipa::path(
    get,
    path = "/executions/{execution_id}/steps",
    params(
        ("execution_id" = Uuid, Path, description = "Execution ID (UUID)")
    ),
    responses(
        (status = 200, description = "Execution step details retrieved successfully", body = ListStepExecutionsResponse),
        (status = 400, description = "Invalid execution ID format"),
        (status = 404, description = "Execution not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_execution_steps(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(execution_id): Path<Uuid>,
) -> Result<Json<ListStepExecutionsResponse>, ErrorResponse> {
    use std::collections::HashMap;

    let state_store = executor.state_store();

    // Get execution details to retrieve the workflow hash
    let execution = state_store
        .get_execution(execution_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::ExecutionNotFound(execution_id)))?;

    let workflow_hash = execution.workflow_hash;

    // Get the workflow from the state store
    let workflow = state_store
        .get_workflow(&workflow_hash)
        .await?
        .ok_or_else(|| {
            error_stack::report!(ServerError::WorkflowNotFound(workflow_hash.clone()))
        })?;

    // Get step results for completed steps
    let step_results = state_store.list_step_results(execution_id).await?;

    let mut completed_steps: HashMap<usize, stepflow_state::StepResult<'_>> = HashMap::new();
    for step_result in step_results {
        completed_steps.insert(step_result.step_idx(), step_result);
    }

    // Create unified response with both status and results
    let mut step_responses = Vec::new();

    // Get step status through WorkflowExecutor (consistent interface for all step info)
    let step_statuses = {
        // Get input for workflow executor
        let input = execution.input;

        // Create workflow executor to get step status
        let workflow_executor = stepflow_execution::WorkflowExecutor::new(
            executor.clone(),
            workflow.clone(),
            workflow_hash,
            execution_id,
            input,
            state_store.clone(),
        )?;

        // Use unified interface for step status (works for both debug and non-debug)
        let statuses = workflow_executor.list_all_steps().await;
        let mut status_map = HashMap::new();
        for status in statuses {
            status_map.insert(status.step_index, status.status);
        }
        status_map
    };

    // Build unified responses
    for (idx, step) in workflow.steps.iter().enumerate() {
        let state = step_statuses
            .get(&idx)
            .copied()
            .unwrap_or(StepStatus::Blocked);
        let result = completed_steps.get(&idx).map(|sr| sr.result().clone());

        step_responses.push(StepExecutionResponse {
            step_index: idx,
            step_id: if step.id.is_empty() {
                None
            } else {
                Some(step.id.clone())
            },
            component: Some(step.component.to_string()),
            state,
            result,
        });
    }

    Ok(Json(ListStepExecutionsResponse {
        steps: step_responses,
    }))
}

// Conversion implementations
impl From<ExecutionSummary> for ExecutionSummaryResponse {
    fn from(summary: ExecutionSummary) -> Self {
        Self {
            execution_id: summary.execution_id.to_string(),
            endpoint_name: summary.endpoint_name,
            endpoint_label: summary.endpoint_label,
            workflow_hash: summary.workflow_hash,
            status: summary.status,
            debug_mode: summary.debug_mode,
            created_at: summary.created_at.to_rfc3339(),
            completed_at: summary.completed_at.map(|dt| dt.to_rfc3339()),
        }
    }
}

impl From<ExecutionDetails> for ExecutionDetailsResponse {
    fn from(details: ExecutionDetails) -> Self {
        Self {
            execution_id: details.execution_id.to_string(),
            endpoint_name: details.endpoint_name,
            endpoint_label: details.endpoint_label,
            workflow_hash: details.workflow_hash,
            status: details.status,
            debug_mode: details.debug_mode,
            input: None,  // Will be populated separately if needed
            result: None, // Will be populated separately if needed
            created_at: details.created_at.to_rfc3339(),
            completed_at: details.completed_at.map(|dt| dt.to_rfc3339()),
        }
    }
}
