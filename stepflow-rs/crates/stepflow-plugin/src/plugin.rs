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

use std::{collections::HashMap, path::Path, sync::Arc};

use crate::{PluginError, Result, RunContext, StepflowEnvironment};
use stepflow_core::{
    component::ComponentInfo,
    workflow::{Component, StepId, ValueRef},
};

/// All dispatch-specific fields for a task.
///
/// Mirrors proto `ComponentExecuteRequest` for wire-bound fields.
/// Additional fields (`task_id`, `route_params`) are orchestrator-internal.
pub struct TaskRequest {
    /// Unique task identifier (registered in TaskRegistry by the executor).
    /// Not sent in `ComponentExecuteRequest` — it's on `TaskAssignment`.
    pub task_id: String,

    /// The component ID to execute (e.g., "foo"). Used by workers for flat lookup.
    /// Maps to proto `ComponentExecuteRequest.component_id`.
    pub component_id: String,

    /// Parameters extracted from path pattern matching (e.g., `{path: "bar"}`).
    /// Passed to the component function by the worker SDK.
    /// Maps to proto `ComponentExecuteRequest.path_params`.
    pub path_params: HashMap<String, String>,

    /// The resolved input for the component.
    /// Maps to proto `ComponentExecuteRequest.input` (converted to protobuf Value).
    pub input: ValueRef,

    /// Execution attempt number (1-based, monotonically increasing).
    /// Maps to proto `ComponentExecuteRequest.attempt`.
    pub attempt: u32,

    /// Transport-level parameters from the matched route rule config
    /// (e.g., `stream` for NATS, `queueName` for gRPC).
    /// These override plugin-level defaults and are consumed by the
    /// transport layer — they never reach the component function.
    /// NOT sent in `ComponentExecuteRequest`.
    pub route_params: HashMap<String, serde_json::Value>,
}

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
    /// [`TaskRegistry`]: crate::TaskRegistry
    async fn start_task(
        &self,
        request: &TaskRequest,
        run_context: &Arc<RunContext>,
        step: Option<&StepId>,
    ) -> Result<()>;

    /// Prepare the plugin for a retry after a transport error.
    ///
    /// Implementations may use this to recover / re-initialize resources needed
    /// for the next attempt (e.g. restarting a crashed subprocess). The default
    /// is a no-op.
    async fn prepare_for_retry(&self) -> Result<()>;
}

/// Trait for stateless plugin factory types.
///
/// Each plugin crate provides a unit struct implementing this trait, allowing
/// uniform `Factory::create_dyn(config, working_directory)` construction.
///
/// Named `create_dyn` to leave room for a typed `create` method returning
/// the concrete plugin type in the future.
pub trait PluginFactory {
    type Config;

    fn create_dyn(
        config: Self::Config,
        working_directory: &Path,
    ) -> impl Future<Output = error_stack::Result<Box<DynPlugin<'static>>, PluginError>>;
}
