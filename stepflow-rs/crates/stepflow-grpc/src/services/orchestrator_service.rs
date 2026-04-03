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

use stepflow_core::TaskErrorCode;
use stepflow_core::workflow::ValueRef;
use stepflow_core::{
    BlobId, DEFAULT_WAIT_TIMEOUT_SECS, FlowError, FlowResult, GetRunParams, SubmitRunParams,
};
use stepflow_plugin::StepflowEnvironment;
use stepflow_state::{ActiveExecutionsExt as _, ActiveRecoveriesExt as _, BlobStoreExt as _};
use tonic::{Request, Response, Status};

use crate::conversions::{
    chrono_to_timestamp, execution_status_to_proto, item_result_to_proto, item_stats_to_proto,
    proto_to_result_order,
};
use crate::error as grpc_err;

use crate::pending_tasks::HeartbeatResult;
use crate::pending_tasks::PendingTasks;
use crate::proto::stepflow::v1::orchestrator_service_server::OrchestratorService;
use crate::proto::stepflow::v1::{
    CompleteTaskRequest, CompleteTaskResponse, OrchestratorGetRunRequest, OrchestratorRunStatus,
    OrchestratorSubmitRunRequest, TaskHeartbeatRequest, TaskHeartbeatResponse,
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

        // Validate that this orchestrator owns the root run (if provided).
        self.check_run_ownership(&req.root_run_id)?;

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
                .map(|(k, v)| {
                    (
                        k,
                        ValueRef::new(crate::conversions::proto_value_to_json(&v)),
                    )
                })
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

        // Validate that this orchestrator owns the root run (if provided).
        self.check_run_ownership(&req.root_run_id)?;

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
                // Convert proto TaskErrorCode i32 to the TaskErrorCode enum.
                let error_code =
                    TaskErrorCode::try_from(task_error.code).unwrap_or(TaskErrorCode::WorkerError);

                // Convert optional structured error data from proto Struct
                let data = task_error.data.map(|s| {
                    let json = crate::conversions::proto_value_to_json(&prost_wkt_types::Value {
                        kind: Some(prost_wkt_types::value::Kind::StructValue(s)),
                    });
                    ValueRef::new(json)
                });

                FlowResult::Failed(FlowError {
                    code: error_code,
                    message: task_error.message.into(),
                    data,
                })
            }
            Some(TaskResult::ListComponents(list)) => {
                // Convert the ListComponentsResult into a JSON value so it can
                // flow through the existing task completion mechanism. The NATS
                // plugin (or other queue transports) will parse this when the
                // discovery task completes.
                let components: Vec<serde_json::Value> = list
                    .components
                    .into_iter()
                    .map(|c| {
                        serde_json::json!({
                            "component_id": c.component_id,
                            "path": c.path,
                            "description": c.description,
                            "input_schema": c.input_schema,
                            "output_schema": c.output_schema,
                        })
                    })
                    .collect();
                FlowResult::Success(ValueRef::new(serde_json::json!({
                    "components": components,
                })))
            }
            None => {
                return Err(grpc_err::invalid_argument(
                    "either response, error, or list_components must be provided",
                ));
            }
        };

        if self.task_registry.complete(&req.task_id, result) {
            Ok(Response::new(CompleteTaskResponse {}))
        } else if self.is_run_recovering(&req.run_id) {
            Err(Status::unavailable("run is being recovered"))
        } else if self.env.active_recoveries().is_startup_pending() {
            Err(Status::unavailable("startup recovery in progress"))
        } else {
            Err(grpc_err::not_found("task", &req.task_id))
        }
    }

    async fn task_heartbeat(
        &self,
        request: Request<TaskHeartbeatRequest>,
    ) -> Result<Response<TaskHeartbeatResponse>, Status> {
        let req = request.into_inner();
        if req.worker_id.is_empty() {
            return Err(grpc_err::invalid_field(
                "worker_id",
                "worker_id is required",
            ));
        }
        let result = self.task_registry.heartbeat(&req.task_id, &req.worker_id);
        match result {
            HeartbeatResult::InProgress => Ok(Response::new(TaskHeartbeatResponse {
                should_abort: false,
                status: crate::proto::stepflow::v1::TaskStatus::InProgress as i32,
            })),
            HeartbeatResult::AlreadyClaimed => Ok(Response::new(TaskHeartbeatResponse {
                should_abort: true,
                status: crate::proto::stepflow::v1::TaskStatus::AlreadyClaimed as i32,
            })),
            HeartbeatResult::NotFound => {
                if self.is_run_recovering(&req.run_id) {
                    Err(Status::unavailable("run is being recovered"))
                } else if self.env.active_recoveries().is_startup_pending() {
                    Err(Status::unavailable("startup recovery in progress"))
                } else {
                    Err(grpc_err::not_found("task", &req.task_id))
                }
            }
        }
    }
}

impl OrchestratorServiceImpl {
    /// Validate that this orchestrator owns the given root run.
    ///
    /// If `root_run_id` is `None`, the check is skipped (backwards compatibility).
    /// Returns `Ok(())` if the run is active, or a gRPC error:
    /// - `UNAVAILABLE` if the run is being recovered or startup recovery is pending
    /// - `NOT_FOUND` if the run is not on this orchestrator
    fn check_run_ownership(&self, root_run_id: &Option<String>) -> Result<(), Status> {
        let Some(id_str) = root_run_id.as_deref() else {
            return Ok(());
        };
        let id = uuid::Uuid::parse_str(id_str)
            .map_err(|_| grpc_err::invalid_field("root_run_id", "invalid UUID format"))?;

        if self.env.active_executions().contains(&id) {
            return Ok(());
        }
        if self.env.active_recoveries().contains(&id) {
            return Err(Status::unavailable("run is being recovered"));
        }
        if self.env.active_recoveries().is_startup_pending() {
            return Err(Status::unavailable("startup recovery in progress"));
        }
        Err(grpc_err::not_found("run", id_str))
    }

    /// Check if the given run_id is currently being recovered.
    ///
    /// Uses the optional `run_id` from the request. If not provided,
    /// returns false (can't determine recovery state without it).
    fn is_run_recovering(&self, run_id: &Option<String>) -> bool {
        let Some(run_id_str) = run_id.as_deref() else {
            return false;
        };
        let Ok(run_id) = uuid::Uuid::parse_str(run_id_str) else {
            return false;
        };
        self.env.active_recoveries().contains(&run_id)
    }
}

fn run_status_to_proto(s: &stepflow_domain::RunStatus) -> OrchestratorRunStatus {
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
