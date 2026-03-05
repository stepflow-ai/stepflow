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
use futures::StreamExt as _;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_with::{DefaultOnNull, serde_as};
use std::collections::HashMap;
use std::sync::Arc;
use stepflow_core::status::{ExecutionStatus, StepStatus};
use stepflow_core::{
    BlobId, DEFAULT_WAIT_TIMEOUT_SECS, FlowResult,
    workflow::{Flow, ValueRef, WorkflowOverrides},
};
use stepflow_dtos::{
    ItemResult, ResultOrder, RunDetails, RunFilters, RunStatus, RunSummary, StatusEvent,
    StatusEventKind,
};
use stepflow_plugin::StepflowEnvironment;
use stepflow_state::{
    BlobStoreExt as _, ExecutionJournalExt as _, JournalEvent, MetadataStoreExt as _,
    SequenceNumber,
};
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
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateRunRequest {
    /// The flow hash to execute
    pub flow_id: BlobId,
    /// Input data for the flow - always an array (one element per run)
    pub input: Vec<ValueRef>,
    /// Optional workflow overrides to apply before execution
    #[serde(default, skip_serializing_if = "WorkflowOverrides::is_empty")]
    #[serde_as(as = "DefaultOnNull")]
    pub overrides: WorkflowOverrides,
    /// Optional variables to provide for variable references in the workflow
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    #[serde_as(as = "DefaultOnNull")]
    pub variables: HashMap<String, ValueRef>,
    /// Maximum concurrency for batch execution (only used when input is an array)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrency: Option<usize>,
    /// If true, block until the run completes and return the result (200 OK).
    /// If false (default), return immediately with status Running (202 Accepted).
    #[serde(default)]
    #[serde_as(as = "DefaultOnNull")]
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

// =============================================================================
// Step Detail Endpoint
// =============================================================================

/// Path parameters for step detail endpoint
#[derive(Deserialize, schemars::JsonSchema)]
pub struct StepPath {
    /// Run ID (UUID)
    pub run_id: Uuid,
    /// Step ID (name)
    pub step_id: String,
}

/// Query parameters for step detail endpoint
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StepDetailQuery {
    /// Item index within the run (default: 0).
    #[serde(default)]
    pub item_index: Option<usize>,
    /// Minimum journal sequence number. If the step status in metadata has
    /// a `journal_seqno` less than this value, the server returns 409 Conflict
    /// indicating the read is not yet consistent with the SSE stream position.
    #[serde(default)]
    pub asof: Option<u64>,
}

pub fn get_step_detail_docs(op: TransformOperation<'_>) -> TransformOperation<'_> {
    op.id("getStepDetail")
        .summary("Get detailed status for a specific step")
        .description(
            "Returns the detailed status, component, and result for a specific step. \
             Use `asof` with a journal sequence number from the SSE stream to ensure \
             read-after-write consistency. Returns 409 if the metadata store has not \
             yet caught up to the requested sequence number.",
        )
        .tag("Run")
        .response_with::<404, ErrorResponse, _>(|res| res.description("Run or step not found"))
        .response_with::<409, ErrorResponse, _>(|res| {
            res.description("Metadata not yet consistent with requested sequence number")
        })
}

/// Get detailed status for a specific step
pub async fn get_step_detail(
    State(env): State<Arc<StepflowEnvironment>>,
    Path(StepPath { run_id, step_id }): Path<StepPath>,
    Query(query): Query<StepDetailQuery>,
) -> Result<Json<stepflow_dtos::StepStatusEntry>, ErrorResponse> {
    let metadata_store = env.metadata_store();
    let item_index = query.item_index.unwrap_or(0);

    let entry = metadata_store
        .get_step_status(run_id, item_index, &step_id)
        .await?
        .ok_or_else(|| {
            error_stack::report!(ServerError::InvalidRequest(format!(
                "Step '{step_id}' not found for run {run_id} item {item_index}"
            )))
        })?;

    // Check asof consistency
    if let Some(asof) = query.asof
        && entry.journal_seqno < asof
    {
        return Err(ErrorResponse {
            code: StatusCode::CONFLICT,
            message: format!(
                "Metadata not yet consistent: step has journal_seqno={}, requested asof={}",
                entry.journal_seqno, asof
            ),
            stack: vec![],
        });
    }

    Ok(Json(entry))
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

// =============================================================================
// SSE Status Stream
// =============================================================================

/// Query parameters for the status stream endpoint.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatusStreamQuery {
    /// Journal sequence number to start from (inclusive). If omitted,
    /// starts from the beginning of the run.
    #[serde(default)]
    pub since: Option<u64>,
    /// Include events from sub-runs (default: false, only root run events).
    #[serde(default)]
    pub include_sub_runs: Option<bool>,
    /// Comma-separated list of event types to include (e.g., "step_started,step_completed").
    /// If omitted, all event types are included.
    #[serde(default)]
    pub event_types: Option<String>,
    /// Include result payloads in step_completed and item_completed events (default: false).
    #[serde(default)]
    pub include_results: Option<bool>,
}

/// Wrapper for SSE responses that implements aide's `OperationOutput` for
/// OpenAPI generation with `text/event-stream` media type.
pub struct SseStream<S>(pub axum::response::sse::Sse<S>);

impl<S> axum::response::IntoResponse for SseStream<S>
where
    S: futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>
        + Send
        + 'static,
{
    fn into_response(self) -> axum::response::Response {
        self.0.into_response()
    }
}

impl<S: Send + 'static> aide::OperationOutput for SseStream<S> {
    type Inner = StatusEvent;

    fn operation_response(
        ctx: &mut aide::generate::GenContext,
        _operation: &mut aide::openapi::Operation,
    ) -> Option<aide::openapi::Response> {
        let json_schema = ctx.schema.subschema_for::<StatusEvent>();

        Some(aide::openapi::Response {
            description: "Server-Sent Events stream of execution events".into(),
            content: indexmap::IndexMap::from_iter([(
                "text/event-stream".into(),
                aide::openapi::MediaType {
                    schema: Some(aide::openapi::SchemaObject {
                        json_schema,
                        example: None,
                        external_docs: None,
                    }),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        })
    }
}

pub fn get_status_stream_docs(op: TransformOperation<'_>) -> TransformOperation<'_> {
    op.id("getRunStatusStream")
        .summary("Stream run execution events via SSE")
        .description(
            "Opens a Server-Sent Events (SSE) stream of execution events for a run. \
             Events are streamed in journal order with the SSE `id` set to the journal \
             sequence number. Use `since` to resume from a specific point. \
             The stream closes after a `run_completed` event for the requested run.",
        )
        .tag("Run")
        .response_with::<200, SseStream<
            futures::stream::Empty<Result<axum::response::sse::Event, std::convert::Infallible>>,
        >, _>(|res| res.description("Server-Sent Events stream of execution events"))
        .response_with::<404, ErrorResponse, _>(|res| res.description("Run not found"))
}

/// Stream run execution events via Server-Sent Events.
///
/// Returns an SSE stream of execution events for the given run. Each SSE event has:
/// - `id`: journal sequence number (for resumption via `since`)
/// - `event`: event type name (e.g., `step_started`, `step_completed`)
/// - `data`: JSON event payload
pub async fn get_status_stream(
    State(env): State<Arc<StepflowEnvironment>>,
    Path(RunPath { run_id }): Path<RunPath>,
    Query(query): Query<StatusStreamQuery>,
) -> Result<
    SseStream<
        impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>,
    >,
    ErrorResponse,
> {
    let metadata_store = env.metadata_store();

    // Verify the run exists and get its root_run_id + start sequence
    let run_details = metadata_store
        .get_run(run_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::ExecutionNotFound(run_id)))?;

    let root_run_id = run_details.summary.root_run_id;
    let include_sub_runs = query.include_sub_runs.unwrap_or(false);
    let include_results = query.include_results.unwrap_or(false);

    // Parse event type filter
    let event_type_filter: Option<Vec<String>> = query.event_types.map(|types| {
        types
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    });

    // Determine the starting sequence number
    let from_sequence = SequenceNumber::new(query.since.unwrap_or(0));

    let journal = env.execution_journal().clone();

    let stream = async_stream::stream! {
        let mut event_stream = journal.follow(root_run_id, from_sequence);

        while let Some(item) = event_stream.next().await {
            let entry = match item {
                Ok(entry) => entry,
                Err(_) => break, // Journal error — close the stream
            };
            let seq = entry.sequence;
            let timestamp = entry.timestamp;

            // Convert journal event to stream events
            let stream_events = journal_event_to_stream_events(&entry.event, include_results);
            let mut is_run_completed = false;

            for kind in stream_events {
                let event_run_id = kind.run_id();

                // Filter by run scope
                if !include_sub_runs && event_run_id != run_id {
                    continue;
                }

                // Filter by event type
                if let Some(ref filter) = event_type_filter
                    && !filter.iter().any(|f| f == kind.event_type())
                {
                    continue;
                }

                // Check if this is the target run completing
                if let StatusEventKind::RunCompleted { run_id: completed_id, .. } = &kind
                    && *completed_id == run_id
                {
                    is_run_completed = true;
                }

                // Wrap in StatusEvent with metadata
                let event = StatusEvent {
                    sequence_number: seq.value(),
                    timestamp,
                    kind,
                };

                // Serialize and yield
                if let Ok(data) = serde_json::to_string(&event) {
                    let sse_event = axum::response::sse::Event::default()
                        .id(seq.value().to_string())
                        .event(event.event_type())
                        .data(data);
                    yield Ok(sse_event);
                }
            }

            if is_run_completed {
                break;
            }
        }
    };

    Ok(SseStream(axum::response::sse::Sse::new(stream)))
}

/// Convert a `JournalEvent` into zero or more `StatusEvent`s.
///
/// Most journal events map 1:1, except `TasksStarted` which produces one
/// `StepStarted` per task across potentially multiple runs.
fn journal_event_to_stream_events(
    event: &JournalEvent,
    include_results: bool,
) -> Vec<StatusEventKind> {
    match event {
        JournalEvent::RootRunCreated {
            run_id,
            flow_id,
            inputs,
            ..
        } => vec![StatusEventKind::RunCreated {
            run_id: *run_id,
            flow_id: flow_id.clone(),
            item_count: inputs.len(),
        }],

        JournalEvent::RunInitialized {
            run_id,
            needed_steps,
        } => vec![StatusEventKind::RunInitialized {
            run_id: *run_id,
            // Convert step indices to step IDs would require the flow,
            // which we don't have here. Send indices for now; clients can
            // correlate with the flow definition.
            steps: Some(
                needed_steps
                    .iter()
                    .map(|is| is.step_indices.iter().map(|i| i.to_string()).collect())
                    .collect(),
            ),
        }],

        JournalEvent::TasksStarted { runs } => {
            let mut events = Vec::new();
            for run_tasks in runs {
                for task in &run_tasks.tasks {
                    events.push(StatusEventKind::StepStarted {
                        run_id: run_tasks.run_id,
                        item_index: task.item_index,
                        step_index: task.step_index,
                        step_id: None, // Would need flow to resolve
                        attempt: task.attempt,
                    });
                }
            }
            events
        }

        JournalEvent::TaskCompleted {
            run_id,
            item_index,
            step_index,
            result,
        } => {
            let status = match result {
                FlowResult::Failed(_) => StepStatus::Failed,
                _ => StepStatus::Completed,
            };
            vec![StatusEventKind::StepCompleted {
                run_id: *run_id,
                item_index: *item_index,
                step_index: *step_index,
                step_id: None,
                status,
                result: if include_results {
                    Some(result.clone())
                } else {
                    None
                },
            }]
        }

        JournalEvent::StepsUnblocked {
            run_id,
            item_index,
            step_indices,
        } => step_indices
            .iter()
            .map(|&step_index| StatusEventKind::StepReady {
                run_id: *run_id,
                item_index: *item_index,
                step_index,
                step_id: None,
            })
            .collect(),

        JournalEvent::ItemCompleted {
            run_id,
            item_index,
            result,
        } => vec![StatusEventKind::ItemCompleted {
            run_id: *run_id,
            item_index: *item_index,
            result: if include_results {
                Some(result.clone())
            } else {
                None
            },
        }],

        JournalEvent::RunCompleted { run_id, status } => vec![StatusEventKind::RunCompleted {
            run_id: *run_id,
            status: *status,
        }],

        JournalEvent::SubRunCreated {
            run_id,
            flow_id,
            inputs,
            parent_run_id,
            ..
        } => vec![StatusEventKind::SubRunCreated {
            run_id: *run_id,
            parent_run_id: *parent_run_id,
            flow_id: flow_id.clone(),
            item_count: inputs.len(),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_run_request_all_optional_null() {
        // All optional/defaulted fields as explicit null — simulates a Python client
        // calling model_dump() without exclude_none=True.
        let json = serde_json::json!({
            "flowId": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "input": [{"key": "value"}],
            "overrides": null,
            "variables": null,
            "maxConcurrency": null,
            "wait": null,
            "timeoutSecs": null,
        });
        let req: CreateRunRequest = serde_json::from_value(json).unwrap();
        assert!(req.overrides.is_empty());
        assert!(req.variables.is_empty());
        assert!(req.max_concurrency.is_none());
        assert!(!req.wait);
        assert!(req.timeout_secs.is_none());
    }
}
