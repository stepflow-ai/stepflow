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
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_core::{BlobId, workflow::ValueRef};
use stepflow_execution::StepflowExecutor;
use stepflow_state::{BatchDetails, BatchFilters, BatchMetadata, BatchStatus, RunSummary};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{ErrorResponse, ServerError};

/// Request to create a batch execution
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateBatchRequest {
    /// The flow hash to execute
    pub flow_id: BlobId,
    /// Array of input data for each run in the batch
    pub inputs: Vec<ValueRef>,
    /// Maximum number of concurrent executions (defaults to number of inputs if not specified)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrency: Option<usize>,
}

/// Response for create batch operations
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateBatchResponse {
    /// The batch ID
    pub batch_id: Uuid,
    /// Total number of runs in the batch
    pub total_runs: usize,
    /// The batch status
    pub status: BatchStatus,
}

/// Response for get batch operations (metadata + statistics)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetBatchResponse {
    #[serde(flatten)]
    pub details: BatchDetails,
}

/// Response for listing batches
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListBatchesResponse {
    /// List of batch metadata
    pub batches: Vec<BatchMetadata>,
}

/// Run information with batch context
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchRunInfo {
    #[serde(flatten)]
    pub run: RunSummary,
    /// Position in the batch input array
    pub batch_input_index: usize,
}

/// Response for listing batch runs
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListBatchRunsResponse {
    /// List of runs with batch context
    pub runs: Vec<BatchRunInfo>,
}

/// Response for cancel batch operations
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CancelBatchResponse {
    /// The batch ID
    pub batch_id: Uuid,
    /// The updated batch status
    pub status: BatchStatus,
}

/// Output information for a single run in a batch
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchOutputInfo {
    /// Position in the batch input array
    pub batch_input_index: usize,
    /// The execution status
    pub status: stepflow_core::status::ExecutionStatus,
    /// The flow result (if completed)
    pub result: Option<stepflow_core::FlowResult>,
}

/// Response for listing batch outputs
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListBatchOutputsResponse {
    /// List of outputs sorted by batch_input_index
    pub outputs: Vec<BatchOutputInfo>,
}

/// Query parameters for listing batches
#[derive(Debug, Clone, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListBatchesQuery {
    /// Filter by batch status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<BatchStatus>,
    /// Filter by flow name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow_name: Option<String>,
    /// Maximum number of results to return
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    /// Number of results to skip
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
}

/// Create and execute a batch of flows
#[utoipa::path(
    post,
    path = "/batches",
    request_body = CreateBatchRequest,
    responses(
        (status = 200, description = "Batch created successfully and execution started", body = CreateBatchResponse),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Flow not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::BATCH_TAG,
)]
pub async fn create_batch(
    State(executor): State<Arc<StepflowExecutor>>,
    Json(req): Json<CreateBatchRequest>,
) -> Result<Json<CreateBatchResponse>, ErrorResponse> {
    let batch_id = Uuid::new_v4();
    let state_store = executor.state_store();

    // Get the flow from the state store
    let flow = state_store
        .get_flow(&req.flow_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::WorkflowNotFound(req.flow_id.clone())))?;

    let total_runs = req.inputs.len();
    let max_concurrency = req.max_concurrency.unwrap_or(total_runs);

    // Create batch record
    state_store
        .create_batch(batch_id, req.flow_id.clone(), flow.name(), total_runs)
        .await?;

    // Create run records and collect (run_id, input, index) tuples
    let mut run_inputs = Vec::with_capacity(total_runs);
    for (idx, input) in req.inputs.into_iter().enumerate() {
        let run_id = Uuid::new_v4();

        // Create run record
        state_store
            .create_run(
                run_id,
                req.flow_id.clone(),
                flow.name(),
                None,  // No flow label for batch execution
                false, // Batch runs are not in debug mode
                input.clone(),
            )
            .await?;

        // Link run to batch
        state_store.add_run_to_batch(batch_id, run_id, idx).await?;

        run_inputs.push((run_id, input, idx));
    }

    // Spawn background task for batch execution
    let executor_clone = executor.clone();
    let flow_clone = flow.clone();
    let flow_id = req.flow_id.clone();

    tokio::spawn(async move {
        let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrency));
        let mut tasks = vec![];

        for (run_id, input, _idx) in run_inputs {
            let permit = match semaphore.clone().acquire_owned().await {
                Ok(permit) => permit,
                Err(_) => {
                    tracing::error!("Semaphore closed, aborting batch execution");
                    break;
                }
            };
            let executor_ref = executor_clone.clone();
            let flow_ref = flow_clone.clone();
            let flow_id_ref = flow_id.clone();

            let task = tokio::spawn(async move {
                let _permit = permit; // Hold permit during execution

                // Execute flow using Context trait method
                use stepflow_plugin::Context as _;

                match executor_ref.submit_flow(flow_ref, flow_id_ref, input).await {
                    Ok(submitted_run_id) => {
                        // Wait for the result
                        match executor_ref.flow_result(submitted_run_id).await {
                            Ok(flow_result) => {
                                // Update run status based on result
                                let state_store = executor_ref.state_store();
                                let status = match &flow_result {
                                    stepflow_core::FlowResult::Success(_) => {
                                        stepflow_core::status::ExecutionStatus::Completed
                                    }
                                    stepflow_core::FlowResult::Failed(_)
                                    | stepflow_core::FlowResult::Skipped { .. } => {
                                        stepflow_core::status::ExecutionStatus::Failed
                                    }
                                };
                                let result_ref = match &flow_result {
                                    stepflow_core::FlowResult::Success(r) => Some(r.clone()),
                                    _ => None,
                                };
                                let _ = state_store
                                    .update_run_status(run_id, status, result_ref)
                                    .await;
                            }
                            Err(e) => {
                                tracing::error!("Batch run {run_id} failed to get result: {e:?}");
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Batch run {run_id} failed to submit: {e:?}");
                    }
                }
            });

            tasks.push(task);
        }

        // Wait for all tasks to complete
        for task in tasks {
            let _ = task.await;
        }

        tracing::info!("Batch {batch_id} execution completed");
    });

    Ok(Json(CreateBatchResponse {
        batch_id,
        total_runs,
        status: BatchStatus::Running,
    }))
}

/// Get batch details by ID
#[utoipa::path(
    get,
    path = "/batches/{batch_id}",
    params(
        ("batch_id" = Uuid, Path, description = "Batch ID (UUID)")
    ),
    responses(
        (status = 200, description = "Batch details retrieved successfully", body = GetBatchResponse),
        (status = 400, description = "Invalid batch ID format"),
        (status = 404, description = "Batch not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::BATCH_TAG,
)]
pub async fn get_batch(
    State(executor): State<Arc<StepflowExecutor>>,
    Path(batch_id): Path<Uuid>,
) -> Result<Json<GetBatchResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    // Get batch metadata
    let metadata = state_store
        .get_batch(batch_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::BatchNotFound(batch_id)))?;

    // Get batch statistics
    let statistics = state_store.get_batch_statistics(batch_id).await?;

    // Calculate completion time if all runs are complete
    let completed_at = if metadata.status == BatchStatus::Cancelled
        || (statistics.running_runs == 0 && statistics.paused_runs == 0)
    {
        Some(chrono::Utc::now())
    } else {
        None
    };

    Ok(Json(GetBatchResponse {
        details: BatchDetails {
            metadata,
            statistics,
            completed_at,
        },
    }))
}

/// List runs in a batch
#[utoipa::path(
    get,
    path = "/batches/{batch_id}/runs",
    params(
        ("batch_id" = Uuid, Path, description = "Batch ID (UUID)")
    ),
    responses(
        (status = 200, description = "Batch runs listed successfully", body = ListBatchRunsResponse),
        (status = 400, description = "Invalid batch ID format"),
        (status = 404, description = "Batch not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::BATCH_TAG,
)]
pub async fn list_batch_runs(
    State(executor): State<Arc<StepflowExecutor>>,
    Path(batch_id): Path<Uuid>,
) -> Result<Json<ListBatchRunsResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    // Verify batch exists
    let _batch = state_store
        .get_batch(batch_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::BatchNotFound(batch_id)))?;

    // Get runs for this batch
    let filters = stepflow_state::RunFilters::default();
    let batch_runs = state_store.list_batch_runs(batch_id, &filters).await?;

    let runs = batch_runs
        .into_iter()
        .map(|(run, batch_input_index)| BatchRunInfo {
            run,
            batch_input_index,
        })
        .collect();

    Ok(Json(ListBatchRunsResponse { runs }))
}

/// List batches with optional filtering
#[utoipa::path(
    get,
    path = "/batches",
    params(
        ListBatchesQuery
    ),
    responses(
        (status = 200, description = "Batches listed successfully", body = ListBatchesResponse),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::BATCH_TAG,
)]
pub async fn list_batches(
    State(executor): State<Arc<StepflowExecutor>>,
    Query(query): Query<ListBatchesQuery>,
) -> Result<Json<ListBatchesResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    let filters = BatchFilters {
        status: query.status,
        flow_name: query.flow_name,
        limit: query.limit,
        offset: query.offset,
    };

    let batches = state_store.list_batches(&filters).await?;

    Ok(Json(ListBatchesResponse { batches }))
}

/// Get batch outputs (results for all runs)
#[utoipa::path(
    get,
    path = "/batches/{batch_id}/outputs",
    params(
        ("batch_id" = Uuid, Path, description = "Batch ID (UUID)")
    ),
    responses(
        (status = 200, description = "Batch outputs retrieved successfully", body = ListBatchOutputsResponse),
        (status = 400, description = "Invalid batch ID format"),
        (status = 404, description = "Batch not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::BATCH_TAG,
)]
pub async fn get_batch_outputs(
    State(executor): State<Arc<StepflowExecutor>>,
    Path(batch_id): Path<Uuid>,
) -> Result<Json<ListBatchOutputsResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    // Verify batch exists
    let _batch = state_store
        .get_batch(batch_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::BatchNotFound(batch_id)))?;

    // Get runs for this batch
    let filters = stepflow_state::RunFilters::default();
    let batch_runs = state_store.list_batch_runs(batch_id, &filters).await?;

    // Fetch run details for each to get results
    let mut outputs = Vec::new();
    for (run_summary, batch_input_index) in batch_runs {
        let run_details = state_store.get_run(run_summary.run_id).await?;

        let result = run_details.and_then(|details| details.result);

        outputs.push(BatchOutputInfo {
            batch_input_index,
            status: run_summary.status,
            result,
        });
    }

    // Sort by batch_input_index to maintain input order
    outputs.sort_by_key(|o| o.batch_input_index);

    Ok(Json(ListBatchOutputsResponse { outputs }))
}

/// Cancel a batch execution
#[utoipa::path(
    post,
    path = "/batches/{batch_id}/cancel",
    params(
        ("batch_id" = Uuid, Path, description = "Batch ID (UUID)")
    ),
    responses(
        (status = 200, description = "Batch cancelled successfully", body = CancelBatchResponse),
        (status = 400, description = "Invalid batch ID format"),
        (status = 404, description = "Batch not found"),
        (status = 409, description = "Batch already completed"),
        (status = 500, description = "Internal server error")
    ),
    tag = crate::api::BATCH_TAG,
)]
pub async fn cancel_batch(
    State(executor): State<Arc<StepflowExecutor>>,
    Path(batch_id): Path<Uuid>,
) -> Result<Json<CancelBatchResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    // Get batch to check current status
    let batch = state_store
        .get_batch(batch_id)
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::BatchNotFound(batch_id)))?;

    // Check if batch can be cancelled
    match batch.status {
        BatchStatus::Cancelled => {
            return Err(error_stack::report!(ServerError::BatchNotCancellable {
                batch_id,
                status: batch.status
            })
            .into());
        }
        BatchStatus::Running => {
            // Update batch status to cancelled
            state_store
                .update_batch_status(batch_id, BatchStatus::Cancelled)
                .await?;

            // TODO: Cancel all running runs in this batch
            // For now, we just mark the batch as cancelled
        }
    }

    Ok(Json(CancelBatchResponse {
        batch_id,
        status: BatchStatus::Cancelled,
    }))
}
