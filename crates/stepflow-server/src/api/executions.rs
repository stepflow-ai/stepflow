use axum::{
    extract::{Path, State},
    response::Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_core::status::{ExecutionStatus, StepStatus};
use stepflow_core::{
    FlowResult,
    workflow::{Flow, FlowHash, ValueRef},
};
use stepflow_execution::StepFlowExecutor;
use stepflow_state::{ExecutionDetails, ExecutionSummary};
use utoipa::ToSchema;
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
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub struct ExecutionSummaryResponse {
    /// The execution ID
    pub execution_id: Uuid,
    /// The workflow name (from workflow.name field)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_name: Option<String>,
    /// The workflow label (if executed via labeled workflow route)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_label: Option<String>,
    /// The workflow hash
    pub workflow_hash: FlowHash,
    /// Current status of the execution
    pub status: ExecutionStatus,
    /// Whether execution is in debug mode
    pub debug_mode: bool,
    /// When the execution was created
    pub created_at: DateTime<Utc>,
    /// When the execution was completed (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
}

/// Detailed execution information for API responses
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExecutionDetailsResponse {
    /// Execution summary information (same as list response)
    #[serde(flatten)]
    pub summary: ExecutionSummaryResponse,
    /// Input data for the execution
    pub input: ValueRef,
    /// Result data (if completed and available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<FlowResult>,
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

/// Create and execute a workflow ad-hoc
#[utoipa::path(
    post,
    path = "/executions",
    request_body = CreateExecutionRequest,
    responses(
        (status = 200, description = "Workflow execution created successfully", body = CreateExecutionResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::EXECUTION_TAG,
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
            workflow_hash.clone(),
            workflow.name.as_deref(), // Use workflow name if available
            None,                     // No workflow label for ad-hoc execution
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
    ),
    tag = crate::api::EXECUTION_TAG,
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
    ),
    tag = crate::api::EXECUTION_TAG,
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

    let workflow_hash = execution.summary.workflow_hash;
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
    ),
    tag = crate::api::EXECUTION_TAG,
)]
pub async fn list_executions(
    State(executor): State<Arc<StepFlowExecutor>>,
) -> Result<Json<ListExecutionsResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    // TODO: Add query parameters for filtering (status, workflow_name, workflow_label, limit, offset)
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
    ),
    tag = crate::api::EXECUTION_TAG,
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

    let workflow_hash = execution.summary.workflow_hash;

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

    // Get step status from state store
    let step_statuses = {
        let step_info_list = state_store
            .get_step_info_for_execution(execution_id)
            .await?;
        let mut status_map = HashMap::new();
        for step_info in step_info_list {
            status_map.insert(step_info.step_index, step_info.status);
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
            execution_id: summary.execution_id,
            workflow_name: summary.workflow_name,
            workflow_label: summary.workflow_label,
            workflow_hash: summary.workflow_hash,
            status: summary.status,
            debug_mode: summary.debug_mode,
            created_at: summary.created_at,
            completed_at: summary.completed_at,
        }
    }
}

impl From<ExecutionDetails> for ExecutionDetailsResponse {
    fn from(details: ExecutionDetails) -> Self {
        Self {
            summary: ExecutionSummaryResponse::from(details.summary),
            input: details.input,
            result: details.result.map(|r| FlowResult::Success { result: r }),
        }
    }
}
