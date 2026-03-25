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
//! 1. Resolve the orchestrator channel for this task (may differ from the pull channel)
//! 2. Send an initial `TaskHeartbeat` to claim the task (skip if already claimed)
//! 3. Spawn a background heartbeat loop
//! 4. Execute the component or respond to a `ListComponents` request
//! 5. Send `CompleteTask` with the result or error
//! 6. Cancel the heartbeat loop

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

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Process a single `TaskAssignment` to completion.
///
/// Returns `true` if the task was successfully claimed and executed (or listed),
/// `false` if it was already claimed by another worker or if a fatal setup error occurred.
pub(crate) async fn handle_task(
    assignment: TaskAssignment,
    registry: Arc<ComponentRegistry>,
    worker_id: &str,
    default_orchestrator_url: &str,
    default_blob_url: Option<&str>,
    default_orch_channel: Channel,
) -> bool {
    let task_id = assignment.task_id.clone();
    let heartbeat_interval_secs = assignment.heartbeat_interval_secs;
    let task_context = assignment.context.clone().unwrap_or_default();

    // Determine orchestrator URL: task context may specify a different one (e.g. in
    // multi-orchestrator deployments). Build a dedicated channel only when the URL differs
    // to avoid redundant connections.
    let orchestrator_url = if task_context.orchestrator_service_url.is_empty() {
        default_orchestrator_url.to_string()
    } else {
        task_context.orchestrator_service_url.clone()
    };

    let Some(orch_ch) = resolve_orch_channel(
        &task_id,
        &orchestrator_url,
        default_orchestrator_url,
        default_orch_channel,
    ) else {
        return false;
    };

    // --- Step 1: Claim the task ---
    if !claim_task(
        &task_id,
        worker_id,
        &mut OrchestratorServiceClient::new(orch_ch.clone()),
    )
    .await
    {
        return false;
    }

    // --- Step 2: Start background heartbeat loop ---
    let heartbeat_handle = spawn_heartbeat(
        task_id.clone(),
        worker_id.to_string(),
        heartbeat_interval_secs,
        orch_ch.clone(),
    );

    // --- Step 3: Dispatch on task type ---
    let result = dispatch_task(
        assignment.task,
        &task_id,
        registry,
        default_blob_url,
        task_context,
        orch_ch.clone(),
    )
    .await;

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

    if let Err(e) = OrchestratorServiceClient::new(orch_ch)
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

// ---------------------------------------------------------------------------
// Helpers (split out for testability)
// ---------------------------------------------------------------------------

/// Resolve the orchestrator gRPC channel to use for this task.
///
/// Reuses `default_channel` when the normalized `url` matches `default_url` to avoid opening
/// a new connection. Returns `None` (and logs a warning) if `url` is not a valid gRPC endpoint.
///
/// The orchestrator may advertise its address without an `http://` scheme (e.g.
/// `"127.0.0.1:7837"` in `run`/`test` mode).  We normalise such URLs by prepending
/// `"http://"` before comparing and before creating the tonic channel.
pub(crate) fn resolve_orch_channel(
    task_id: &str,
    url: &str,
    default_url: &str,
    default_channel: Channel,
) -> Option<Channel> {
    // Normalise: add http:// scheme if the URL has no scheme.
    let normalised = if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("http://{url}")
    };

    if normalised == default_url {
        return Some(default_channel);
    }

    match tonic::transport::Channel::from_shared(normalised.clone()) {
        Ok(endpoint) => Some(endpoint.connect_lazy()),
        Err(e) => {
            warn!(
                task_id,
                orchestrator_url = %normalised,
                "Invalid orchestrator URL from task context, skipping task: {e}"
            );
            None
        }
    }
}

/// Attempt to claim a task by sending the initial heartbeat.
///
/// Returns `true` if the claim succeeded, `false` if the task was already claimed
/// by another worker or if the gRPC call failed.
pub(crate) async fn claim_task(
    task_id: &str,
    worker_id: &str,
    orch_client: &mut OrchestratorServiceClient<Channel>,
) -> bool {
    let resp = match orch_client
        .task_heartbeat(TaskHeartbeatRequest {
            task_id: task_id.to_string(),
            worker_id: worker_id.to_string(),
            progress: None,
            status_message: None,
            run_id: None,
        })
        .await
    {
        Ok(r) => r.into_inner(),
        Err(e) => {
            warn!(task_id, worker_id, "Failed to send initial heartbeat: {e}");
            return false;
        }
    };

    if resp.status == TaskStatus::AlreadyClaimed as i32 {
        debug!(task_id, "Task already claimed by another worker, skipping");
        return false;
    }

    true
}

/// Spawn a background task that sends periodic heartbeats to keep the task alive.
///
/// The returned `JoinHandle` should be aborted once the task completes or fails.
fn spawn_heartbeat(
    task_id: String,
    worker_id: String,
    interval_secs: u32,
    orch_channel: Channel,
) -> tokio::task::JoinHandle<()> {
    let interval = Duration::from_secs(interval_secs.max(1) as u64);
    tokio::spawn(async move {
        let mut client = OrchestratorServiceClient::new(orch_channel);
        loop {
            tokio::time::sleep(interval).await;
            if let Err(e) = client
                .task_heartbeat(TaskHeartbeatRequest {
                    task_id: task_id.clone(),
                    worker_id: worker_id.clone(),
                    progress: None,
                    status_message: None,
                    run_id: None,
                })
                .await
            {
                warn!(task_id, worker_id, "Heartbeat failed: {e}");
            }
        }
    })
}

/// Dispatch a task to the appropriate handler based on its type.
async fn dispatch_task(
    task: Option<Task>,
    task_id: &str,
    registry: Arc<ComponentRegistry>,
    blob_url: Option<&str>,
    task_context: stepflow_proto::TaskContext,
    orch_channel: Channel,
) -> Result<TaskResult, ComponentError> {
    match task {
        Some(Task::Execute(req)) => {
            execute_component(
                *req,
                task_id,
                registry,
                blob_url,
                task_context,
                orch_channel,
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
    }
}

// ---------------------------------------------------------------------------
// Component execution
// ---------------------------------------------------------------------------

/// Execute a component and return the proto result.
async fn execute_component(
    req: stepflow_proto::ComponentExecuteRequest,
    task_id: &str,
    registry: Arc<ComponentRegistry>,
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

// ---------------------------------------------------------------------------
// Component listing
// ---------------------------------------------------------------------------

/// Build a `ListComponents` result from the registry.
pub(crate) fn list_components_response(
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_registry(names: &[&str]) -> ComponentRegistry {
        let mut registry = ComponentRegistry::new();
        for name in names {
            let owned = name.to_string();
            let key = owned.clone();
            registry.register_fn(&key, move |_: serde_json::Value, _ctx| {
                let name = owned.clone();
                async move { Ok(serde_json::json!({ "component": name })) }
            });
        }
        registry
    }

    #[test]
    fn test_list_components_response_empty() {
        let registry = make_registry(&[]);
        let result = list_components_response(&registry, stepflow_proto::ListComponentsRequest {});
        match result.unwrap() {
            TaskResult::ListComponents(r) => assert!(r.components.is_empty()),
            other => panic!("unexpected result: {other:?}"),
        }
    }

    #[test]
    fn test_list_components_response_names() {
        let registry = make_registry(&["/alpha", "/beta", "/gamma"]);
        let result = list_components_response(&registry, stepflow_proto::ListComponentsRequest {});
        match result.unwrap() {
            TaskResult::ListComponents(r) => {
                let names: Vec<_> = r.components.iter().map(|c| c.name.as_str()).collect();
                assert_eq!(names, ["/alpha", "/beta", "/gamma"]);
            }
            other => panic!("unexpected result: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_resolve_orch_channel_reuses_default() {
        // When URLs match, the same channel object is returned (not a new connection).
        let url = "http://127.0.0.1:7837";
        let endpoint = tonic::transport::Channel::from_shared(url).unwrap();
        let channel = endpoint.connect_lazy();
        let result = resolve_orch_channel("task-1", url, url, channel);
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_resolve_orch_channel_normalises_scheme() {
        // The orchestrator may advertise "127.0.0.1:port" without http://.
        // resolve_orch_channel should prepend http:// so the channel is valid.
        let default_url = "http://127.0.0.1:7837";
        let channel = tonic::transport::Channel::from_shared(default_url)
            .unwrap()
            .connect_lazy();
        // Scheme-less variant — normalises to "http://127.0.0.1:7837", which equals default_url.
        let result = resolve_orch_channel("task-1", "127.0.0.1:7837", default_url, channel);
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_resolve_orch_channel_invalid_url() {
        let channel = tonic::transport::Channel::from_shared("http://127.0.0.1:7837")
            .unwrap()
            .connect_lazy();
        let result =
            resolve_orch_channel("task-1", "not a url !!!", "http://127.0.0.1:7837", channel);
        assert!(result.is_none());
    }
}
