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

use aide::transform::TransformOperation;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use stepflow_core::status::{ExecutionStatus, StepStatus};
use stepflow_core::{
    BlobId, DEFAULT_WAIT_TIMEOUT_SECS, FlowResult,
    workflow::{Flow, ValueRef, WorkflowOverrides},
};
use stepflow_dtos::{ItemResult, ResultOrder, RunDetails, RunFilters, RunStatus, RunSummary};
use stepflow_plugin::StepflowEnvironment;
use stepflow_state::{BlobStoreExt as _, MetadataStoreExt as _};
use uuid::Uuid;

use crate::error::{ErrorResponse, ServerError};

/// Path parameters for run endpoints
#[derive(Deserialize, schemars::JsonSchema)]
pub struct RunPath {
    /// Run ID (UUID)
    pub run_id: Uuid,
}

/// Request to create/execute a flow.
///
/// The `input` field is always an array of input values:
/// - Single-item array `[value]`: Executes one run with `value` as input
/// - Multi-item array `[v1, v2, ...]`: Executes multiple runs (batch mode)
///
/// This design avoids ambiguity: to run a workflow with an array as input,
/// wrap it in another array: `[[1, 2, 3]]` runs once with input `[1, 2, 3]`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
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
    /// If true, block until the run completes and return the result (200 OK).
    /// If false (default), return immediately with status Running (202 Accepted).
    #[serde(default)]
    pub wait: bool,
    /// Maximum seconds to wait when wait=true (default 300). If the timeout elapses,
    /// returns the current run status rather than an error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

/// Response for create run operations.
///
/// Shares the same base fields as `RunDetails` (GET /runs/{id}) via `RunSummary`,
/// with optional item results when `wait=true`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateRunResponse {
    /// Run summary fields (run_id, flow_id, status, items, timestamps, etc.)
    #[serde(flatten)]
    pub summary: RunSummary,
    /// Item results, only populated when `wait=true` and the run has completed.
    /// Each entry contains the item's index, status, and result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<Vec<ItemResult>>,
}

impl From<RunStatus> for CreateRunResponse {
    fn from(status: RunStatus) -> Self {
        Self {
            summary: RunSummary {
                run_id: status.run_id,
                flow_id: status.flow_id,
                flow_name: status.flow_name,
                status: status.status,
                items: status.items,
                created_at: status.created_at,
                completed_at: status.completed_at,
                root_run_id: status.root_run_id,
                parent_run_id: status.parent_run_id,
                orchestrator_id: None,
                created_at_seqno: status.created_at_seqno,
                finished_at_seqno: None,
            },
            results: status.results,
        }
    }
}

/// Response for listing runs
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListRunsResponse {
    /// List of run summaries
    pub runs: Vec<RunSummary>,
}

/// Query parameters for listing runs
#[derive(Debug, Clone, Default, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListRunsQuery {
    /// Filter by execution status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<stepflow_core::status::ExecutionStatus>,
    /// Filter by flow name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow_name: Option<String>,
    /// Filter to runs under this root (includes the root itself)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_run_id: Option<Uuid>,
    /// Filter to direct children of this parent run
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<Uuid>,
    /// Filter to only root runs (runs where parent_run_id is None)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roots_only: Option<bool>,
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

/// Response for listing run items
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListItemsResponse {
    /// Total number of items in this run
    pub item_count: usize,
    /// Individual item results ordered by item index
    pub items: Vec<ItemResult>,
}

/// Response for step run details
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StepRunResponse {
    /// Step ID (the stable identifier for this step in the workflow)
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
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListStepRunsResponse {
    /// Dictionary of step run results keyed by step ID
    pub steps: IndexMap<String, StepRunResponse>,
}

/// Query parameters for step runs endpoint
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StepRunsQuery {
    /// Optional item index for multi-item (batch) runs.
    /// If not specified, step statuses are aggregated across all items
    /// using "worst status" precedence (Failed > Running > Runnable > Blocked > Skipped > Completed).
    #[serde(default)]
    pub item_index: Option<usize>,
}

/// Response containing a flow definition and its hash for run endpoints
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunFlowResponse {
    /// The flow definition
    pub flow: Arc<Flow>,
    /// The flow hash
    pub flow_id: BlobId,
}

pub fn create_run_docs(op: TransformOperation<'_>) -> TransformOperation<'_> {
    op.id("createRun")
        .summary("Create and execute a flow run")
        .description(
            "Create and execute a flow by hash. Supports both single and batch execution. \
             By default, returns immediately with 202 Accepted. Set `wait: true` to block \
             until the run completes and return 200 OK with the result.",
        )
        .tag("Run")
        .response_with::<202, Json<CreateRunResponse>, _>(|res| {
            res.description("Run created and executing asynchronously")
        })
        .response_with::<200, Json<CreateRunResponse>, _>(|res| {
            res.description("Run completed (when wait=true)")
        })
        .response_with::<400, ErrorResponse, _>(|res| res.description("Invalid request"))
        .response_with::<404, ErrorResponse, _>(|res| res.description("Flow not found"))
}

/// Create and execute a flow by hash
///
/// Supports both single and batch execution:
/// - Single input: Executes one run
/// - Multiple inputs: Executes multiple runs (batch mode)
///
/// By default, returns immediately with 202 Accepted and status Running.
/// Set `wait: true` in the request body to block until the run completes
/// and return 200 OK with the result.
pub async fn create_run(
    State(executor): State<Arc<StepflowEnvironment>>,
    Json(req): Json<CreateRunRequest>,
) -> Result<(StatusCode, Json<CreateRunResponse>), ErrorResponse> {
    let blob_store = executor.blob_store();

    // Get the flow from the blob store
    let flow = blob_store
        .get_flow(&req.flow_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::WorkflowNotFound(req.flow_id.clone())))?;

    let wait = req.wait;
    let timeout_secs = req.timeout_secs;

    // Execute the run
    log::info!(
        "create_run: flow_id={}, {} variables received: {:?}",
        req.flow_id,
        req.variables.len(),
        req.variables.keys().collect::<Vec<_>>()
    );
    let variables = if req.variables.is_empty() {
        None
    } else {
        Some(req.variables)
    };
    let params = stepflow_core::SubmitRunParams {
        max_concurrency: req.max_concurrency,
        overrides: req.overrides,
        variables,
    };

    let run_status =
        stepflow_execution::submit_run(&executor, flow, req.flow_id, req.input, params).await?;

    if wait {
        // Wait for completion with timeout, then return results
        let timeout_duration =
            std::time::Duration::from_secs(timeout_secs.unwrap_or(DEFAULT_WAIT_TIMEOUT_SECS));
        if let Ok(Err(e)) = tokio::time::timeout(
            timeout_duration,
            stepflow_execution::wait_for_completion(&executor, run_status.run_id),
        )
        .await
        {
            return Err(e.into());
        }

        let run_status = stepflow_execution::get_run(
            &executor,
            run_status.run_id,
            stepflow_core::GetRunParams {
                include_results: true,
                ..Default::default()
            },
        )
        .await?;

        Ok((StatusCode::OK, Json(CreateRunResponse::from(run_status))))
    } else {
        // Return immediately with 202 Accepted
        Ok((
            StatusCode::ACCEPTED,
            Json(CreateRunResponse::from(run_status)),
        ))
    }
}

/// Query parameters for getting a run by ID
#[derive(Debug, Clone, Default, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetRunQuery {
    /// If true, wait for the run to reach a terminal state before responding.
    #[serde(default)]
    pub wait: Option<bool>,
    /// Maximum seconds to wait when wait=true (default 300). If the timeout elapses,
    /// returns the current run status rather than an error.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

pub fn get_run_docs(op: TransformOperation<'_>) -> TransformOperation<'_> {
    op.id("getRun")
        .summary("Get run details by ID")
        .description(
            "Returns the current run status and details. Use `wait=true` to long-poll \
             until the run reaches a terminal state (completed, failed, or cancelled).",
        )
        .tag("Run")
        .response_with::<404, ErrorResponse, _>(|res| res.description("Run not found"))
}

/// Get execution details by ID
///
/// Returns the current run status and details. Use `wait=true` to long-poll
/// until the run reaches a terminal state (completed, failed, or cancelled).
pub async fn get_run(
    State(executor): State<Arc<StepflowEnvironment>>,
    Path(RunPath { run_id }): Path<RunPath>,
    Query(query): Query<GetRunQuery>,
) -> Result<Json<RunDetails>, ErrorResponse> {
    // If wait=true, block until the run reaches a terminal state (with timeout)
    if query.wait.unwrap_or(false) {
        let timeout_duration =
            std::time::Duration::from_secs(query.timeout_secs.unwrap_or(DEFAULT_WAIT_TIMEOUT_SECS));
        // Ignore timeout — return current status either way, but propagate actual errors
        if let Ok(Err(e)) = tokio::time::timeout(
            timeout_duration,
            stepflow_execution::wait_for_completion(&executor, run_id),
        )
        .await
        {
            return Err(e.into());
        }
    }

    let metadata_store = executor.metadata_store();

    // Get execution details
    let details = metadata_store
        .get_run(run_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::ExecutionNotFound(run_id)))?;

    Ok(Json(details))
}

pub fn get_run_items_docs(op: TransformOperation<'_>) -> TransformOperation<'_> {
    op.id("getRunItems")
        .summary("Get all item results for a run")
        .description(
            "Returns results for all items in the run, ordered by item index. \
             For single-item runs (item_count=1), returns a single item.",
        )
        .tag("Run")
        .response_with::<404, ErrorResponse, _>(|res| res.description("Run not found"))
}

/// Get all item results for a run
///
/// Returns results for all items in the run, ordered by item index.
/// For single-item runs (item_count=1), returns a single item.
pub async fn get_run_items(
    State(executor): State<Arc<StepflowEnvironment>>,
    Path(RunPath { run_id }): Path<RunPath>,
) -> Result<Json<ListItemsResponse>, ErrorResponse> {
    let metadata_store = executor.metadata_store();

    // Get run details to get item_count
    let run_details = metadata_store
        .get_run(run_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::ExecutionNotFound(run_id)))?;

    let item_count = run_details.summary.items.total;

    // Get item results from metadata store (ordered by item_index)
    let items = metadata_store
        .get_item_results(run_id, ResultOrder::ByIndex)
        .await?;

    Ok(Json(ListItemsResponse { item_count, items }))
}

pub fn get_run_flow_docs(op: TransformOperation<'_>) -> TransformOperation<'_> {
    op.id("getRunFlow")
        .summary("Get the workflow definition for a run")
        .description("Retrieve the workflow definition associated with a specific run.")
        .tag("Run")
        .response_with::<404, ErrorResponse, _>(|res| res.description("Run not found"))
}

/// Get the workflow definition for an execution
pub async fn get_run_flow(
    State(executor): State<Arc<StepflowEnvironment>>,
    Path(RunPath { run_id }): Path<RunPath>,
) -> Result<Json<RunFlowResponse>, ErrorResponse> {
    let metadata_store = executor.metadata_store();
    let blob_store = executor.blob_store();

    // Get execution details to retrieve the workflow hash
    let execution = metadata_store
        .get_run(run_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::ExecutionNotFound(run_id)))?;

    let workflow_id = execution.summary.flow_id;
    // Get the workflow from the blob store
    let workflow = blob_store
        .get_flow(&workflow_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::WorkflowNotFound(workflow_id.clone())))?;

    Ok(Json(RunFlowResponse {
        flow: workflow,
        flow_id: workflow_id,
    }))
}

pub fn list_runs_docs(op: TransformOperation<'_>) -> TransformOperation<'_> {
    op.id("listRuns")
        .summary("List runs with optional filtering")
        .description("List executions with optional status, flow name, and hierarchy filtering.")
        .tag("Run")
        .response_with::<400, ErrorResponse, _>(|res| res.description("Invalid query parameters"))
}

/// List executions with optional filtering
pub async fn list_runs(
    State(executor): State<Arc<StepflowEnvironment>>,
    Query(query): Query<ListRunsQuery>,
) -> Result<Json<ListRunsResponse>, ErrorResponse> {
    let metadata_store = executor.metadata_store();

    let filters = RunFilters {
        status: query.status,
        flow_name: query.flow_name,
        root_run_id: query.root_run_id,
        parent_run_id: query.parent_run_id,
        roots_only: query.roots_only,
        max_depth: query.max_depth,
        orchestrator_id: None,
        created_after_seqno: None,
        not_finished_before_seqno: None,
        limit: query.limit,
        offset: query.offset,
    };

    let executions = metadata_store.list_runs(&filters).await?;

    Ok(Json(ListRunsResponse { runs: executions }))
}

pub fn get_run_steps_docs(op: TransformOperation<'_>) -> TransformOperation<'_> {
    op.id("getRunSteps")
        .summary("Get step-level execution details")
        .description(
            "Get step-level execution details for a specific run. \
             Use `item_index` to get statuses for a specific item in batch runs. \
             Without `item_index`, statuses are aggregated across all items.",
        )
        .tag("Run")
        .response_with::<404, ErrorResponse, _>(|res| res.description("Run not found"))
}

/// Get step-level execution details for a specific execution
pub async fn get_run_steps(
    State(executor): State<Arc<StepflowEnvironment>>,
    Path(RunPath { run_id }): Path<RunPath>,
    Query(query): Query<StepRunsQuery>,
) -> Result<Json<ListStepRunsResponse>, ErrorResponse> {
    let metadata_store = executor.metadata_store();
    let blob_store = executor.blob_store();

    // Get execution details to retrieve the workflow hash and step statuses
    let execution = metadata_store
        .get_run(run_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::ExecutionNotFound(run_id)))?;

    let workflow_id = execution.summary.flow_id;

    // Get the workflow from the blob store
    let workflow = blob_store
        .get_flow(&workflow_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::WorkflowNotFound(workflow_id.clone())))?;

    // Build step status map from ItemDetails.steps
    let step_statuses: HashMap<String, StepStatus> = if let Some(item_index) = query.item_index {
        // Specific item requested - return statuses for that item only
        execution
            .item_details
            .as_ref()
            .and_then(|items| items.get(item_index))
            .map(|item| {
                item.steps
                    .iter()
                    .map(|s| (s.step_id.clone(), s.status))
                    .collect()
            })
            .ok_or_else(|| {
                error_stack::report!(ServerError::InvalidRequest(format!(
                    "Item index {} out of bounds (run has {} items)",
                    item_index,
                    execution
                        .item_details
                        .as_ref()
                        .map(|i| i.len())
                        .unwrap_or(0)
                )))
            })?
    } else {
        // No item specified - aggregate across all items using "worst status" precedence
        aggregate_step_statuses(&execution.item_details)
    };

    // Build step responses from workflow definition.
    // Status comes from ItemDetails.steps; results are not included (use journal for detailed output).
    let mut step_responses = IndexMap::new();

    for step in workflow.steps().iter() {
        let status = step_statuses
            .get(&step.id)
            .copied()
            .unwrap_or(StepStatus::Blocked);

        let step_response = StepRunResponse {
            step_id: step.id.clone(),
            component: Some(step.component.to_string()),
            status,
            result: None, // Results not included; use debug API with journal for step outputs
        };

        step_responses.insert(step.id.clone(), step_response);
    }

    Ok(Json(ListStepRunsResponse {
        steps: step_responses,
    }))
}

/// Aggregate step statuses across all items using "worst status" precedence.
///
/// For each step, the aggregated status is the "worst" status across all items:
/// Failed > Running > Runnable > Blocked > Skipped > Completed
fn aggregate_step_statuses(
    item_details: &Option<Vec<stepflow_dtos::ItemDetails>>,
) -> HashMap<String, StepStatus> {
    let items = match item_details {
        Some(items) if !items.is_empty() => items,
        _ => return HashMap::new(),
    };

    let mut aggregated: HashMap<String, StepStatus> = HashMap::new();

    for item in items {
        for step in &item.steps {
            let current = aggregated
                .entry(step.step_id.clone())
                .or_insert(step.status);
            // Update if new status has higher precedence (is "worse")
            if status_precedence(step.status) > status_precedence(*current) {
                *current = step.status;
            }
        }
    }

    aggregated
}

/// Returns a precedence value for step status (higher = worse/more attention needed).
fn status_precedence(status: StepStatus) -> u8 {
    match status {
        StepStatus::Completed => 0,
        StepStatus::Skipped => 1,
        StepStatus::Blocked => 2,
        StepStatus::Runnable => 3,
        StepStatus::Running => 4,
        StepStatus::Failed => 5,
    }
}

pub fn cancel_run_docs(op: TransformOperation<'_>) -> TransformOperation<'_> {
    op.id("cancelRun")
        .summary("Cancel a running execution")
        .description("Cancel a running execution by ID.")
        .tag("Run")
        .response_with::<404, ErrorResponse, _>(|res| res.description("Run not found"))
        .response_with::<409, ErrorResponse, _>(|res| {
            res.description("Run cannot be cancelled (already completed)")
        })
}

/// Cancel a running execution
pub async fn cancel_run(
    State(executor): State<Arc<StepflowEnvironment>>,
    Path(RunPath { run_id }): Path<RunPath>,
) -> Result<Json<RunSummary>, ErrorResponse> {
    let metadata_store = executor.metadata_store();

    // Get execution to check current status
    let execution = metadata_store
        .get_run(run_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::ExecutionNotFound(run_id)))?;

    // Check if execution can be cancelled
    match execution.summary.status {
        ExecutionStatus::Completed
        | ExecutionStatus::Failed
        | ExecutionStatus::Cancelled
        | ExecutionStatus::RecoveryFailed => {
            return Err(error_stack::report!(ServerError::ExecutionNotCancellable {
                run_id,
                status: execution.summary.status
            })
            .into());
        }
        ExecutionStatus::Running | ExecutionStatus::Paused => {
            // TODO: Implement actual execution cancellation logic
            // For now, just update the status in the database
            metadata_store
                .update_run_status(run_id, ExecutionStatus::Cancelled, None)
                .await?;
        }
    }

    // Return updated execution summary
    let updated_execution = metadata_store
        .get_run(run_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::ExecutionNotFound(run_id)))?;

    Ok(Json(updated_execution.summary))
}

pub fn delete_run_docs(op: TransformOperation<'_>) -> TransformOperation<'_> {
    op.id("deleteRun")
        .summary("Delete a completed execution")
        .description("Delete a completed execution by ID. Running executions cannot be deleted.")
        .tag("Run")
        .response_with::<404, ErrorResponse, _>(|res| res.description("Run not found"))
        .response_with::<409, ErrorResponse, _>(|res| {
            res.description("Run is still running and cannot be deleted")
        })
}

/// Delete a completed execution
pub async fn delete_run(
    State(executor): State<Arc<StepflowEnvironment>>,
    Path(RunPath { run_id }): Path<RunPath>,
) -> Result<(), ErrorResponse> {
    let metadata_store = executor.metadata_store();

    // Get execution to check current status
    let execution = metadata_store
        .get_run(run_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::ExecutionNotFound(run_id)))?;

    // Check if execution can be deleted (only allow deletion of non-running executions)
    match execution.summary.status {
        ExecutionStatus::Running | ExecutionStatus::Paused => {
            return Err(error_stack::report!(ServerError::ExecutionStillRunning(run_id)).into());
        }
        ExecutionStatus::Completed
        | ExecutionStatus::Failed
        | ExecutionStatus::Cancelled
        | ExecutionStatus::RecoveryFailed => {
            // TODO: Implement actual execution deletion logic
            // This should remove execution record and all associated step results
            // For now, this is a placeholder
        }
    }

    Ok(())
}
