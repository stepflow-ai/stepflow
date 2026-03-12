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
use std::time::Duration;

use error_stack::ResultExt as _;
use stepflow_core::FlowResult;
use stepflow_core::component::ComponentInfo;
use stepflow_core::workflow::{Component, StepId, ValueRef};
use stepflow_plugin::{
    OrchestratorServiceUrl, PluginError, Result, RunContext, StepflowEnvironment,
};

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
    /// PullPlugin) and the environment was built before the port was known.
    orchestrator_url_override: std::sync::RwLock<Option<String>>,
    /// Maximum time a task can sit in the queue before a worker calls
    /// StartTask. Must be greater than zero (validated at config load time).
    queue_timeout: Duration,
    /// Maximum time from StartTask to CompleteTask. `None` means no
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
    /// Used by `PullPlugin` after starting the gRPC server to inject
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

impl stepflow_plugin::Plugin for StepflowQueuePlugin {
    async fn ensure_initialized(&self, _env: &Arc<StepflowEnvironment>) -> Result<()> {
        // Queue transports don't need stateful initialization.
        // Workers connect independently and register their components.
        Ok(())
    }

    async fn list_components(&self) -> Result<Vec<ComponentInfo>> {
        self.transport.list_components().await
    }

    async fn component_info(&self, component: &Component) -> Result<ComponentInfo> {
        self.transport.component_info(component.path()).await
    }

    async fn execute(
        &self,
        component: &Component,
        run_context: &Arc<RunContext>,
        step: Option<&StepId>,
        input: ValueRef,
        attempt: u32,
    ) -> Result<FlowResult> {
        // Generate a unique task ID
        let task_id = uuid::Uuid::now_v7().to_string();

        // Build observability context
        let observability = build_observability_context(run_context, step);

        // Build execution context with orchestrator URL, applying any override
        let mut context = build_execution_context(run_context.env());
        if let Some(override_url) = self
            .orchestrator_url_override
            .read()
            .expect("lock poisoned")
            .as_ref()
        {
            let ctx = context.get_or_insert_with(|| proto::TaskContext {
                orchestrator_service_url: String::new(),
            });
            ctx.orchestrator_service_url = override_url.clone();
        }

        // Convert input to proto Value
        let proto_input = value_ref_to_proto(&input)?;

        let task = proto::TaskAssignment {
            task_id: task_id.clone(),
            request: Some(proto::ComponentExecuteRequest {
                component: component.path().to_string(),
                input: Some(proto_input),
                attempt,
                observability: Some(observability),
                context,
            }),
            deadline_secs: self.queue_timeout.as_secs() as u32,
            heartbeat_interval_secs: HEARTBEAT_INTERVAL_SECS,
            execution_timeout_secs: self.execution_timeout.map_or(0, |d| d.as_secs() as u32),
        };

        // Register waiter before sending (avoids race condition where
        // CompleteTask arrives before we're listening).
        // The registry handles queue timeout internally via a watcher task.
        let rx = self.registry.register(
            task_id.clone(),
            component.path().to_string(),
            self.queue_timeout,
            self.execution_timeout,
        );

        // Dispatch the task via the configured transport
        if let Err(e) = self.transport.send_task(task).await {
            // Clean up the registration on send failure
            self.registry.remove(&task_id);
            return Err(e);
        }

        // Wait for the result. The registry's timeout watchers will
        // deliver a FlowResult::Failed if queue or heartbeat timeouts
        // expire, so we just await the oneshot.
        //
        // As a safety net, if execution_timeout is set, apply a generous
        // outer timeout to prevent truly indefinite hangs.
        let safety_timeout = self.queue_timeout
            + self.execution_timeout.unwrap_or(Duration::from_secs(3600))
            + Duration::from_secs(30); // grace period

        match tokio::time::timeout(safety_timeout, rx).await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(_)) => {
                // Sender was dropped — task was cleaned up
                Err(error_stack::report!(PluginError::Execution)
                    .attach_printable(format!("task '{task_id}' was cancelled")))
            }
            Err(_elapsed) => {
                // Safety net timeout — should not normally fire
                self.registry.remove(&task_id);
                Err(error_stack::report!(PluginError::Execution)
                    .attach_printable(format!("task '{task_id}' exceeded safety timeout")))
            }
        }
    }

    async fn prepare_for_retry(&self) -> Result<()> {
        // Queue transports don't need retry preparation.
        // The connection is worker→orchestrator, not the other way around.
        Ok(())
    }
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

fn build_execution_context(env: &StepflowEnvironment) -> Option<proto::TaskContext> {
    let orchestrator_url = env
        .get::<OrchestratorServiceUrl>()
        .and_then(|u| u.url().map(|s| s.to_string()))
        .unwrap_or_default();

    if orchestrator_url.is_empty() {
        return None;
    }

    Some(proto::TaskContext {
        orchestrator_service_url: orchestrator_url,
    })
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
