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

use stepflow_core::error_code::ErrorCode;
use stepflow_core::workflow::ValueRef;
use stepflow_core::{
    BlobId, DEFAULT_WAIT_TIMEOUT_SECS, FlowError, FlowResult, GetRunParams, SubmitRunParams,
};
use stepflow_plugin::StepflowEnvironment;
use stepflow_state::BlobStoreExt as _;
use tonic::{Request, Response, Status};

use crate::conversions::{
    chrono_to_timestamp, execution_status_to_proto, item_result_to_proto, item_stats_to_proto,
    proto_to_result_order,
};
use crate::error as grpc_err;

use crate::pending_tasks::PendingTasks;
use crate::proto::stepflow::v1::orchestrator_service_server::OrchestratorService;
use crate::proto::stepflow::v1::{
    CompleteTaskRequest, CompleteTaskResponse, OrchestratorGetRunRequest, OrchestratorRunStatus,
    OrchestratorSubmitRunRequest, StartTaskRequest, StartTaskResponse, TaskHeartbeatRequest,
    TaskHeartbeatResponse,
};

/// gRPC implementation of `OrchestratorService`.
///
/// This service is called by workers during component execution to submit
/// sub-runs, query run status, and report task completions. Workers reach
/// it via the `orchestrator_service_url` from `TaskContext`.
#[derive(Debug)]
pub struct OrchestratorServiceImpl {
    env: Arc<StepflowEnvironment>,
    task_registry: Arc<PendingTasks>,
}

impl OrchestratorServiceImpl {
    pub fn new(env: Arc<StepflowEnvironment>, task_registry: Arc<PendingTasks>) -> Self {
        Self { env, task_registry }
    }
}

#[tonic::async_trait]
impl OrchestratorService for OrchestratorServiceImpl {
    async fn submit_run(
        &self,
        request: Request<OrchestratorSubmitRunRequest>,
    ) -> Result<Response<OrchestratorRunStatus>, Status> {
        let req = request.into_inner();

        let run_req = req
            .run_request
            .ok_or_else(|| grpc_err::invalid_field("run_request", "run_request is required"))?;

        let flow_id = BlobId::new(run_req.flow_id.clone())
            .map_err(|_| grpc_err::invalid_field("flow_id", "invalid flow_id format"))?;

        let flow = self
            .env
            .blob_store()
            .get_flow(&flow_id)
            .await
            .map_err(|e| grpc_err::internal(format!("failed to get flow: {e}")))?
            .ok_or_else(|| grpc_err::not_found("flow", run_req.flow_id.clone()))?;

        let inputs: Vec<ValueRef> = run_req
            .input
            .into_iter()
            .map(|v| ValueRef::new(crate::conversions::proto_value_to_json(&v)))
            .collect();

        let overrides = if let Some(overrides_struct) = run_req.overrides {
            let json = crate::conversions::proto_value_to_json(&prost_wkt_types::Value {
                kind: Some(prost_wkt_types::value::Kind::StructValue(overrides_struct)),
            });
            serde_json::from_value(json).map_err(|e| {
                grpc_err::invalid_field("overrides", format!("invalid overrides: {e}"))
            })?
        } else {
            Default::default()
        };

        let variables = if run_req.variables.is_empty() {
            None
        } else {
            let vars = run_req
                .variables
                .into_iter()
                .map(|(k, v)| (k, ValueRef::new(crate::conversions::proto_value_to_json(&v))))
                .collect::<std::collections::HashMap<_, _>>();
            Some(vars)
        };

        let params = SubmitRunParams {
            max_concurrency: run_req.max_concurrency.map(|v| v as usize),
            overrides,
            variables,
        };

        let run_status = stepflow_execution::submit_run(&self.env, flow, flow_id, inputs, params)
            .await
            .map_err(|e| grpc_err::internal(format!("failed to submit run: {e}")))?;

        let mut response = run_status_to_proto(&run_status);

        if run_req.wait {
            let timeout = std::time::Duration::from_secs(
                run_req.timeout_secs.unwrap_or(DEFAULT_WAIT_TIMEOUT_SECS),
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

            response = run_status_to_proto(&updated);
        }

        Ok(Response::new(response))
    }

    async fn get_run(
        &self,
        request: Request<OrchestratorGetRunRequest>,
    ) -> Result<Response<OrchestratorRunStatus>, Status> {
        let req = request.into_inner();

        let run_id: uuid::Uuid = req
            .run_id
            .parse()
            .map_err(|_| grpc_err::invalid_field("run_id", "invalid run_id format"))?;

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

        let result_order = proto_to_result_order(req.result_order);

        let run_status = stepflow_execution::get_run(
            &self.env,
            run_id,
            GetRunParams {
                include_results: req.include_results,
                result_order,
            },
        )
        .await
        .map_err(|e| grpc_err::internal(format!("failed to get run: {e}")))?;

        Ok(Response::new(run_status_to_proto(&run_status)))
    }

    async fn complete_task(
        &self,
        request: Request<CompleteTaskRequest>,
    ) -> Result<Response<CompleteTaskResponse>, Status> {
        use crate::proto::stepflow::v1::complete_task_request::Result as TaskResult;

        let req = request.into_inner();

        let result = match req.result {
            Some(TaskResult::Response(response)) => {
                let output = response.output.ok_or_else(|| {
                    grpc_err::invalid_field(
                        "response.output",
                        "response present but output is missing",
                    )
                })?;
                let json = crate::conversions::proto_value_to_json(&output);
                FlowResult::Success(ValueRef::new(json))
            }
            Some(TaskResult::Error(task_error)) => {
                // Map proto TaskErrorCode to Stepflow ErrorCode constants.
                // This preserves fine-grained error categorization and retryability
                // semantics, matching the behavior of the JSON-RPC transport.
                let error_code = match task_error.code {
                    1 => ErrorCode::WORKER_ERROR,                   // INTERNAL
                    2 => ErrorCode::TRANSPORT_ERROR,                // TIMEOUT (retriable)
                    3 => ErrorCode::COMPONENT_BAD_REQUEST,          // INVALID_INPUT
                    4 => ErrorCode::COMPONENT_EXECUTION_FAILED,     // COMPONENT_FAILED
                    5 => ErrorCode::WORKER_ERROR,                   // CANCELLED
                    6 => ErrorCode::COMPONENT_RESOURCE_UNAVAILABLE, // UNAVAILABLE
                    7 => ErrorCode::COMPONENT_NOT_FOUND,            // COMPONENT_NOT_FOUND
                    _ => ErrorCode::COMPONENT_EXECUTION_FAILED,     // UNSPECIFIED/unknown
                };

                // Convert optional structured error data from proto Struct
                let data = task_error.data.map(|s| {
                    let json = crate::conversions::proto_value_to_json(
                        &prost_wkt_types::Value {
                            kind: Some(prost_wkt_types::value::Kind::StructValue(s)),
                        },
                    );
                    ValueRef::new(json)
                });

                FlowResult::Failed(FlowError {
                    code: error_code,
                    message: task_error.message.into(),
                    data,
                })
            }
            None => {
                return Err(grpc_err::invalid_argument(
                    "either response or error must be provided",
                ));
            }
        };

        if self.task_registry.complete(&req.task_id, result) {
            Ok(Response::new(CompleteTaskResponse {}))
        } else {
            Err(grpc_err::not_found("task", &req.task_id))
        }
    }

    async fn start_task(
        &self,
        request: Request<StartTaskRequest>,
    ) -> Result<Response<StartTaskResponse>, Status> {
        let req = request.into_inner();
        match self.task_registry.start_task(&req.task_id) {
            Some(timed_out) => Ok(Response::new(StartTaskResponse { timed_out })),
            None => Err(grpc_err::not_found("task", &req.task_id)),
        }
    }

    async fn task_heartbeat(
        &self,
        request: Request<TaskHeartbeatRequest>,
    ) -> Result<Response<TaskHeartbeatResponse>, Status> {
        let req = request.into_inner();
        match self.task_registry.heartbeat(&req.task_id) {
            Some(should_cancel) => Ok(Response::new(TaskHeartbeatResponse { should_cancel })),
            None => Err(grpc_err::not_found("task", &req.task_id)),
        }
    }
}

fn run_status_to_proto(s: &stepflow_dtos::RunStatus) -> OrchestratorRunStatus {
    OrchestratorRunStatus {
        run_id: s.run_id.to_string(),
        flow_id: s.flow_id.to_string(),
        flow_name: s.flow_name.clone(),
        status: execution_status_to_proto(s.status),
        items: Some(item_stats_to_proto(&s.items)),
        created_at: Some(chrono_to_timestamp(s.created_at)),
        completed_at: s.completed_at.map(chrono_to_timestamp),
        results: s
            .results
            .as_ref()
            .map(|results| results.iter().map(item_result_to_proto).collect())
            .unwrap_or_default(),
        root_run_id: s.root_run_id.to_string(),
        parent_run_id: s.parent_run_id.map(|id| id.to_string()),
        created_at_seqno: s.created_at_seqno,
    }
}
