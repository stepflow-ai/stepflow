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
    extract::{Path, Query, State},
    response::Json,
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use stepflow_core::status::{ExecutionStatus, StepStatus};
use stepflow_core::{
    BlobId, FlowResult,
    workflow::{Flow, ValueRef, WorkflowOverrides},
};
use stepflow_execution::StepflowExecutor;
use stepflow_state::{RunDetails, RunSummary};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{ErrorResponse, ServerError};

/// Request to create/execute a flow.
///
/// The `input` field is always an array of input values:
/// - Single-item array `[value]`: Executes one run with `value` as input
/// - Multi-item array `[v1, v2, ...]`: Executes multiple runs (batch mode)
///
/// This design avoids ambiguity: to run a workflow with an array as input,
/// wrap it in another array: `[[1, 2, 3]]` runs once with input `[1, 2, 3]`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateRunRequest {
    /// The flow hash to execute
    pub flow_id: BlobId,
    /// Input data for the flow - always an array (one element per run)
    pub input: Vec<ValueRef>,
    /// Optional workflow overrides to apply before execution
    #[serde(default, skip_serializing_if = "WorkflowOverrides::is_empty")]
    pub overrides: WorkflowOverrides,
    /// Optional variables to provide for variable references in the workflow
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub variables: HashMap<String, ValueRef>,
    /// Maximum concurrency for batch execution (only used when input is an array)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrency: Option<usize>,
}

/// Response for create run operations
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateRunResponse {
    /// The run ID (for single runs) or batch ID (for batch runs)
    pub run_id: Uuid,
    /// Number of items in this run (1 for single runs, > 1 for batch runs)
    pub item_count: u32,
    /// The result of the flow execution (if completed, for single runs only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<FlowResult>,
    /// The run status
    pub status: ExecutionStatus,
}

/// Response for listing runs
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListRunsResponse {
    /// List of run summaries
    pub runs: Vec<RunSummary>,
}

/// Query parameters for listing runs
#[derive(Debug, Clone, Default, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListRunsQuery {
    /// Filter by execution status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<stepflow_core::status::ExecutionStatus>,
    /// Filter by flow name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow_name: Option<String>,
    /// Filter by flow label
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow_label: Option<String>,
    /// Filter to runs under this root (includes the root itself)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_run_id: Option<Uuid>,
    /// Filter to direct children of this parent run
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<Uuid>,
    /// Maximum depth for hierarchy queries (0 = root only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<u32>,
    /// Maximum number of results to return
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    /// Number of results to skip
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
}

/// A single item result in a multi-item run
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ItemResult {
    /// Item index (0-based)
    pub item_index: usize,
    /// The status of this item
    pub status: ExecutionStatus,
    /// The result of the flow execution for this item (if completed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<FlowResult>,
}

/// Response for listing run items
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListItemsResponse {
    /// Total number of items in this run
    pub item_count: usize,
    /// Individual item results ordered by item index
    pub items: Vec<ItemResult>,
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
    pub flow_id: BlobId,
}

/// Create and execute a flow by hash
///
/// Supports both single and batch execution:
/// - Single input: Executes one run and returns the result directly
/// - Multiple inputs: Executes batch and waits for all results
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
    State(executor): State<Arc<StepflowExecutor>>,
    Json(req): Json<CreateRunRequest>,
) -> Result<Json<CreateRunResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    // Get the flow from the state store
    let flow = state_store
        .get_flow(&req.flow_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::WorkflowNotFound(req.flow_id.clone())))?;

    let item_count = req.input.len() as u32;

    // Execute the run
    use stepflow_plugin::Context as _;

    let mut params = stepflow_core::SubmitRunParams::new(flow, req.flow_id, req.input);
    params = params.with_wait(true);
    if let Some(max_concurrency) = req.max_concurrency {
        params = params.with_max_concurrency(max_concurrency);
    }
    if !req.overrides.is_empty() {
        params = params.with_overrides(req.overrides);
    }

    let run_status = executor.submit_run(params).await?;

    // For single-item runs, extract the result directly into the response
    let result = if item_count == 1 {
        run_status
            .results
            .as_ref()
            .and_then(|r| r.first())
            .and_then(|item| item.result.clone())
    } else {
        None // Batch results should be fetched via items endpoint
    };

    Ok(Json(CreateRunResponse {
        run_id: run_status.run_id,
        item_count,
        result,
        status: run_status.status,
    }))
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
    State(executor): State<Arc<StepflowExecutor>>,
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

/// Get all item results for a run
///
/// Returns results for all items in the run, ordered by item index.
/// For single-item runs (item_count=1), returns a single item.
#[utoipa::path(
    get,
    path = "/runs/{run_id}/items",
    params(
        ("run_id" = Uuid, Path, description = "Run ID (UUID)")
    ),
    responses(
        (status = 200, description = "Run items retrieved successfully", body = ListItemsResponse),
        (status = 400, description = "Invalid run ID format"),
        (status = 404, description = "Run not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::RUN_TAG,
)]
pub async fn get_run_items(
    State(executor): State<Arc<StepflowExecutor>>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<ListItemsResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    // Get run details to get item_count
    let run_details = state_store
        .get_run(run_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::ExecutionNotFound(run_id)))?;

    let item_count = run_details.summary.items.total;

    // Get item results from state store (ordered by item_index)
    let state_items = state_store
        .get_item_results(run_id, stepflow_state::ResultOrder::ByIndex)
        .await?;

    // Convert to API response type
    let items: Vec<ItemResult> = state_items
        .into_iter()
        .map(|item| ItemResult {
            item_index: item.item_index,
            status: item.status,
            result: item.result,
        })
        .collect();

    Ok(Json(ListItemsResponse { item_count, items }))
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
    State(executor): State<Arc<StepflowExecutor>>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<RunFlowResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    // Get execution details to retrieve the workflow hash
    let execution = state_store
        .get_run(run_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::ExecutionNotFound(run_id)))?;

    let workflow_id = execution.summary.flow_id;
    // Get the workflow from the state store
    let workflow = state_store
        .get_flow(&workflow_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::WorkflowNotFound(workflow_id.clone())))?;

    Ok(Json(RunFlowResponse {
        flow: workflow,
        flow_id: workflow_id,
    }))
}

/// List executions with optional filtering
#[utoipa::path(
    get,
    path = "/runs",
    params(
        ListRunsQuery
    ),
    responses(
        (status = 200, description = "Runs listed successfully", body = ListRunsResponse),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::RUN_TAG,
)]
pub async fn list_runs(
    State(executor): State<Arc<StepflowExecutor>>,
    Query(query): Query<ListRunsQuery>,
) -> Result<Json<ListRunsResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    let filters = stepflow_state::RunFilters {
        status: query.status,
        flow_name: query.flow_name,
        flow_label: query.flow_label,
        root_run_id: query.root_run_id,
        parent_run_id: query.parent_run_id,
        max_depth: query.max_depth,
        limit: query.limit,
        offset: query.offset,
    };

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
    State(executor): State<Arc<StepflowExecutor>>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<ListStepRunsResponse>, ErrorResponse> {
    use std::collections::HashMap;

    let state_store = executor.state_store();

    // Get execution details to retrieve the workflow hash
    let execution = state_store
        .get_run(run_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::ExecutionNotFound(run_id)))?;

    let workflow_id = execution.summary.flow_id;

    // Get the workflow from the state store
    let workflow = state_store
        .get_flow(&workflow_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::WorkflowNotFound(workflow_id.clone())))?;

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
    for (idx, step) in workflow.steps().iter().enumerate() {
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
    State(executor): State<Arc<StepflowExecutor>>,
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
                .update_run_status(run_id, ExecutionStatus::Cancelled)
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
    State(executor): State<Arc<StepflowExecutor>>,
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
