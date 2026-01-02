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
use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use serde_with::{OneOrMany, serde_as};
use std::sync::Arc;
use stepflow_core::FlowResult;
use stepflow_core::status::StepStatus;
use stepflow_execution::StepflowExecutor;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{ErrorResponse, ServerError};

/// Request to eval a step
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DebugEvalRequest {
    /// Step ID to evaluate
    pub step_id: String,
}

/// Request to queue steps - accepts either a single step ID or a list
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DebugQueueRequest {
    /// Step ID(s) to queue - can be a single string or an array
    #[serde_as(as = "OneOrMany<_>")]
    pub step_ids: Vec<String>,
}

/// Response from eval step
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DebugEvalResponse {
    /// Result of the evaluated step
    pub result: FlowResult,
}

/// Response from queue step
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DebugQueueResponse {
    /// Newly queued step IDs (including dependencies)
    pub queued: Vec<String>,
    /// Steps that are ready to run
    pub ready: Vec<String>,
}

/// Response from next step
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DebugNextResponse {
    /// Step that was executed (if any)
    pub step_id: Option<String>,
    /// Result of the executed step (if any)
    pub result: Option<FlowResult>,
    /// Steps that are ready to run after this execution
    pub ready: Vec<String>,
}

/// Response from run-queue
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DebugRunQueueResponse {
    /// Number of steps executed
    pub executed_count: usize,
    /// Results keyed by step ID
    pub results: std::collections::HashMap<String, FlowResult>,
}

/// Response for queued steps
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DebugQueueStatusResponse {
    /// Steps in the queue (not yet completed)
    pub queued: Vec<QueuedStep>,
    /// Steps that are ready to run
    pub ready: Vec<String>,
}

/// A queued step with its status
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueuedStep {
    /// Step ID
    pub step_id: String,
    /// Step status (Runnable or Blocked)
    pub status: String,
}

/// Response from show step
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DebugShowResponse {
    /// Step ID
    pub step_id: String,
    /// Whether the step has been run
    pub completed: bool,
    /// Result if the step has been run
    pub result: Option<FlowResult>,
}

/// Evaluate a step: queue it, run all dependencies, return result
#[utoipa::path(
    post,
    path = "/runs/{run_id}/debug/eval",
    params(
        ("run_id" = Uuid, Path, description = "Run ID (UUID)")
    ),
    request_body = DebugEvalRequest,
    responses(
        (status = 200, description = "Step evaluated successfully", body = DebugEvalResponse),
        (status = 400, description = "Invalid run ID or request"),
        (status = 404, description = "Run not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::DEBUG_TAG,
)]
pub async fn debug_eval(
    State(executor): State<Arc<StepflowExecutor>>,
    Path(run_id): Path<Uuid>,
    Json(req): Json<DebugEvalRequest>,
) -> Result<Json<DebugEvalResponse>, ErrorResponse> {
    let mut debug_session = executor
        .debug_session(run_id)
        .await
        .change_context(ServerError::ExecutionNotFound(run_id))?;

    let result = debug_session.eval_step(&req.step_id).await?;

    Ok(Json(DebugEvalResponse { result }))
}

/// Queue one or more steps and their dependencies for execution
#[utoipa::path(
    post,
    path = "/runs/{run_id}/debug/queue",
    params(
        ("run_id" = Uuid, Path, description = "Run ID (UUID)")
    ),
    request_body = DebugQueueRequest,
    responses(
        (status = 200, description = "Steps queued successfully", body = DebugQueueResponse),
        (status = 400, description = "Invalid run ID or request"),
        (status = 404, description = "Run not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::DEBUG_TAG,
)]
pub async fn debug_queue(
    State(executor): State<Arc<StepflowExecutor>>,
    Path(run_id): Path<Uuid>,
    Json(req): Json<DebugQueueRequest>,
) -> Result<Json<DebugQueueResponse>, ErrorResponse> {
    let mut debug_session = executor
        .debug_session(run_id)
        .await
        .change_context(ServerError::ExecutionNotFound(run_id))?;

    let mut all_queued = Vec::new();
    for step_id in &req.step_ids {
        // queue_step is async and auto-persists to the debug queue
        let queued = debug_session.queue_step(step_id).await?;
        all_queued.extend(queued);
    }

    let ready_steps = debug_session.get_queued_steps();
    let ready: Vec<String> = ready_steps
        .iter()
        .filter(|s| s.status == StepStatus::Runnable)
        .map(|s| s.step_id.clone())
        .collect();

    Ok(Json(DebugQueueResponse {
        queued: all_queued,
        ready,
    }))
}

/// Run the next ready step from the queue
#[utoipa::path(
    post,
    path = "/runs/{run_id}/debug/next",
    params(
        ("run_id" = Uuid, Path, description = "Run ID (UUID)")
    ),
    responses(
        (status = 200, description = "Next step executed", body = DebugNextResponse),
        (status = 400, description = "Invalid run ID"),
        (status = 404, description = "Run not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::DEBUG_TAG,
)]
pub async fn debug_next(
    State(executor): State<Arc<StepflowExecutor>>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<DebugNextResponse>, ErrorResponse> {
    let mut debug_session = executor
        .debug_session(run_id)
        .await
        .change_context(ServerError::ExecutionNotFound(run_id))?;

    // run_next_step is async and auto-removes from the debug queue
    let step_result = debug_session.run_next_step().await?;

    let (step_id, result) = match &step_result {
        Some(r) => (Some(r.metadata.step_id.clone()), Some(r.result.clone())),
        None => (None, None),
    };

    let ready_steps = debug_session.get_queued_steps();
    let ready: Vec<String> = ready_steps
        .iter()
        .filter(|s| s.status == StepStatus::Runnable)
        .map(|s| s.step_id.clone())
        .collect();

    Ok(Json(DebugNextResponse {
        step_id,
        result,
        ready,
    }))
}

/// Run all steps in the queue until empty
#[utoipa::path(
    post,
    path = "/runs/{run_id}/debug/run-queue",
    params(
        ("run_id" = Uuid, Path, description = "Run ID (UUID)")
    ),
    responses(
        (status = 200, description = "Queue executed", body = DebugRunQueueResponse),
        (status = 400, description = "Invalid run ID"),
        (status = 404, description = "Run not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::DEBUG_TAG,
)]
pub async fn debug_run_queue(
    State(executor): State<Arc<StepflowExecutor>>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<DebugRunQueueResponse>, ErrorResponse> {
    let mut debug_session = executor
        .debug_session(run_id)
        .await
        .change_context(ServerError::ExecutionNotFound(run_id))?;

    // run_queue is async and auto-removes executed steps from the debug queue
    let step_results = debug_session.run_queue().await?;

    let mut results = std::collections::HashMap::new();
    for step_result in &step_results {
        results.insert(
            step_result.metadata.step_id.clone(),
            step_result.result.clone(),
        );
    }

    Ok(Json(DebugRunQueueResponse {
        executed_count: step_results.len(),
        results,
    }))
}

/// Get queued steps and their status
#[utoipa::path(
    get,
    path = "/runs/{run_id}/debug/queue",
    params(
        ("run_id" = Uuid, Path, description = "Run ID (UUID)")
    ),
    responses(
        (status = 200, description = "Queue status retrieved", body = DebugQueueStatusResponse),
        (status = 400, description = "Invalid run ID"),
        (status = 404, description = "Run not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::DEBUG_TAG,
)]
pub async fn debug_get_queue(
    State(executor): State<Arc<StepflowExecutor>>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<DebugQueueStatusResponse>, ErrorResponse> {
    let debug_session = executor
        .debug_session(run_id)
        .await
        .change_context(ServerError::ExecutionNotFound(run_id))?;

    let queued_steps = debug_session.get_queued_steps();

    let queued: Vec<QueuedStep> = queued_steps
        .iter()
        .map(|s| QueuedStep {
            step_id: s.step_id.clone(),
            status: format!("{:?}", s.status),
        })
        .collect();

    let ready: Vec<String> = queued_steps
        .iter()
        .filter(|s| s.status == StepStatus::Runnable)
        .map(|s| s.step_id.clone())
        .collect();

    Ok(Json(DebugQueueStatusResponse { queued, ready }))
}

/// Get the result of a specific step
#[utoipa::path(
    get,
    path = "/runs/{run_id}/debug/show/{step_id}",
    params(
        ("run_id" = Uuid, Path, description = "Run ID (UUID)"),
        ("step_id" = String, Path, description = "Step ID")
    ),
    responses(
        (status = 200, description = "Step result retrieved", body = DebugShowResponse),
        (status = 400, description = "Invalid run ID"),
        (status = 404, description = "Run not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::DEBUG_TAG,
)]
pub async fn debug_show(
    State(executor): State<Arc<StepflowExecutor>>,
    Path((run_id, step_id)): Path<(Uuid, String)>,
) -> Result<Json<DebugShowResponse>, ErrorResponse> {
    let debug_session = executor
        .debug_session(run_id)
        .await
        .change_context(ServerError::ExecutionNotFound(run_id))?;

    let result = debug_session.get_step_result(&step_id);

    Ok(Json(DebugShowResponse {
        step_id,
        completed: result.is_some(),
        result,
    }))
}
