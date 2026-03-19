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

use std::{path::Path, sync::Arc};

use crate::{Result, RunContext, StepflowEnvironment};
use serde::{Serialize, de::DeserializeOwned};
use stepflow_core::{
    component::ComponentInfo,
    workflow::{Component, StepId, ValueRef},
};

#[trait_variant::make(Send)]
#[dynosaur::dynosaur(pub DynPlugin = dyn Plugin)]
pub trait Plugin: Send + Sync {
    /// Ensure the plugin is initialized with the shared environment.
    ///
    /// This method is idempotent - calling it multiple times has no additional effect
    /// after the first successful initialization. Plugins should track their own
    /// initialization state and return `Ok(())` immediately if already initialized.
    ///
    /// Called during environment setup, before any runs are executed.
    /// Plugins can use this to connect to external services, start background tasks, etc.
    async fn ensure_initialized(&self, env: &Arc<StepflowEnvironment>) -> Result<()>;

    /// List all components available in this plugin.
    async fn list_components(&self) -> Result<Vec<ComponentInfo>>;

    /// Return the outputs for the given component.
    async fn component_info(&self, component: &Component) -> Result<ComponentInfo>;

    /// Start a task for the given component.
    ///
    /// The plugin dispatches work and delivers the result via the shared
    /// [`TaskRegistry`] (accessed through `run_context.env().task_registry()`).
    /// The executor registers the task_id in the registry *before* calling
    /// this method and awaits the result via the registry's oneshot channel.
    ///
    /// For synchronous plugins (builtins, protocol, MCP), this executes the
    /// component inline and calls `task_registry.complete(task_id, result)`.
    /// For queue-based plugins (gRPC pull), this dispatches to the queue and
    /// returns immediately — the worker delivers the result later.
    ///
    /// # Arguments
    /// * `task_id` - Unique task identifier (registered in TaskRegistry by the executor)
    /// * `component` - The component to execute
    /// * `run_context` - The run context with flow, environment, and subflow submission
    /// * `step` - The step being executed (None for workflow-level operations)
    /// * `input` - The resolved input values
    /// * `attempt` - The execution attempt number (1-based). A monotonically
    ///   increasing counter that increments on every re-execution of this step,
    ///   regardless of the reason (transport error, component error, or
    ///   orchestrator recovery).
    /// * `route_params` - Additional parameters from the route rule (e.g.,
    ///   `subject` for NATS, `queueName` for gRPC). These override plugin-level
    ///   defaults. Empty for most plugins.
    ///
    /// [`TaskRegistry`]: crate::TaskRegistry
    #[allow(clippy::too_many_arguments)]
    async fn start_task(
        &self,
        task_id: &str,
        component: &Component,
        run_context: &Arc<RunContext>,
        step: Option<&StepId>,
        input: ValueRef,
        attempt: u32,
        route_params: &std::collections::HashMap<String, serde_json::Value>,
    ) -> Result<()>;

    /// Prepare the plugin for a retry after a transport error.
    ///
    /// Implementations may use this to recover / re-initialize resources needed
    /// for the next attempt (e.g. restarting a crashed subprocess). The default
    /// is a no-op.
    async fn prepare_for_retry(&self) -> Result<()>;
}

/// Trait implemented by a deserializable plugin configuration.
pub trait PluginConfig: Serialize + DeserializeOwned {
    type Error: error_stack::Context;

    fn create_plugin(
        self,
        working_directory: &Path,
    ) -> impl Future<Output = error_stack::Result<Box<DynPlugin<'static>>, Self::Error>>;
}
