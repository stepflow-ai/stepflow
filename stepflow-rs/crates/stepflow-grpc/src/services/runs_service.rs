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

use std::sync::Arc;

use futures::StreamExt as _;
use stepflow_core::workflow::{Flow, ValueRef};
use stepflow_core::{
    BlobId, BlobType, DEFAULT_WAIT_TIMEOUT_SECS, FlowResult, GetRunParams, SubmitRunParams,
};
use stepflow_dtos::RunFilters;
use stepflow_plugin::StepflowEnvironment;
use stepflow_state::{
    BlobStoreExt as _, ExecutionJournalExt as _, JournalEvent, MetadataStoreExt as _,
    SequenceNumber,
};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use crate::conversions::{
    chrono_to_timestamp, execution_status_to_proto, item_result_to_proto, item_stats_to_proto,
    proto_to_execution_status, proto_to_result_order,
    step_status_to_proto as step_status_to_proto_enum,
};
use crate::error as grpc_err;
use crate::proto::stepflow::v1::runs_service_server::RunsService;
use crate::proto::stepflow::v1::{
    self as proto, CancelRunRequest, CancelRunResponse, CreateRunRequest, CreateRunResponse,
    DeleteRunRequest, DeleteRunResponse, GetRunEventsRequest, GetRunFlowRequest,
    GetRunFlowResponse, GetRunItemsRequest, GetRunItemsResponse, GetRunRequest, GetRunResponse,
    GetRunStepsRequest, GetRunStepsResponse, GetStepDetailRequest, GetStepDetailResponse,
    ListRunsRequest, ListRunsResponse, StatusEvent,
};

/// gRPC implementation of [RunsService].
#[derive(Debug)]
pub struct RunsServiceImpl {
    env: Arc<StepflowEnvironment>,
}

impl RunsServiceImpl {
    pub fn new(env: Arc<StepflowEnvironment>) -> Self {
        Self { env }
    }
}

fn run_summary_to_proto(s: &stepflow_dtos::RunSummary) -> proto::RunSummary {
    proto::RunSummary {
        run_id: s.run_id.to_string(),
        flow_id: s.flow_id.to_string(),
        flow_name: s.flow_name.clone(),
        status: execution_status_to_proto(s.status),
        items: Some(item_stats_to_proto(&s.items)),
        created_at: Some(chrono_to_timestamp(s.created_at)),
        completed_at: s.completed_at.map(chrono_to_timestamp),
        root_run_id: s.root_run_id.to_string(),
        parent_run_id: s.parent_run_id.map(|id| id.to_string()),
        orchestrator_id: s.orchestrator_id.clone(),
        created_at_seqno: s.created_at_seqno,
        finished_at_seqno: s.finished_at_seqno,
    }
}

fn step_status_to_proto(entry: &stepflow_dtos::StepStatusEntry) -> proto::StepStatus {
    let (output, error_message, error_code) = match &entry.result {
        Some(FlowResult::Success(v)) => {
            let val: Option<prost_wkt_types::Value> = serde_json::to_value(v)
                .ok()
                .and_then(|j| serde_json::from_value(j).ok());
            (val, None, None)
        }
        Some(FlowResult::Failed(error)) => (
            None,
            Some(error.message.to_string()),
            Some(error.code.into()),
        ),
        _ => (None, None, None),
    };

    proto::StepStatus {
        step_id: entry.step_id.clone(),
        step_index: entry.step_index as u32,
        item_index: entry.item_index,
        status: step_status_to_proto_enum(entry.status),
        component: entry.component.clone(),
        output,
        error_message,
        error_code,
        started_at: None,
        completed_at: None,
        journal_seqno: entry.journal_seqno,
        updated_at: Some(chrono_to_timestamp(entry.updated_at)),
    }
}

fn parse_uuid(s: &str) -> Result<uuid::Uuid, Status> {
    s.parse()
        .map_err(|_| grpc_err::invalid_field("run_id", format!("invalid UUID: '{s}'")))
}

#[tonic::async_trait]
impl RunsService for RunsServiceImpl {
    async fn create_run(
        &self,
        request: Request<CreateRunRequest>,
    ) -> Result<Response<CreateRunResponse>, Status> {
        let req = request.into_inner();

        let flow_id = BlobId::new(req.flow_id.clone())
            .map_err(|_| grpc_err::invalid_field("flow_id", "invalid flow_id format"))?;

        let flow = self
            .env
            .blob_store()
            .get_flow(&flow_id)
            .await
            .map_err(|e| grpc_err::internal(format!("failed to get flow: {e}")))?
            .ok_or_else(|| grpc_err::not_found("flow", req.flow_id.clone()))?;

        let inputs: Vec<ValueRef> = req
            .input
            .into_iter()
            .map(|v| {
                let json: serde_json::Value = serde_json::to_value(&v)
                    .map_err(|e| grpc_err::internal(format!("failed to convert input: {e}")))?;
                Ok(ValueRef::new(json))
            })
            .collect::<Result<Vec<_>, Status>>()?;

        let variables = if req.variables.is_empty() {
            None
        } else {
            let vars = req
                .variables
                .into_iter()
                .map(|(k, v)| {
                    let json: serde_json::Value = serde_json::to_value(&v).map_err(|e| {
                        grpc_err::internal(format!("failed to convert variable: {e}"))
                    })?;
                    Ok((k, ValueRef::new(json)))
                })
                .collect::<Result<std::collections::HashMap<_, _>, Status>>()?;
            Some(vars)
        };

        let overrides = if let Some(overrides_struct) = req.overrides {
            let json: serde_json::Value = serde_json::to_value(&overrides_struct)
                .map_err(|e| grpc_err::internal(format!("failed to convert overrides: {e}")))?;
            serde_json::from_value(json).map_err(|e| {
                grpc_err::invalid_field("overrides", format!("invalid overrides: {e}"))
            })?
        } else {
            Default::default()
        };

        let params = SubmitRunParams {
            max_concurrency: req.max_concurrency.map(|v| v as usize),
            overrides,
            variables,
        };

        let run_status = stepflow_execution::submit_run(&self.env, flow, flow_id, inputs, params)
            .await
            .map_err(|e| grpc_err::internal(format!("failed to submit run: {e}")))?;

        let summary = run_summary_from_run_status(&run_status);

        if req.wait {
            let timeout = std::time::Duration::from_secs(
                req.timeout_secs.unwrap_or(DEFAULT_WAIT_TIMEOUT_SECS),
            );
            let _ = tokio::time::timeout(
                timeout,
                stepflow_execution::wait_for_completion(&self.env, run_status.run_id),
            )
            .await;

            let updated = stepflow_execution::get_run(
                &self.env,
                run_status.run_id,
                GetRunParams {
                    include_results: true,
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| grpc_err::internal(format!("failed to get run: {e}")))?;

            let summary = run_summary_from_run_status(&updated);
            let results = updated
                .results
                .unwrap_or_default()
                .iter()
                .map(item_result_to_proto)
                .collect();

            Ok(Response::new(CreateRunResponse {
                summary: Some(summary),
                results,
            }))
        } else {
            Ok(Response::new(CreateRunResponse {
                summary: Some(summary),
                results: vec![],
            }))
        }
    }

    async fn list_runs(
        &self,
        request: Request<ListRunsRequest>,
    ) -> Result<Response<ListRunsResponse>, Status> {
        let req = request.into_inner();

        // status field is an optional i32 enum; None or 0 (UNSPECIFIED) means no filter
        let status = req.status.and_then(proto_to_execution_status);
        let filters = RunFilters {
            status,
            flow_name: req.flow_name,
            root_run_id: req.root_run_id.and_then(|s| s.parse().ok()),
            parent_run_id: req.parent_run_id.and_then(|s| s.parse().ok()),
            roots_only: if req.roots_only { Some(true) } else { None },
            max_depth: req.max_depth,
            limit: req.limit.map(|l| l as usize),
            offset: req.offset.map(|o| o as usize),
            ..Default::default()
        };

        let runs = self
            .env
            .metadata_store()
            .list_runs(&filters)
            .await
            .map_err(|e| grpc_err::internal(format!("failed to list runs: {e}")))?;

        Ok(Response::new(ListRunsResponse {
            runs: runs.iter().map(run_summary_to_proto).collect(),
        }))
    }

    async fn get_run(
        &self,
        request: Request<GetRunRequest>,
    ) -> Result<Response<GetRunResponse>, Status> {
        let req = request.into_inner();
        let run_id = parse_uuid(&req.run_id)?;

        if req.wait {
            let timeout = std::time::Duration::from_secs(
                req.timeout_secs.unwrap_or(DEFAULT_WAIT_TIMEOUT_SECS),
            );
            let _ = tokio::time::timeout(
                timeout,
                stepflow_execution::wait_for_completion(&self.env, run_id),
            )
            .await;
        }

        let details = self
            .env
            .metadata_store()
            .get_run(run_id)
            .await
            .map_err(|e| grpc_err::internal(format!("failed to get run: {e}")))?
            .ok_or_else(|| grpc_err::not_found("run", &req.run_id))?;

        let step_statuses = self
            .env
            .metadata_store()
            .get_step_statuses(run_id, None, None)
            .await
            .map_err(|e| grpc_err::internal(format!("failed to get step statuses: {e}")))?;

        Ok(Response::new(GetRunResponse {
            summary: Some(run_summary_to_proto(&details.summary)),
            steps: step_statuses.iter().map(step_status_to_proto).collect(),
        }))
    }

    async fn delete_run(
        &self,
        request: Request<DeleteRunRequest>,
    ) -> Result<Response<DeleteRunResponse>, Status> {
        let req = request.into_inner();
        let run_id = parse_uuid(&req.run_id)?;

        let details = self
            .env
            .metadata_store()
            .get_run(run_id)
            .await
            .map_err(|e| grpc_err::internal(format!("failed to get run: {e}")))?
            .ok_or_else(|| grpc_err::not_found("run", &req.run_id))?;

        // Only allow deletion of non-running executions
        use stepflow_core::status::ExecutionStatus;
        match details.summary.status {
            ExecutionStatus::Running | ExecutionStatus::Paused => {
                return Err(grpc_err::failed_precondition(
                    "RUN_NOT_DELETABLE",
                    format!(
                        "run '{}' is still {:?} and cannot be deleted",
                        req.run_id, details.summary.status
                    ),
                ));
            }
            ExecutionStatus::Completed
            | ExecutionStatus::Failed
            | ExecutionStatus::Cancelled
            | ExecutionStatus::RecoveryFailed => {
                // Actual deletion not yet implemented — matches aide REST handler.
            }
        }

        Ok(Response::new(DeleteRunResponse {}))
    }

    async fn get_run_items(
        &self,
        request: Request<GetRunItemsRequest>,
    ) -> Result<Response<GetRunItemsResponse>, Status> {
        let req = request.into_inner();
        let run_id = parse_uuid(&req.run_id)?;

        let order = proto_to_result_order(req.result_order);

        let items = self
            .env
            .metadata_store()
            .get_item_results(run_id, order)
            .await
            .map_err(|e| grpc_err::internal(format!("failed to get items: {e}")))?;

        Ok(Response::new(GetRunItemsResponse {
            results: items.iter().map(item_result_to_proto).collect(),
        }))
    }

    async fn get_run_flow(
        &self,
        request: Request<GetRunFlowRequest>,
    ) -> Result<Response<GetRunFlowResponse>, Status> {
        let req = request.into_inner();
        let run_id = parse_uuid(&req.run_id)?;

        let details = self
            .env
            .metadata_store()
            .get_run(run_id)
            .await
            .map_err(|e| grpc_err::internal(format!("failed to get run: {e}")))?
            .ok_or_else(|| grpc_err::not_found("run", run_id.to_string()))?;

        let flow_id = &details.summary.flow_id;
        let raw = self
            .env
            .blob_store()
            .get_blob(flow_id)
            .await
            .map_err(|e| grpc_err::internal(format!("failed to get flow: {e}")))?
            .ok_or_else(|| grpc_err::not_found("flow", flow_id.to_string()))?;

        if raw.blob_type != BlobType::Flow {
            return Err(grpc_err::internal("blob is not a flow"));
        }

        let flow_value: prost_wkt_types::Struct = serde_json::from_slice(&raw.content)
            .map_err(|e| grpc_err::internal(format!("failed to deserialize flow: {e}")))?;

        Ok(Response::new(GetRunFlowResponse {
            flow: Some(flow_value),
            flow_id: flow_id.to_string(),
        }))
    }

    async fn get_run_steps(
        &self,
        request: Request<GetRunStepsRequest>,
    ) -> Result<Response<GetRunStepsResponse>, Status> {
        let req = request.into_inner();
        let run_id = parse_uuid(&req.run_id)?;
        let item_index = req.item_index.map(|i| i as usize);

        let steps = self
            .env
            .metadata_store()
            .get_step_statuses(run_id, item_index, None)
            .await
            .map_err(|e| grpc_err::internal(format!("failed to get step statuses: {e}")))?;

        Ok(Response::new(GetRunStepsResponse {
            steps: steps.iter().map(step_status_to_proto).collect(),
        }))
    }

    async fn get_step_detail(
        &self,
        request: Request<GetStepDetailRequest>,
    ) -> Result<Response<GetStepDetailResponse>, Status> {
        let req = request.into_inner();
        let run_id = parse_uuid(&req.run_id)?;
        let item_index = req.item_index.unwrap_or(0) as usize;

        let entry = self
            .env
            .metadata_store()
            .get_step_status(run_id, item_index, &req.step_id)
            .await
            .map_err(|e| grpc_err::internal(format!("failed to get step status: {e}")))?
            .ok_or_else(|| grpc_err::not_found("step", &req.step_id))?;

        // Check asof consistency: if the metadata store hasn't caught up to
        // the requested journal sequence number, signal the caller to retry.
        if let Some(asof) = req.asof
            && entry.journal_seqno < asof
        {
            return Err(grpc_err::failed_precondition(
                "METADATA_NOT_CONSISTENT",
                format!(
                    "metadata not yet consistent: step has journal_seqno={}, requested asof={}",
                    entry.journal_seqno, asof
                ),
            ));
        }

        Ok(Response::new(GetStepDetailResponse {
            step: Some(step_status_to_proto(&entry)),
        }))
    }

    type GetRunEventsStream = ReceiverStream<Result<StatusEvent, Status>>;

    async fn get_run_events(
        &self,
        request: Request<GetRunEventsRequest>,
    ) -> Result<Response<Self::GetRunEventsStream>, Status> {
        let req = request.into_inner();
        let run_id = parse_uuid(&req.run_id)?;

        // Verify the run exists and get root_run_id
        let run_details = self
            .env
            .metadata_store()
            .get_run(run_id)
            .await
            .map_err(|e| grpc_err::internal(format!("failed to get run: {e}")))?
            .ok_or_else(|| grpc_err::not_found("run", &req.run_id))?;

        let root_run_id = run_details.summary.root_run_id;
        let include_sub_runs = req.include_sub_runs;
        let include_results = req.include_results;

        // Event type filter: empty list means all types included
        let event_type_filter: Option<std::collections::HashSet<i32>> =
            if req.event_types.is_empty() {
                None
            } else {
                Some(req.event_types.into_iter().collect())
            };

        let from_sequence = SequenceNumber::new(req.since.unwrap_or(0));

        // Load the flow for step index → step ID resolution
        let flow = self
            .env
            .blob_store()
            .get_flow(&run_details.summary.flow_id)
            .await
            .ok()
            .flatten();

        let journal = self.env.execution_journal().clone();

        let (tx, rx) = tokio::sync::mpsc::channel(128);

        tokio::spawn(async move {
            let mut event_stream = journal.follow(root_run_id, from_sequence);

            while let Some(item) = event_stream.next().await {
                let entry = match item {
                    Ok(entry) => entry,
                    Err(_) => break,
                };
                let seq = entry.sequence;
                let timestamp = entry.timestamp;

                let proto_events =
                    journal_event_to_proto(&entry.event, include_results, flow.as_deref());
                let mut is_run_completed = false;

                for (event_run_id, proto_event) in proto_events {
                    // Filter by run scope
                    if !include_sub_runs && event_run_id != run_id {
                        continue;
                    }

                    // Check completion before applying filters
                    if let proto::status_event::Event::RunCompleted(ref rc) = proto_event
                        && rc.run_id.parse::<uuid::Uuid>() == Ok(run_id)
                    {
                        is_run_completed = true;
                    }

                    // Filter by event type
                    if let Some(ref filter) = event_type_filter
                        && !filter.contains(&proto_event_type_enum(&proto_event))
                    {
                        continue;
                    }

                    let status_event = StatusEvent {
                        sequence_number: seq.value(),
                        timestamp: Some(chrono_to_timestamp(timestamp)),
                        event: Some(proto_event),
                    };

                    if tx.send(Ok(status_event)).await.is_err() {
                        return; // Client disconnected
                    }
                }

                if is_run_completed {
                    break;
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn cancel_run(
        &self,
        request: Request<CancelRunRequest>,
    ) -> Result<Response<CancelRunResponse>, Status> {
        let req = request.into_inner();
        let run_id = parse_uuid(&req.run_id)?;

        // Update status to cancelled
        self.env
            .metadata_store()
            .update_run_status(
                run_id,
                stepflow_core::status::ExecutionStatus::Cancelled,
                None,
            )
            .await
            .map_err(|e| grpc_err::internal(format!("failed to cancel run: {e}")))?;

        let details = self
            .env
            .metadata_store()
            .get_run(run_id)
            .await
            .map_err(|e| grpc_err::internal(format!("failed to get run: {e}")))?
            .ok_or_else(|| grpc_err::not_found("run", run_id.to_string()))?;

        Ok(Response::new(CancelRunResponse {
            summary: Some(run_summary_to_proto(&details.summary)),
        }))
    }
}

// --- Journal event → proto StatusEvent conversion ---

/// Resolve a step index to a step ID using the flow, if available.
fn resolve_step_id(flow: Option<&Flow>, step_index: usize) -> Option<String> {
    flow.and_then(|f| f.steps().get(step_index))
        .map(|s| s.id.clone())
}

/// Convert a `JournalEvent` into proto status event variants.
/// Returns (run_id, proto event) pairs for filtering.
fn journal_event_to_proto(
    event: &JournalEvent,
    include_results: bool,
    flow: Option<&Flow>,
) -> Vec<(uuid::Uuid, proto::status_event::Event)> {
    match event {
        JournalEvent::RootRunCreated {
            run_id,
            flow_id,
            inputs,
            ..
        } => vec![(
            *run_id,
            proto::status_event::Event::RunCreated(proto::RunCreatedEvent {
                run_id: run_id.to_string(),
                flow_id: flow_id.to_string(),
                item_count: inputs.len() as u32,
            }),
        )],

        JournalEvent::StepsNeeded {
            run_id,
            item_index,
            step_indices,
        } => {
            let step_ids: Vec<String> = step_indices
                .iter()
                .filter_map(|&idx| resolve_step_id(flow, idx))
                .collect();
            vec![(
                *run_id,
                proto::status_event::Event::StepsNeeded(proto::StepsNeededEvent {
                    run_id: run_id.to_string(),
                    item_index: *item_index,
                    step_ids,
                }),
            )]
        }

        JournalEvent::TasksStarted { runs } => {
            let mut events = Vec::new();
            for run_tasks in runs {
                for task in &run_tasks.tasks {
                    events.push((
                        run_tasks.run_id,
                        proto::status_event::Event::StepStarted(proto::StepStartedEvent {
                            run_id: run_tasks.run_id.to_string(),
                            item_index: task.item_index,
                            step_index: task.step_index as u32,
                            step_id: resolve_step_id(flow, task.step_index),
                            attempt: task.attempt,
                        }),
                    ));
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
                FlowResult::Failed(_) => proto::ExecutionStatus::Failed as i32,
                _ => proto::ExecutionStatus::Completed as i32,
            };
            let result_value = if include_results {
                match result {
                    FlowResult::Success(v) => serde_json::to_value(v)
                        .ok()
                        .and_then(|j| serde_json::from_value(j).ok()),
                    _ => None,
                }
            } else {
                None
            };
            vec![(
                *run_id,
                proto::status_event::Event::StepCompleted(proto::StepCompletedEvent {
                    run_id: run_id.to_string(),
                    item_index: *item_index,
                    step_index: *step_index as u32,
                    step_id: resolve_step_id(flow, *step_index),
                    status,
                    result: result_value,
                }),
            )]
        }

        JournalEvent::StepsUnblocked {
            run_id,
            item_index,
            step_indices,
        } => step_indices
            .iter()
            .map(|&step_index| {
                (
                    *run_id,
                    proto::status_event::Event::StepReady(proto::StepReadyEvent {
                        run_id: run_id.to_string(),
                        item_index: *item_index,
                        step_index: step_index as u32,
                        step_id: resolve_step_id(flow, step_index),
                    }),
                )
            })
            .collect(),

        JournalEvent::ItemCompleted {
            run_id,
            item_index,
            result,
        } => {
            let result_value = if include_results {
                match result {
                    FlowResult::Success(v) => serde_json::to_value(v)
                        .ok()
                        .and_then(|j| serde_json::from_value(j).ok()),
                    _ => None,
                }
            } else {
                None
            };
            vec![(
                *run_id,
                proto::status_event::Event::ItemCompleted(proto::ItemCompletedEvent {
                    run_id: run_id.to_string(),
                    item_index: *item_index,
                    result: result_value,
                }),
            )]
        }

        JournalEvent::RunCompleted { run_id, status } => vec![(
            *run_id,
            proto::status_event::Event::RunCompleted(proto::RunCompletedEvent {
                run_id: run_id.to_string(),
                status: execution_status_to_proto(*status),
            }),
        )],

        JournalEvent::SubRunCreated {
            run_id,
            flow_id,
            inputs,
            parent_run_id,
            ..
        } => vec![(
            *run_id,
            proto::status_event::Event::SubRunCreated(proto::SubRunCreatedEvent {
                run_id: run_id.to_string(),
                parent_run_id: parent_run_id.to_string(),
                flow_id: flow_id.to_string(),
                item_count: inputs.len() as u32,
            }),
        )],
    }
}

/// Get the `StatusEventType` enum value for a proto status event (for filtering).
fn proto_event_type_enum(event: &proto::status_event::Event) -> i32 {
    match event {
        proto::status_event::Event::RunCreated(_) => proto::StatusEventType::RunCreated as i32,
        proto::status_event::Event::StepStarted(_) => proto::StatusEventType::StepStarted as i32,
        proto::status_event::Event::StepCompleted(_) => {
            proto::StatusEventType::StepCompleted as i32
        }
        proto::status_event::Event::StepReady(_) => proto::StatusEventType::StepReady as i32,
        proto::status_event::Event::ItemCompleted(_) => {
            proto::StatusEventType::ItemCompleted as i32
        }
        proto::status_event::Event::RunCompleted(_) => proto::StatusEventType::RunCompleted as i32,
        proto::status_event::Event::SubRunCreated(_) => {
            proto::StatusEventType::SubRunCreated as i32
        }
        proto::status_event::Event::StepsNeeded(_) => proto::StatusEventType::StepsNeeded as i32,
    }
}

fn run_summary_from_run_status(s: &stepflow_dtos::RunStatus) -> proto::RunSummary {
    proto::RunSummary {
        run_id: s.run_id.to_string(),
        flow_id: s.flow_id.to_string(),
        flow_name: s.flow_name.clone(),
        status: execution_status_to_proto(s.status),
        items: Some(item_stats_to_proto(&s.items)),
        created_at: Some(chrono_to_timestamp(s.created_at)),
        completed_at: s.completed_at.map(chrono_to_timestamp),
        root_run_id: s.root_run_id.to_string(),
        parent_run_id: s.parent_run_id.map(|id| id.to_string()),
        orchestrator_id: None, // RunStatus doesn't carry orchestrator_id
        created_at_seqno: s.created_at_seqno,
        finished_at_seqno: None, // RunStatus doesn't carry finished_at_seqno
    }
}
