use axum::{
    extract::{Path, State},
    response::Json,
};
use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_core::FlowResult;
use stepflow_core::status::ExecutionStatus;
use stepflow_execution::StepFlowExecutor;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{ErrorResponse, ServerError};

/// Request to execute specific steps in debug mode
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DebugStepRequest {
    /// Step IDs to execute
    pub step_ids: Vec<String>,
}

/// Response from debug step executions
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DebugStepResponse {
    /// Results of executed steps
    pub results: std::collections::HashMap<String, FlowResult>,
}

/// Response for runnable steps in debug mode
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DebugRunnableResponse {
    /// Steps that can be executed
    pub runnable_steps: Vec<String>,
}

/// Execute specific steps in debug mode
#[utoipa::path(
    post,
    path = "/executions/{execution_id}/debug/step",
    params(
        ("execution_id" = Uuid, Path, description = "Execution ID (UUID)")
    ),
    request_body = DebugStepRequest,
    responses(
        (status = 200, description = "Steps executed successfully", body = DebugStepResponse),
        (status = 400, description = "Invalid execution ID or request"),
        (status = 404, description = "Execution not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::DEBUG_TAG,
)]
pub async fn debug_execute_step(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(execution_id): Path<Uuid>,
    Json(req): Json<DebugStepRequest>,
) -> Result<Json<DebugStepResponse>, ErrorResponse> {
    // Get the debug session for this execution
    let mut debug_session = executor
        .debug_session(execution_id)
        .await
        .change_context(ServerError::ExecutionNotFound(execution_id))?;

    // Execute the requested steps
    let step_results = debug_session.execute_steps(&req.step_ids).await?;

    // Convert results to the expected format
    let mut results = std::collections::HashMap::new();
    for step_result in step_results {
        results.insert(step_result.metadata.step_id, step_result.result);
    }

    Ok(Json(DebugStepResponse { results }))
}

/// Continue debug execution to completion
#[utoipa::path(
    post,
    path = "/executions/{execution_id}/debug/continue",
    params(
        ("execution_id" = Uuid, Path, description = "Execution ID (UUID)")
    ),
    responses(
        (status = 200, description = "Execution continued successfully", body = super::executions::CreateExecutionResponse),
        (status = 400, description = "Invalid execution ID"),
        (status = 404, description = "Execution not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::DEBUG_TAG,
)]
pub async fn debug_continue(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(execution_id): Path<Uuid>,
) -> Result<Json<super::executions::CreateExecutionResponse>, ErrorResponse> {
    // Get the debug session for this execution
    let mut debug_session = executor
        .debug_session(execution_id)
        .await
        .change_context(ServerError::ExecutionNotFound(execution_id))?;

    // Continue execution to completion
    let final_result = debug_session.execute_to_completion().await?;

    // Update execution status based on the result
    let state_store = executor.state_store();
    let status = match &final_result {
        FlowResult::Success { .. } => ExecutionStatus::Completed,
        FlowResult::Failed { .. } | FlowResult::Skipped => ExecutionStatus::Failed,
    };

    state_store
        .update_execution_status(execution_id, status, None)
        .await?;

    Ok(Json(super::executions::CreateExecutionResponse {
        execution_id,
        result: Some(final_result),
        status,
        debug: true,
    }))
}

/// Get runnable steps in debug mode
#[utoipa::path(
    get,
    path = "/executions/{execution_id}/debug/runnable",
    params(
        ("execution_id" = Uuid, Path, description = "Execution ID (UUID)")
    ),
    responses(
        (status = 200, description = "Runnable steps retrieved successfully", body = DebugRunnableResponse),
        (status = 400, description = "Invalid execution ID"),
        (status = 404, description = "Execution not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::DEBUG_TAG,
)]
pub async fn debug_get_runnable(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(execution_id): Path<Uuid>,
) -> Result<Json<DebugRunnableResponse>, ErrorResponse> {
    // Get the debug session for this execution
    let debug_session = executor
        .debug_session(execution_id)
        .await
        .change_context(ServerError::ExecutionNotFound(execution_id))?;

    // Get runnable steps
    let runnable_steps = debug_session
        .get_runnable_steps()
        .await
        .into_iter()
        .map(|step| step.step_id)
        .collect();

    Ok(Json(DebugRunnableResponse { runnable_steps }))
}
