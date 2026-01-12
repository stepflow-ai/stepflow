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

//! Environment carrier for Stepflow execution.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::routing::PluginRouter;
use crate::{DynPlugin, Plugin as _, Result};
use error_stack::ResultExt as _;
use stepflow_core::workflow::{Component, ValueRef};
use stepflow_state::{InMemoryStateStore, StateStore};

use crate::PluginError;

/// Environment for Stepflow flow execution.
///
/// This struct holds the shared resources needed for flow execution:
/// - State store for persisting runs, results, and blobs
/// - Working directory for file operations
/// - Plugin router for component resolution
///
/// `StepflowEnvironment` is designed to be created once and shared across
/// multiple flow executions.
///
/// # Creation
///
/// Use [`StepflowEnvironment::new`] to create an environment with custom resources,
/// or [`StepflowEnvironment::new_in_memory`] for testing with an in-memory state store.
///
/// # Example
///
/// ```ignore
/// use stepflow_plugin::StepflowEnvironment;
///
/// let env = StepflowEnvironment::new(
///     state_store,
///     PathBuf::from("/path/to/working/dir"),
///     plugin_router,
/// ).await?;
///
/// // Use env for flow execution
/// ```
pub struct StepflowEnvironment {
    state_store: Arc<dyn StateStore>,
    working_directory: PathBuf,
    plugin_router: PluginRouter,
}

impl StepflowEnvironment {
    /// Create a new Stepflow environment with a custom state store and plugin router.
    ///
    /// This initializes all plugins before returning. Plugin initialization is idempotent,
    /// so plugins that are already initialized will not be re-initialized.
    pub async fn new(
        state_store: Arc<dyn StateStore>,
        working_directory: PathBuf,
        plugin_router: PluginRouter,
    ) -> Result<Arc<Self>> {
        let env = Arc::new(Self {
            state_store,
            working_directory,
            plugin_router,
        });

        // Initialize all plugins
        for plugin in env.plugins() {
            plugin
                .ensure_initialized(&env)
                .await
                .change_context(PluginError::Initializing)?;
        }

        Ok(env)
    }

    /// Create a new Stepflow environment with an in-memory state store and empty plugin router.
    ///
    /// This is primarily useful for testing.
    pub async fn new_in_memory() -> Result<Arc<Self>> {
        let plugin_router = PluginRouter::builder().build().unwrap();
        Self::new(
            Arc::new(InMemoryStateStore::new()),
            PathBuf::from("."),
            plugin_router,
        )
        .await
    }

    /// Get a reference to the state store.
    pub fn state_store(&self) -> &Arc<dyn StateStore> {
        &self.state_store
    }

    /// Get the working directory.
    pub fn working_directory(&self) -> &Path {
        &self.working_directory
    }

    /// Get the plugin router for accessing routing functionality.
    pub fn plugin_router(&self) -> &PluginRouter {
        &self.plugin_router
    }

    /// Get a plugin and resolved component name for execution.
    pub fn get_plugin_and_component(
        &self,
        component: &Component,
        input: ValueRef,
    ) -> Result<(&Arc<DynPlugin<'static>>, String)> {
        self.plugin_router
            .get_plugin_and_component(component.path(), input)
            .change_context(PluginError::InvalidInput)
    }

    /// List all registered plugins.
    pub fn plugins(&self) -> impl Iterator<Item = &Arc<DynPlugin<'static>>> {
        self.plugin_router.plugins()
    }
}

impl Drop for StepflowEnvironment {
    fn drop(&mut self) {
        log::info!("StepflowEnvironment being dropped, cleaning up plugins");
    }
}
