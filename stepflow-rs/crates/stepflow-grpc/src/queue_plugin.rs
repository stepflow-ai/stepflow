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

//! Queue-based plugin for dispatching component executions to workers.
//!
//! [`StepflowQueuePlugin`] implements the `Plugin` trait by:
//! 1. Building a `TaskAssignment` from the execution parameters
//! 2. Dispatching it via a [`TaskTransport`] (SQLite/pull, NATS, Kafka, etc.)
//! 3. Waiting for the result via a [`PendingTasks`] oneshot channel
//!
//! Task completion is handled by `OrchestratorService::complete_task`, which
//! routes results back through the registry.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use error_stack::ResultExt as _;
use stepflow_core::component::ComponentInfo;
use stepflow_core::workflow::{Component, StepId, ValueRef};
use stepflow_plugin::{
    OrchestratorServiceUrl, PluginError, Result, RunContext, StepflowEnvironment,
};

/// Monotonic counter for generating unique discovery task IDs.
static DISCOVERY_COUNTER: AtomicU64 = AtomicU64::new(0);

use crate::pending_tasks::PendingTasks;
use crate::proto::stepflow::v1 as proto;
use crate::task_transport::TaskTransport;

/// Default heartbeat interval communicated to workers via TaskAssignment.
const HEARTBEAT_INTERVAL_SECS: u32 = 1;

/// Plugin that dispatches component executions via a task queue.
///
/// This is the orchestrator-side plugin for all queue-based transports
/// (pull, NATS, Kafka, etc.). The transport determines how tasks are
/// delivered to workers; the plugin handles task creation and result
/// collection uniformly.
pub struct StepflowQueuePlugin {
    transport: Box<dyn TaskTransport>,
    registry: Arc<PendingTasks>,
    /// Override for the orchestrator service URL in TaskContext.
    /// When set, this URL is used instead of the one from the environment.
    /// This is needed when the gRPC server is started dynamically (e.g., by
    /// GrpcPlugin) and the environment was built before the port was known.
    orchestrator_url_override: std::sync::RwLock<Option<String>>,
    /// Maximum time a task can sit in the queue before a worker sends its
    /// first heartbeat. Must be greater than zero (validated at config load time).
    queue_timeout: Duration,
    /// Maximum time from first heartbeat to CompleteTask. `None` means no
    /// execution timeout (heartbeat-only crash detection).
    execution_timeout: Option<Duration>,
}

impl StepflowQueuePlugin {
    /// Create a new queue plugin with the given transport and completion
    /// registry.
    ///
    /// The `registry` must be the same instance shared with the
    /// `OrchestratorServiceImpl` that handles `CompleteTask` RPCs.
    pub fn new(
        transport: Box<dyn TaskTransport>,
        registry: Arc<PendingTasks>,
        queue_timeout: Duration,
        execution_timeout: Option<Duration>,
    ) -> Self {
        Self {
            transport,
            registry,
            orchestrator_url_override: std::sync::RwLock::new(None),
            queue_timeout,
            execution_timeout,
        }
    }

    /// Set an override for the orchestrator service URL.
    ///
    /// Used by `GrpcPlugin` after starting the gRPC server to inject
    /// the dynamically assigned port into task assignments.
    pub fn set_orchestrator_url(&self, url: String) {
        let mut guard = self
            .orchestrator_url_override
            .write()
            .expect("lock poisoned");
        *guard = Some(url);
    }

    /// Get a reference to the completion registry.
    ///
    /// Used by `OrchestratorServiceImpl` to resolve tasks.
    pub fn registry(&self) -> &Arc<PendingTasks> {
        &self.registry
    }
}

/// Timeout for discovery tasks (how long to wait for a worker to respond).
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(5);

/// Sentinel component name used for discovery tasks in PendingTasks tracking.
const DISCOVERY_COMPONENT: &str = "__stepflow/list_components";

impl stepflow_plugin::Plugin for StepflowQueuePlugin {
    async fn ensure_initialized(&self, _env: &Arc<StepflowEnvironment>) -> Result<()> {
        // Queue transports don't need stateful initialization.
        // Workers connect independently and register their components.
        Ok(())
    }

    async fn list_components(&self) -> Result<Vec<ComponentInfo>> {
        // Send a ListComponentsRequest task through the normal task pipeline.
        // Any worker in the queue picks it up and responds with the component
        // list via CompleteTask. The result flows through PendingTasks/TaskRegistry.
        let seq = DISCOVERY_COUNTER.fetch_add(1, Ordering::Relaxed);
        let task_id = format!("discovery-{seq}");

        // Build context with orchestrator URL so the worker can send CompleteTask.
        // This is always set by GrpcPlugin::ensure_initialized() before tasks flow.
        let context = self.build_context(None);
        if context.is_none() {
            log::warn!("No orchestrator URL available for discovery task — workers won't be able to respond");
        }

        let task = proto::TaskAssignment {
            task_id: task_id.clone(),
            task: Some(proto::task_assignment::Task::ListComponents(
                proto::ListComponentsRequest {},
            )),
            context,
            deadline_secs: DISCOVERY_TIMEOUT.as_secs() as u32,
            heartbeat_interval_secs: 0,
            execution_timeout_secs: 0,
        };

        // Register in TaskRegistry + track in PendingTasks (for queue timeout).
        // Must happen before send_task to avoid a race where the worker
        // completes before we register.
        let rx = self.registry.register_and_track(
            task_id.clone(),
            DISCOVERY_COMPONENT.to_string(),
            DISCOVERY_TIMEOUT,
            None,
        );

        // Dispatch via transport
        if let Err(e) = self
            .transport
            .send_task(task, &std::collections::HashMap::new())
            .await
        {
            // Clean up both PendingTasks tracking and TaskRegistry entry
            self.registry.untrack_and_remove(&task_id);
            log::debug!("Discovery task dispatch failed (no workers?): {e}");
            return Ok(vec![]);
        }

        // Wait for the worker's response
        match tokio::time::timeout(DISCOVERY_TIMEOUT, rx).await {
            Ok(Ok(stepflow_core::FlowResult::Success(value))) => {
                // Parse the JSON-wrapped component list
                parse_discovery_result(&value)
            }
            Ok(Ok(stepflow_core::FlowResult::Failed(e))) => {
                log::warn!("Discovery task failed: {e}");
                Ok(vec![])
            }
            Ok(Err(_)) => {
                // Receiver dropped (task timed out via PendingTasks)
                log::warn!("Discovery task cancelled (worker timeout?)");
                Ok(vec![])
            }
            Err(_) => {
                // Our own timeout fired
                log::warn!("Discovery task timed out after {}s", DISCOVERY_TIMEOUT.as_secs());
                Ok(vec![])
            }
        }
    }

    async fn component_info(&self, component: &Component) -> Result<ComponentInfo> {
        // Use list_components and find the matching component
        let components = self.list_components().await?;
        components
            .into_iter()
            .find(|c| c.component.path() == component.path())
            .ok_or_else(|| {
                error_stack::report!(PluginError::ComponentInfo)
                    .attach_printable(format!("component '{}' not found", component.path()))
            })
    }

    async fn start_task(
        &self,
        task_id: &str,
        component: &Component,
        run_context: &Arc<RunContext>,
        step: Option<&StepId>,
        input: ValueRef,
        attempt: u32,
        route_params: &std::collections::HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        // Build observability context
        let observability = build_observability_context(run_context, step);

        // Build task context with orchestrator URL (override takes precedence
        // over environment) and root_run_id for ownership validation.
        let mut context = self.build_context(Some(run_context.env()));
        if let Some(ctx) = context.as_mut() {
            ctx.root_run_id = run_context.root_run_id.to_string();
        }

        // Convert input to proto Value
        let proto_input = value_ref_to_proto(&input)?;

        let task = proto::TaskAssignment {
            task_id: task_id.to_string(),
            task: Some(proto::task_assignment::Task::Execute(
                proto::ComponentExecuteRequest {
                    component: component.path().to_string(),
                    input: Some(proto_input),
                    attempt,
                    observability: Some(observability),
                },
            )),
            context,
            deadline_secs: self.queue_timeout.as_secs() as u32,
            heartbeat_interval_secs: HEARTBEAT_INTERVAL_SECS,
            execution_timeout_secs: self.execution_timeout.map_or(0, |d| d.as_secs() as u32),
        };

        // Set up timeout tracking in PendingTasks (heartbeat, queue timeout).
        // The task is already registered in the shared TaskRegistry by the executor;
        // PendingTasks adds gRPC-specific lifecycle tracking on top.
        self.registry.track(
            task_id.to_string(),
            component.path().to_string(),
            self.queue_timeout,
            self.execution_timeout,
        );

        // Dispatch the task via the configured transport
        if let Err(e) = self.transport.send_task(task, route_params).await {
            // Clean up the tracking on send failure
            self.registry.untrack(task_id);
            return Err(e);
        }

        Ok(())
    }

    async fn prepare_for_retry(&self) -> Result<()> {
        // Queue transports don't need retry preparation.
        // The connection is worker→orchestrator, not the other way around.
        Ok(())
    }
}

impl StepflowQueuePlugin {
    /// Build a TaskContext with the orchestrator URL.
    ///
    /// The override (set by `GrpcPlugin` after starting the gRPC server)
    /// takes precedence. If `env` is provided, falls back to the
    /// environment's `OrchestratorServiceUrl`. Returns `None` if no URL
    /// is available from either source.
    fn build_context(&self, env: Option<&StepflowEnvironment>) -> Option<proto::TaskContext> {
        let orchestrator_url = self
            .orchestrator_url_override
            .read()
            .expect("lock poisoned")
            .clone()
            .or_else(|| {
                env.and_then(|e| {
                    e.get::<OrchestratorServiceUrl>()
                        .and_then(|u| u.url().map(|s| s.to_string()))
                })
            })
            .unwrap_or_default();

        if orchestrator_url.is_empty() {
            return None;
        }

        Some(proto::TaskContext {
            orchestrator_service_url: orchestrator_url,
            root_run_id: String::new(),
        })
    }
}

/// Parse a discovery result from `FlowResult::Success(JSON)` back to
/// `Vec<ComponentInfo>`.
///
/// The JSON format matches what `OrchestratorServiceImpl::complete_task`
/// produces when converting `ListComponentsResult` to `FlowResult`.
fn parse_discovery_result(value: &ValueRef) -> Result<Vec<ComponentInfo>> {
    let json = value.as_ref();
    let components_json = json
        .get("components")
        .and_then(|v| v.as_array())
        .unwrap_or(&Vec::new())
        .clone();

    let mut components = Vec::with_capacity(components_json.len());
    for c in components_json {
        let name = c.get("name").and_then(|v| v.as_str()).unwrap_or_default();
        let description = c.get("description").and_then(|v| v.as_str()).map(String::from);
        let input_schema = c
            .get("input_schema")
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str(s).ok());
        let output_schema = c
            .get("output_schema")
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str(s).ok());

        components.push(ComponentInfo {
            component: stepflow_core::workflow::Component::from_string(name.to_string()),
            description,
            input_schema,
            output_schema,
        });
    }
    Ok(components)
}

// --- Conversion helpers (same logic as the former GrpcComponentClient) ---

fn build_observability_context(
    run_context: &RunContext,
    step: Option<&StepId>,
) -> proto::ObservabilityContext {
    let (trace_id, span_id) = extract_trace_context();
    proto::ObservabilityContext {
        trace_id,
        span_id,
        run_id: Some(run_context.run_id.to_string()),
        flow_id: Some(run_context.flow_id().to_string()),
        step_id: step.map(|s| s.to_string()),
    }
}

/// Extract trace context from the current fastrace span.
fn extract_trace_context() -> (Option<String>, Option<String>) {
    if let Some(span_context) = fastrace::prelude::SpanContext::current_local_parent() {
        (
            Some(format!("{:032x}", span_context.trace_id.0)),
            Some(format!("{:016x}", span_context.span_id.0)),
        )
    } else {
        (None, None)
    }
}


fn value_ref_to_proto(value: &ValueRef) -> Result<prost_wkt_types::Value> {
    let json = serde_json::to_value(value.as_ref())
        .change_context(PluginError::Execution)
        .attach_printable("Failed to serialize input to JSON")?;
    let proto_value: prost_wkt_types::Value = serde_json::from_value(json)
        .change_context(PluginError::Execution)
        .attach_printable("Failed to convert JSON to proto Value")?;
    Ok(proto_value)
}
