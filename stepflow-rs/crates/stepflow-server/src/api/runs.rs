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
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_core::status::{ExecutionStatus, StepStatus};
use stepflow_core::{
    FlowResult,
    workflow::{Flow, FlowHash, ValueRef},
};
use stepflow_execution::StepFlowExecutor;
use stepflow_state::{RunDetails, RunSummary};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{ErrorResponse, ServerError};

/// Request to create/execute a flow
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateRunRequest {
    /// The flow hash to execute
    pub flow_hash: FlowHash,
    /// Input data for the flow
    pub input: ValueRef,
    /// Whether to run in debug mode (pauses execution for step-by-step control)
    #[serde(default)]
    pub debug: bool,
}

/// Response for create run operations
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateRunResponse {
    /// The run ID
    pub run_id: Uuid,
    /// The result of the flow execution (if completed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<FlowResult>,
    /// The run status
    pub status: ExecutionStatus,
    /// Whether this run is in debug mode
    pub debug: bool,
}

/// Response for listing runs
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListRunsResponse {
    /// List of run summaries
    pub runs: Vec<RunSummary>,
}

/// Response for step run details
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StepRunResponse {
    /// Step index in the flow
    pub step_index: usize,
    /// Step ID
    pub step_id: String,
    /// Component name/URL that this step executes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    /// Current status of the step
    pub status: StepStatus,
    /// The result of the step execution (if completed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<FlowResult>,
}

/// Response for listing step runs
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListStepRunsResponse {
    /// Dictionary of step run results keyed by step ID
    pub steps: IndexMap<String, StepRunResponse>,
}

/// Response containing a flow definition and its hash for run endpoints
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunFlowResponse {
    /// The flow definition
    pub flow: Arc<Flow>,
    /// The flow hash
    pub flow_hash: FlowHash,
}

/// Create and execute a flow by hash
#[utoipa::path(
    post,
    path = "/runs",
    request_body = CreateRunRequest,
    responses(
        (status = 200, description = "Flow run created successfully", body = CreateRunResponse),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Flow not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::RUN_TAG,
)]
pub async fn create_run(
    State(executor): State<Arc<StepFlowExecutor>>,
    Json(req): Json<CreateRunRequest>,
) -> Result<Json<CreateRunResponse>, ErrorResponse> {
    let run_id = Uuid::new_v4();
    let state_store = executor.state_store();

    // Get the flow from the state store
    let flow = state_store
        .get_workflow(&req.flow_hash)
        .await?
        .ok_or_else(|| {
            error_stack::report!(ServerError::WorkflowNotFound(req.flow_hash.clone()))
        })?;

    // Create execution record
    state_store
        .create_run(
            run_id,
            req.flow_hash.clone(),
            flow.name.as_deref(), // Use flow name if available
            None,                 // No flow label for hash-based execution
            req.debug,
            req.input.clone(),
        )
        .await?;

    let debug_mode = req.debug;
    let input = req.input;
    let flow_hash = req.flow_hash;

    if debug_mode {
        // In debug mode, pause execution by default
        // The execution will be controlled via debug endpoints
        state_store
            .update_run_status(run_id, ExecutionStatus::Paused, None)
            .await?;

        return Ok(Json(CreateRunResponse {
            run_id,
            result: None,
            status: ExecutionStatus::Paused,
            debug: debug_mode,
        }));
    }

    // Execute the flow using the Context trait methods
    use stepflow_plugin::Context as _;

    // Submit the flow for execution
    let submitted_run_id = executor.submit_flow(flow, flow_hash, input).await?;

    // Wait for the result (synchronous execution for the HTTP endpoint)
    let flow_result = executor.flow_result(submitted_run_id).await?;

    // Check if the workflow execution was successful
    match &flow_result {
        FlowResult::Success { result } => {
            // Update execution status to completed
            state_store
                .update_run_status(run_id, ExecutionStatus::Completed, Some(result.clone()))
                .await?;

            Ok(Json(CreateRunResponse {
                run_id,
                result: Some(flow_result),
                status: ExecutionStatus::Completed,
                debug: debug_mode,
            }))
        }
        FlowResult::Failed { .. } | FlowResult::Skipped => {
            // Update execution status to failed
            state_store
                .update_run_status(run_id, ExecutionStatus::Failed, None)
                .await?;

            Ok(Json(CreateRunResponse {
                run_id,
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
    path = "/runs/{run_id}",
    params(
        ("run_id" = Uuid, Path, description = "Run ID (UUID)")
    ),
    responses(
        (status = 200, description = "Run details retrieved successfully", body = RunDetails),
        (status = 400, description = "Invalid run ID format"),
        (status = 404, description = "Run not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::RUN_TAG,
)]
pub async fn get_run(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<RunDetails>, ErrorResponse> {
    let state_store = executor.state_store();

    // Get execution details
    let details = state_store
        .get_run(run_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::ExecutionNotFound(run_id)))?;

    Ok(Json(details))
}

/// Get the workflow definition for an execution
#[utoipa::path(
    get,
    path = "/runs/{run_id}/flow",
    params(
        ("run_id" = Uuid, Path, description = "Run ID (UUID)")
    ),
    responses(
        (status = 200, description = "Run workflow retrieved successfully", body = RunFlowResponse),
        (status = 400, description = "Invalid run ID format"),
        (status = 404, description = "Run or workflow not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::RUN_TAG,
)]
pub async fn get_run_flow(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<RunFlowResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    // Get execution details to retrieve the workflow hash
    let execution = state_store
        .get_run(run_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::ExecutionNotFound(run_id)))?;

    let workflow_hash = execution.summary.flow_hash;
    // Get the workflow from the state store
    let workflow = state_store
        .get_workflow(&workflow_hash)
        .await?
        .ok_or_else(|| {
            error_stack::report!(ServerError::WorkflowNotFound(workflow_hash.clone()))
        })?;

    Ok(Json(RunFlowResponse {
        flow: workflow,
        flow_hash: workflow_hash,
    }))
}

/// List executions with optional filtering
#[utoipa::path(
    get,
    path = "/runs",
    responses(
        (status = 200, description = "Runs listed successfully", body = ListRunsResponse),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::RUN_TAG,
)]
pub async fn list_runs(
    State(executor): State<Arc<StepFlowExecutor>>,
) -> Result<Json<ListRunsResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    // TODO: Add query parameters for filtering (status, workflow_name, workflow_label, limit, offset)
    let filters = stepflow_state::RunFilters::default();

    let executions = state_store.list_runs(&filters).await?;

    Ok(Json(ListRunsResponse { runs: executions }))
}

/// Get step-level execution details for a specific execution
#[utoipa::path(
    get,
    path = "/runs/{run_id}/steps",
    params(
        ("run_id" = Uuid, Path, description = "Run ID (UUID)")
    ),
    responses(
        (status = 200, description = "Run step details retrieved successfully", body = ListStepRunsResponse),
        (status = 400, description = "Invalid run ID format"),
        (status = 404, description = "Run not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::RUN_TAG,
)]
pub async fn get_run_steps(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<ListStepRunsResponse>, ErrorResponse> {
    use std::collections::HashMap;

    let state_store = executor.state_store();

    // Get execution details to retrieve the workflow hash
    let execution = state_store
        .get_run(run_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::ExecutionNotFound(run_id)))?;

    let workflow_hash = execution.summary.flow_hash;

    // Get the workflow from the state store
    let workflow = state_store
        .get_workflow(&workflow_hash)
        .await?
        .ok_or_else(|| {
            error_stack::report!(ServerError::WorkflowNotFound(workflow_hash.clone()))
        })?;

    // Get step results for completed steps
    let step_results = state_store.list_step_results(run_id).await?;

    let mut completed_steps: HashMap<usize, stepflow_state::StepResult> = HashMap::new();
    for step_result in step_results {
        completed_steps.insert(step_result.step_idx(), step_result);
    }

    // Create unified response with both status and results
    let mut step_responses = IndexMap::new();

    // Get step status from state store
    let step_statuses = {
        let step_info_list = state_store.get_step_info_for_execution(run_id).await?;
        let mut status_map = HashMap::new();
        for step_info in step_info_list {
            status_map.insert(step_info.step_index, step_info.status);
        }
        status_map
    };

    // Build unified responses
    for (idx, step) in workflow.steps.iter().enumerate() {
        let status = step_statuses
            .get(&idx)
            .copied()
            .unwrap_or(StepStatus::Blocked);
        let result = completed_steps.get(&idx).map(|sr| sr.result().clone());

        let step_response = StepRunResponse {
            step_index: idx,
            step_id: step.id.clone(),
            component: Some(step.component.to_string()),
            status,
            result,
        };

        step_responses.insert(step.id.clone(), step_response);
    }

    Ok(Json(ListStepRunsResponse {
        steps: step_responses,
    }))
}

/// Cancel a running execution
#[utoipa::path(
    post,
    path = "/runs/{run_id}/cancel",
    params(
        ("run_id" = Uuid, Path, description = "Run ID (UUID)")
    ),
    responses(
        (status = 200, description = "Run cancelled successfully", body = RunSummary),
        (status = 400, description = "Invalid run ID format"),
        (status = 404, description = "Run not found"),
        (status = 409, description = "Run already completed"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::RUN_TAG,
)]
pub async fn cancel_run(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<RunSummary>, ErrorResponse> {
    let state_store = executor.state_store();

    // Get execution to check current status
    let execution = state_store
        .get_run(run_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::ExecutionNotFound(run_id)))?;

    // Check if execution can be cancelled
    match execution.summary.status {
        ExecutionStatus::Completed | ExecutionStatus::Failed | ExecutionStatus::Cancelled => {
            return Err(error_stack::report!(ServerError::ExecutionNotCancellable {
                run_id,
                status: execution.summary.status
            })
            .into());
        }
        ExecutionStatus::Running | ExecutionStatus::Paused => {
            // TODO: Implement actual execution cancellation logic
            // For now, just update the status in the database
            state_store
                .update_run_status(run_id, ExecutionStatus::Cancelled, None)
                .await?;
        }
    }

    // Return updated execution summary
    let updated_execution = state_store
        .get_run(run_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::ExecutionNotFound(run_id)))?;

    Ok(Json(updated_execution.summary))
}

/// Delete a completed execution
#[utoipa::path(
    delete,
    path = "/runs/{run_id}",
    params(
        ("run_id" = Uuid, Path, description = "Run ID (UUID)")
    ),
    responses(
        (status = 204, description = "Run deleted successfully"),
        (status = 400, description = "Invalid run ID format"),
        (status = 404, description = "Run not found"),
        (status = 409, description = "Run still running"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::RUN_TAG,
)]
pub async fn delete_run(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(run_id): Path<Uuid>,
) -> Result<(), ErrorResponse> {
    let state_store = executor.state_store();

    // Get execution to check current status
    let execution = state_store
        .get_run(run_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::ExecutionNotFound(run_id)))?;

    // Check if execution can be deleted (only allow deletion of non-running executions)
    match execution.summary.status {
        ExecutionStatus::Running | ExecutionStatus::Paused => {
            return Err(error_stack::report!(ServerError::ExecutionStillRunning(run_id)).into());
        }
        ExecutionStatus::Completed | ExecutionStatus::Failed | ExecutionStatus::Cancelled => {
            // TODO: Implement actual execution deletion logic
            // This should remove execution record and all associated step results
            // For now, this is a placeholder
        }
    }

    Ok(())
}
