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

use scc::HashCache;
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
#[allow(clippy::too_many_arguments)]
pub(crate) async fn handle_task(
    assignment: TaskAssignment,
    registry: Arc<ComponentRegistry>,
    worker_id: &str,
    default_orchestrator_url: &str,
    default_normalised_url: &str,
    default_blob_url: Option<&str>,
    default_orch_channel: Channel,
    channel_cache: &ChannelCache,
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
        default_normalised_url,
        default_orch_channel,
        channel_cache,
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

/// Maximum number of cached orchestrator channels.
///
/// In practice workers talk to 1–2 orchestrators, so a small cap suffices.
/// LRU eviction drops idle connections from orchestrators that have scaled
/// away or been replaced during a rolling deploy.
const MAX_CACHED_CHANNELS: usize = 16;

/// A cache of gRPC channels keyed by normalised URL.
///
/// Uses `scc::HashCache` which provides LRU eviction (preventing unbounded
/// growth of idle connections) with bucket-level locking (negligible
/// contention for the small number of entries we expect).
pub(crate) type ChannelCache = Arc<HashCache<String, Channel>>;

/// Create a new, empty channel cache.
pub(crate) fn new_channel_cache() -> ChannelCache {
    Arc::new(HashCache::with_capacity(0, MAX_CACHED_CHANNELS))
}

/// Ensure a URL has an `http://` scheme (the orchestrator may advertise
/// scheme-less addresses like `"127.0.0.1:7837"` in run/test mode).
fn ensure_scheme(url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("http://{url}")
    }
}

/// Normalise a URL for comparison and cache-key purposes only.
///
/// Adds an `http://` scheme if missing, strips trailing slashes, and treats
/// `localhost` and `127.0.0.1` as equivalent. The result must NOT be used as
/// the endpoint to connect to — use [`ensure_scheme`] for that — because
/// rewriting `localhost` could break setups where it intentionally resolves
/// to `::1` or a non-standard hosts entry.
pub(crate) fn normalize_url(url: &str) -> String {
    let with_scheme = ensure_scheme(url);
    let trimmed = with_scheme.trim_end_matches('/');

    // Extract the host from the authority to do an exact `localhost` match,
    // avoiding false positives like `localhost.example.com`.
    let Some(scheme_end) = trimmed.find("://") else {
        return trimmed.to_string();
    };
    let authority_start = scheme_end + 3;
    let authority_end = trimmed[authority_start..]
        .find(['/', '?', '#'])
        .map(|offset| authority_start + offset)
        .unwrap_or(trimmed.len());
    let host_port = &trimmed[authority_start..authority_end];

    if host_port == "localhost" {
        format!(
            "{}127.0.0.1{}",
            &trimmed[..authority_start],
            &trimmed[authority_end..]
        )
    } else if let Some(port) = host_port.strip_prefix("localhost:") {
        format!(
            "{}127.0.0.1:{}{}",
            &trimmed[..authority_start],
            port,
            &trimmed[authority_end..]
        )
    } else {
        trimmed.to_string()
    }
}

/// Resolve the orchestrator gRPC channel to use for this task.
///
/// Reuses `default_channel` when the normalised URL matches
/// `default_normalised_url`, then falls back to the `channel_cache` for
/// previously-seen URLs. Only creates a new `connect_lazy()` channel when
/// the URL has never been seen before.
///
/// `default_normalised_url` should be pre-computed once via [`normalize_url`]
/// at worker startup to avoid re-allocating on every task.
///
/// Returns `None` (and logs a warning) if `url` is not a valid gRPC endpoint.
pub(crate) fn resolve_orch_channel(
    task_id: &str,
    url: &str,
    default_normalised_url: &str,
    default_channel: Channel,
    channel_cache: &ChannelCache,
) -> Option<Channel> {
    let normalised = normalize_url(url);

    if normalised == default_normalised_url {
        return Some(default_channel);
    }

    // Fast path: channel already cached for this URL.
    if let Some(ch) = channel_cache.read(&normalised, |_k, v| v.clone()) {
        return Some(ch);
    }

    // Slow path: validate the URL, then insert into the cache. The channel is
    // created inside or_put_with so that racing tasks don't each allocate a
    // throwaway connection.
    let with_scheme = ensure_scheme(url);
    match tonic::transport::Channel::from_shared(with_scheme.clone()) {
        Ok(endpoint) => {
            let (_evicted, occupied) = channel_cache
                .entry(normalised)
                .or_put_with(|| endpoint.connect_lazy());
            let ch = occupied.get().clone();
            Some(ch)
        }
        Err(e) => {
            warn!(
                task_id,
                orchestrator_url = %with_scheme,
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
    let component_id = &req.component_id;

    let component = registry.lookup(component_id).ok_or_else(|| {
        ComponentError::WorkerError(format!("Component not found: {component_id}"))
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

    debug!(task_id, component_id, "Executing component");

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
                component_id: info.name,
                description: Some(info.description.unwrap_or_default()),
                input_schema,
                output_schema,
                path: info.path,
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
        let registry = make_registry(&["alpha", "beta", "gamma"]);
        let result = list_components_response(&registry, stepflow_proto::ListComponentsRequest {});
        match result.unwrap() {
            TaskResult::ListComponents(r) => {
                let ids: Vec<_> = r
                    .components
                    .iter()
                    .map(|c| c.component_id.as_str())
                    .collect();
                assert_eq!(ids, ["alpha", "beta", "gamma"]);
            }
            other => panic!("unexpected result: {other:?}"),
        }
    }

    #[test]
    fn test_list_components_response_includes_path() {
        let registry = make_registry(&["echo"]);
        let result = list_components_response(&registry, stepflow_proto::ListComponentsRequest {});
        match result.unwrap() {
            TaskResult::ListComponents(r) => {
                assert_eq!(r.components.len(), 1);
                assert_eq!(r.components[0].component_id, "echo");
                assert_eq!(r.components[0].path, "/echo");
            }
            other => panic!("unexpected result: {other:?}"),
        }
    }

    #[test]
    fn test_normalize_url_adds_scheme() {
        assert_eq!(normalize_url("127.0.0.1:7837"), "http://127.0.0.1:7837");
    }

    #[test]
    fn test_normalize_url_localhost_to_ip() {
        assert_eq!(
            normalize_url("http://localhost:7837"),
            "http://127.0.0.1:7837"
        );
    }

    #[test]
    fn test_normalize_url_strips_trailing_slash() {
        assert_eq!(
            normalize_url("http://127.0.0.1:7837/"),
            "http://127.0.0.1:7837"
        );
    }

    #[test]
    fn test_normalize_url_localhost_without_port() {
        assert_eq!(normalize_url("http://localhost"), "http://127.0.0.1");
    }

    #[test]
    fn test_normalize_url_does_not_rewrite_localhost_subdomain() {
        // Must not rewrite hosts like localhost.example.com
        assert_eq!(
            normalize_url("http://localhost.example.com:8080"),
            "http://localhost.example.com:8080"
        );
    }

    #[tokio::test]
    async fn test_resolve_orch_channel_reuses_default() {
        let url = "http://127.0.0.1:7837";
        let normalised = normalize_url(url);
        let channel = tonic::transport::Channel::from_shared(url)
            .unwrap()
            .connect_lazy();
        let cache = new_channel_cache();
        let result = resolve_orch_channel("task-1", url, &normalised, channel, &cache);
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_resolve_orch_channel_normalises_scheme() {
        let default_url = "http://127.0.0.1:7837";
        let normalised = normalize_url(default_url);
        let channel = tonic::transport::Channel::from_shared(default_url)
            .unwrap()
            .connect_lazy();
        let cache = new_channel_cache();
        // Scheme-less variant — normalises to match default.
        let result = resolve_orch_channel("task-1", "127.0.0.1:7837", &normalised, channel, &cache);
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_resolve_orch_channel_localhost_matches_ip() {
        // The core bug: localhost:PORT should match 127.0.0.1:PORT
        let default_url = "http://localhost:7837";
        let normalised = normalize_url(default_url);
        let channel = tonic::transport::Channel::from_shared(default_url)
            .unwrap()
            .connect_lazy();
        let cache = new_channel_cache();
        let result = resolve_orch_channel(
            "task-1",
            "http://127.0.0.1:7837",
            &normalised,
            channel,
            &cache,
        );
        assert!(result.is_some());
        // Cache should be empty because we matched the default
        assert!(cache.is_empty());
    }

    #[tokio::test]
    async fn test_resolve_orch_channel_caches_new_url() {
        let default_url = "http://127.0.0.1:7837";
        let normalised = normalize_url(default_url);
        let channel = tonic::transport::Channel::from_shared(default_url)
            .unwrap()
            .connect_lazy();
        let cache = new_channel_cache();
        let other_url = "http://10.0.0.5:7837";

        // First call creates a new channel and caches it
        let result =
            resolve_orch_channel("task-1", other_url, &normalised, channel.clone(), &cache);
        assert!(result.is_some());
        assert_eq!(cache.len(), 1);

        // Second call with the same URL reuses the cached channel
        let result2 = resolve_orch_channel("task-2", other_url, &normalised, channel, &cache);
        assert!(result2.is_some());
        assert_eq!(cache.len(), 1);
    }

    #[tokio::test]
    async fn test_resolve_orch_channel_invalid_url() {
        let normalised = normalize_url("http://127.0.0.1:7837");
        let channel = tonic::transport::Channel::from_shared("http://127.0.0.1:7837")
            .unwrap()
            .connect_lazy();
        let cache = new_channel_cache();
        let result = resolve_orch_channel("task-1", "not a url !!!", &normalised, channel, &cache);
        assert!(result.is_none());
    }
}
