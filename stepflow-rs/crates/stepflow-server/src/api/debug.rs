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

//! Debug API endpoints for interactive workflow debugging.
//!
//! These endpoints provide GDB-like debugging capabilities:
//! - `GET /steps` - List steps with optional filtering
//! - `GET /steps/{step_id}` - Get detailed step info
//! - `GET /status` - Get current debug session status
//! - `GET /events` - Get debug event history
//! - `POST /eval` - Evaluate a step (queue + run to completion)
//! - `POST /next` - Execute next ready step (step-over for sub-flows)
//! - `POST /step` - Step into sub-flow if pending, else next
//! - `POST /continue` - Run to completion

use axum::{
    extract::{Path, Query, State},
    response::Json,
};
use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_core::FlowResult;
use stepflow_dtos::{
    ContinueResult, DebugEvent, DebugStatus, StepDetail, StepExecutionResult, StepInfo,
    StepStatusFilter,
};
use stepflow_execution::StepflowExecutor;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{ErrorResponse, ServerError};

// ============================================================================
// Query Parameters
// ============================================================================

/// Query parameters for listing steps.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct ListStepsQuery {
    /// Filter by step status.
    #[serde(default)]
    pub status: Option<StepStatusFilter>,
}

/// Query parameters for debug events.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DebugEventsQuery {
    /// Maximum number of events to return (default: 10).
    #[serde(default = "default_events_limit")]
    pub limit: usize,
    /// Offset for pagination (default: 0).
    #[serde(default)]
    pub offset: usize,
}

fn default_events_limit() -> usize {
    10
}

impl Default for DebugEventsQuery {
    fn default() -> Self {
        Self {
            limit: default_events_limit(),
            offset: 0,
        }
    }
}

// ============================================================================
// Request Bodies
// ============================================================================

/// Request to evaluate a step.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EvalRequest {
    /// Step ID to evaluate.
    pub step_id: String,
}

// ============================================================================
// Response Types
// ============================================================================

/// Response containing a list of steps.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListStepsResponse {
    /// The steps matching the query.
    pub steps: Vec<StepInfo>,
    /// Total number of steps (before filtering).
    pub total: usize,
}

/// Response containing debug events.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DebugEventsResponse {
    /// Debug events (newest first).
    pub events: Vec<DebugEvent>,
    /// Number of events returned.
    pub count: usize,
}

/// Response from eval endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EvalResponse {
    /// Result of the evaluated step.
    pub result: FlowResult,
    /// Updated debug status.
    pub status: DebugStatus,
}

// ============================================================================
// Step Information Endpoints
// ============================================================================

/// List all steps in the debug session.
///
/// Returns a list of steps with their current status. Use the `status` query
/// parameter to filter by step status (completed, pending, runnable, blocked).
#[utoipa::path(
    get,
    path = "/runs/{run_id}/debug/steps",
    params(
        ("run_id" = Uuid, Path, description = "Run ID (UUID)"),
        ("status" = Option<StepStatusFilter>, Query, description = "Filter by step status")
    ),
    responses(
        (status = 200, description = "Steps retrieved", body = ListStepsResponse),
        (status = 404, description = "Run not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::DEBUG_TAG,
)]
pub async fn list_steps(
    State(executor): State<Arc<StepflowExecutor>>,
    Path(run_id): Path<Uuid>,
    Query(query): Query<ListStepsQuery>,
) -> Result<Json<ListStepsResponse>, ErrorResponse> {
    let debug_session = executor
        .debug_session(run_id)
        .await
        .change_context(ServerError::ExecutionNotFound(run_id))?;

    let all_steps = debug_session.get_steps_info().await;
    let total = all_steps.len();

    // Apply filter if specified
    let steps = match query.status {
        Some(StepStatusFilter::Completed) => all_steps
            .into_iter()
            .filter(|s| s.status == stepflow_core::status::StepStatus::Completed)
            .collect(),
        Some(StepStatusFilter::Pending) => all_steps
            .into_iter()
            .filter(|s| {
                s.status == stepflow_core::status::StepStatus::Runnable
                    || s.status == stepflow_core::status::StepStatus::Blocked
            })
            .collect(),
        Some(StepStatusFilter::Runnable) => all_steps
            .into_iter()
            .filter(|s| s.status == stepflow_core::status::StepStatus::Runnable)
            .collect(),
        Some(StepStatusFilter::Blocked) => all_steps
            .into_iter()
            .filter(|s| s.status == stepflow_core::status::StepStatus::Blocked)
            .collect(),
        Some(StepStatusFilter::All) | None => all_steps,
    };

    Ok(Json(ListStepsResponse { steps, total }))
}

/// Get detailed information about a specific step.
///
/// Returns the step's input expression, error handling configuration,
/// result (if completed), and dependencies.
#[utoipa::path(
    get,
    path = "/runs/{run_id}/debug/steps/{step_id}",
    params(
        ("run_id" = Uuid, Path, description = "Run ID (UUID)"),
        ("step_id" = String, Path, description = "Step ID")
    ),
    responses(
        (status = 200, description = "Step detail retrieved", body = StepDetail),
        (status = 404, description = "Run or step not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::DEBUG_TAG,
)]
pub async fn get_step(
    State(executor): State<Arc<StepflowExecutor>>,
    Path((run_id, step_id)): Path<(Uuid, String)>,
) -> Result<Json<StepDetail>, ErrorResponse> {
    let debug_session = executor
        .debug_session(run_id)
        .await
        .change_context(ServerError::ExecutionNotFound(run_id))?;

    let detail = debug_session.get_step_detail(&step_id).await?;

    Ok(Json(detail))
}

// ============================================================================
// Status and History Endpoints
// ============================================================================

/// Get the current debug session status.
///
/// Returns the pending action (what will happen on next step/continue),
/// list of ready steps, and progress information.
#[utoipa::path(
    get,
    path = "/runs/{run_id}/debug/status",
    params(
        ("run_id" = Uuid, Path, description = "Run ID (UUID)")
    ),
    responses(
        (status = 200, description = "Status retrieved", body = DebugStatus),
        (status = 404, description = "Run not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::DEBUG_TAG,
)]
pub async fn get_status(
    State(executor): State<Arc<StepflowExecutor>>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<DebugStatus>, ErrorResponse> {
    let debug_session = executor
        .debug_session(run_id)
        .await
        .change_context(ServerError::ExecutionNotFound(run_id))?;

    let status = debug_session.get_status();

    Ok(Json(status))
}

/// Get debug event history.
///
/// Returns debug events in newest-first order, with optional pagination.
#[utoipa::path(
    get,
    path = "/runs/{run_id}/debug/events",
    params(
        ("run_id" = Uuid, Path, description = "Run ID (UUID)"),
        ("limit" = Option<usize>, Query, description = "Maximum number of events (default: 10)"),
        ("offset" = Option<usize>, Query, description = "Offset for pagination (default: 0)")
    ),
    responses(
        (status = 200, description = "Events retrieved", body = DebugEventsResponse),
        (status = 404, description = "Run not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::DEBUG_TAG,
)]
pub async fn get_events(
    State(executor): State<Arc<StepflowExecutor>>,
    Path(run_id): Path<Uuid>,
    Query(query): Query<DebugEventsQuery>,
) -> Result<Json<DebugEventsResponse>, ErrorResponse> {
    let debug_session = executor
        .debug_session(run_id)
        .await
        .change_context(ServerError::ExecutionNotFound(run_id))?;

    let events = debug_session
        .get_debug_events(query.limit, query.offset)
        .await?;
    let count = events.len();

    Ok(Json(DebugEventsResponse { events, count }))
}

// ============================================================================
// Execution Control Endpoints
// ============================================================================

/// Evaluate a step: queue it and its dependencies, run to completion.
///
/// This is idempotent - if the step is already completed, returns the cached result.
#[utoipa::path(
    post,
    path = "/runs/{run_id}/debug/eval",
    params(
        ("run_id" = Uuid, Path, description = "Run ID (UUID)")
    ),
    request_body = EvalRequest,
    responses(
        (status = 200, description = "Step evaluated", body = EvalResponse),
        (status = 404, description = "Run or step not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::DEBUG_TAG,
)]
pub async fn eval(
    State(executor): State<Arc<StepflowExecutor>>,
    Path(run_id): Path<Uuid>,
    Json(req): Json<EvalRequest>,
) -> Result<Json<EvalResponse>, ErrorResponse> {
    let mut debug_session = executor
        .debug_session(run_id)
        .await
        .change_context(ServerError::ExecutionNotFound(run_id))?;

    let result = debug_session.eval_step(&req.step_id).await?;
    let status = debug_session.get_status();

    Ok(Json(EvalResponse { result, status }))
}

/// Execute the next ready step (step-over for sub-flows).
///
/// If a sub-flow is pending, this runs it to completion before returning.
/// Returns the executed step and result, or indicates no steps are ready.
#[utoipa::path(
    post,
    path = "/runs/{run_id}/debug/next",
    params(
        ("run_id" = Uuid, Path, description = "Run ID (UUID)")
    ),
    responses(
        (status = 200, description = "Step executed", body = StepExecutionResult),
        (status = 404, description = "Run not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::DEBUG_TAG,
)]
pub async fn next(
    State(executor): State<Arc<StepflowExecutor>>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<StepExecutionResult>, ErrorResponse> {
    let mut debug_session = executor
        .debug_session(run_id)
        .await
        .change_context(ServerError::ExecutionNotFound(run_id))?;

    // Auto-queue must_execute steps if nothing is ready
    // This ensures `next` works like GDB's "next" - execute the next step
    debug_session.queue_must_execute().await?;

    let step_result = debug_session.run_next_step().await?;

    let (step_id, result) = match &step_result {
        Some(r) => (Some(r.metadata.step_id.clone()), Some(r.result.clone())),
        None => (None, None),
    };

    let status = debug_session.get_status();

    Ok(Json(StepExecutionResult {
        step_id,
        result,
        status,
    }))
}

/// Step into a sub-flow if pending, otherwise execute next step.
///
/// When a sub-flow is pending, this enters the sub-flow and changes the
/// current run context. Otherwise, behaves like `next`.
#[utoipa::path(
    post,
    path = "/runs/{run_id}/debug/step",
    params(
        ("run_id" = Uuid, Path, description = "Run ID (UUID)")
    ),
    responses(
        (status = 200, description = "Step executed", body = StepExecutionResult),
        (status = 404, description = "Run not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::DEBUG_TAG,
)]
pub async fn step(
    State(executor): State<Arc<StepflowExecutor>>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<StepExecutionResult>, ErrorResponse> {
    let mut debug_session = executor
        .debug_session(run_id)
        .await
        .change_context(ServerError::ExecutionNotFound(run_id))?;

    // Auto-queue must_execute steps if nothing is ready
    debug_session.queue_must_execute().await?;

    // For now, step and next behave the same (sub-flow support is future work)
    let step_result = debug_session.run_next_step().await?;

    let (step_id, result) = match &step_result {
        Some(r) => (Some(r.metadata.step_id.clone()), Some(r.result.clone())),
        None => (None, None),
    };

    let status = debug_session.get_status();

    Ok(Json(StepExecutionResult {
        step_id,
        result,
        status,
    }))
}

/// Complete current sub-flow and return to parent run.
///
/// If in a sub-flow, runs it to completion and returns to the parent context.
/// If at the root run, this is equivalent to `continue`.
#[utoipa::path(
    post,
    path = "/runs/{run_id}/debug/up",
    params(
        ("run_id" = Uuid, Path, description = "Run ID (UUID)")
    ),
    responses(
        (status = 200, description = "Sub-flow completed", body = ContinueResult),
        (status = 404, description = "Run not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::DEBUG_TAG,
)]
pub async fn up(
    State(executor): State<Arc<StepflowExecutor>>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<ContinueResult>, ErrorResponse> {
    let mut debug_session = executor
        .debug_session(run_id)
        .await
        .change_context(ServerError::ExecutionNotFound(run_id))?;

    // For now, up behaves like continue (sub-flow support is future work)
    let (result, steps_executed) = debug_session.continue_to_completion().await?;
    let status = debug_session.get_status();

    Ok(Json(ContinueResult {
        result,
        steps_executed,
        status,
    }))
}

/// Run to completion.
///
/// Executes all remaining steps until the workflow completes.
#[utoipa::path(
    post,
    path = "/runs/{run_id}/debug/continue",
    params(
        ("run_id" = Uuid, Path, description = "Run ID (UUID)")
    ),
    responses(
        (status = 200, description = "Execution completed", body = ContinueResult),
        (status = 404, description = "Run not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::DEBUG_TAG,
)]
pub async fn continue_execution(
    State(executor): State<Arc<StepflowExecutor>>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<ContinueResult>, ErrorResponse> {
    let mut debug_session = executor
        .debug_session(run_id)
        .await
        .change_context(ServerError::ExecutionNotFound(run_id))?;

    let (result, steps_executed) = debug_session.continue_to_completion().await?;
    let status = debug_session.get_status();

    Ok(Json(ContinueResult {
        result,
        steps_executed,
        status,
    }))
}
