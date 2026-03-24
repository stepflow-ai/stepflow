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

//! Task lifecycle: claim → heartbeat → execute → complete.
//!
//! This module handles a single `TaskAssignment` from start to finish:
//! 1. Send an initial `TaskHeartbeat` to claim the task (skip if already claimed)
//! 2. Spawn a background heartbeat loop
//! 3. Execute the component or respond to a `ListComponents` request
//! 4. Send `CompleteTask` with the result or error
//! 5. Cancel the heartbeat loop

use std::sync::Arc;
use std::time::Duration;

use tonic::transport::Channel;
use tracing::{debug, warn};

use stepflow_proto::{
    CompleteTaskRequest, ComponentExecuteResponse, ComponentInfo as ProtoComponentInfo,
    ListComponentsResult, TaskAssignment, TaskHeartbeatRequest, TaskStatus,
    complete_task_request::Result as TaskResult,
    orchestrator_service_client::OrchestratorServiceClient, task_assignment::Task,
};

use crate::{
    ComponentContext, ComponentRegistry,
    context::{json_to_proto_value, proto_value_to_json},
    error::ComponentError,
};

/// Process a single `TaskAssignment` to completion.
///
/// Returns `true` if the task was successfully claimed and executed (or listed),
/// `false` if it was already claimed by another worker.
pub(crate) async fn handle_task(
    assignment: TaskAssignment,
    registry: Arc<ComponentRegistry>,
    worker_id: &str,
    default_orchestrator_url: &str,
    default_blob_url: Option<&str>,
    orch_channel: Channel,
) -> bool {
    let task_id = assignment.task_id.clone();
    let heartbeat_interval_secs = assignment.heartbeat_interval_secs;
    let task_context = assignment.context.clone().unwrap_or_default();

    // Determine orchestrator URL: use the one in task context if provided, else default
    let orchestrator_url = if task_context.orchestrator_service_url.is_empty() {
        default_orchestrator_url.to_string()
    } else {
        task_context.orchestrator_service_url.clone()
    };

    // Build the orchestrator channel (reuse the pull channel if URLs match)
    let orch_ch = orch_channel.clone();
    let mut orch_client = OrchestratorServiceClient::new(orch_ch.clone());

    // --- Step 1: Claim the task with an initial heartbeat ---
    let heartbeat_resp = match orch_client
        .task_heartbeat(TaskHeartbeatRequest {
            task_id: task_id.clone(),
            worker_id: worker_id.to_string(),
            progress: None,
            status_message: None,
            run_id: None,
        })
        .await
    {
        Ok(r) => r.into_inner(),
        Err(e) => {
            warn!(task_id, "Failed to send initial heartbeat: {e}");
            return false;
        }
    };

    if heartbeat_resp.status == TaskStatus::AlreadyClaimed as i32 {
        debug!(task_id, "Task already claimed by another worker, skipping");
        return false;
    }

    // --- Step 2: Start background heartbeat loop ---
    let heartbeat_interval = Duration::from_secs(heartbeat_interval_secs.max(1) as u64);
    let heartbeat_task_id = task_id.clone();
    let heartbeat_worker_id = worker_id.to_string();
    let heartbeat_ch = orch_channel.clone();
    let heartbeat_handle = tokio::spawn(async move {
        let mut client = OrchestratorServiceClient::new(heartbeat_ch);
        loop {
            tokio::time::sleep(heartbeat_interval).await;
            if let Err(e) = client
                .task_heartbeat(TaskHeartbeatRequest {
                    task_id: heartbeat_task_id.clone(),
                    worker_id: heartbeat_worker_id.clone(),
                    progress: None,
                    status_message: None,
                    run_id: None,
                })
                .await
            {
                warn!(task_id = heartbeat_task_id, "Heartbeat failed: {e}");
            }
        }
    });

    // --- Step 3: Dispatch on task type ---
    let result = match assignment.task {
        Some(Task::Execute(req)) => {
            execute_component(
                *req,
                &task_id,
                registry,
                orchestrator_url,
                default_blob_url,
                task_context,
                orch_ch,
            )
            .await
        }
        Some(Task::ListComponents(req)) => list_components_response(&registry, req),
        None => {
            warn!(task_id, "Received task with no payload");
            Err(ComponentError::WorkerError(
                "Empty task payload".to_string(),
            ))
        }
    };

    // --- Step 4: Cancel heartbeat and send CompleteTask ---
    heartbeat_handle.abort();

    let complete_result = match result {
        Ok(proto_result) => proto_result,
        Err(err) => {
            let code = err.task_error_code() as i32;
            TaskResult::Error(stepflow_proto::TaskError {
                code,
                message: err.to_string(),
                data: None,
            })
        }
    };

    if let Err(e) = OrchestratorServiceClient::new(orch_channel)
        .complete_task(CompleteTaskRequest {
            task_id: task_id.clone(),
            result: Some(complete_result),
            run_id: None,
        })
        .await
    {
        warn!(task_id, "Failed to complete task: {e}");
    }

    true
}

/// Execute a component and return the proto result.
async fn execute_component(
    req: stepflow_proto::ComponentExecuteRequest,
    task_id: &str,
    registry: Arc<ComponentRegistry>,
    _orchestrator_url: String,
    blob_url: Option<&str>,
    task_context: stepflow_proto::TaskContext,
    orch_channel: Channel,
) -> Result<TaskResult, ComponentError> {
    let component_path = &req.component;

    let component = registry.lookup(component_path).ok_or_else(|| {
        ComponentError::WorkerError(format!("Component not found: {component_path}"))
    })?;

    // Convert proto input → serde_json::Value
    let input_json = req
        .input
        .as_ref()
        .map(proto_value_to_json)
        .unwrap_or(serde_json::Value::Null);

    // Build observability context fields
    let obs = req.observability.unwrap_or_default();

    // Build ComponentContext
    let ctx = ComponentContext::new(
        blob_url.map(str::to_string),
        obs.run_id.filter(|s: &String| !s.is_empty()),
        Some(task_context.root_run_id).filter(|s: &String| !s.is_empty()),
        obs.flow_id.filter(|s: &String| !s.is_empty()),
        obs.step_id.filter(|s: &String| !s.is_empty()),
        req.attempt,
        orch_channel,
    );

    debug!(task_id, component = component_path, "Executing component");

    let output_json = component.execute(input_json, &ctx).await?;

    let output_proto = json_to_proto_value(output_json);

    Ok(TaskResult::Response(ComponentExecuteResponse {
        output: Some(output_proto),
    }))
}

/// Build a `ListComponents` result from the registry.
fn list_components_response(
    registry: &ComponentRegistry,
    _req: stepflow_proto::ListComponentsRequest,
) -> Result<TaskResult, ComponentError> {
    let components = registry
        .list_components()
        .into_iter()
        .map(|info| {
            let input_schema = info
                .input_schema
                .and_then(|s| serde_json::to_string(&s).ok());
            let output_schema = info
                .output_schema
                .and_then(|s| serde_json::to_string(&s).ok());
            ProtoComponentInfo {
                name: info.name,
                description: Some(info.description.unwrap_or_default()),
                input_schema,
                output_schema,
            }
        })
        .collect();

    Ok(TaskResult::ListComponents(ListComponentsResult {
        components,
    }))
}
