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

//! Builder for constructing StepflowEnvironment with plugins.

use std::path::PathBuf;
use std::sync::Arc;

use error_stack::ResultExt as _;
use stepflow_core::StepflowEnvironment;
use stepflow_state::{InMemoryStateStore, StateStore};

use crate::routing::PluginRouter;
use crate::{Plugin as _, PluginError, PluginRouterExt as _, Result};

/// Builder for constructing a StepflowEnvironment.
///
/// This builder ensures all required resources are set before
/// creating the environment, and handles plugin initialization.
///
/// # Example
///
/// ```ignore
/// use stepflow_plugin::StepflowEnvironmentBuilder;
///
/// let env = StepflowEnvironmentBuilder::new()
///     .state_store(state_store)
///     .working_directory(PathBuf::from("/working/dir"))
///     .plugin_router(plugin_router)
///     .build()
///     .await?;
/// ```
pub struct StepflowEnvironmentBuilder {
    state_store: Option<Arc<dyn StateStore>>,
    working_directory: Option<PathBuf>,
    plugin_router: Option<PluginRouter>,
}

impl Default for StepflowEnvironmentBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl StepflowEnvironmentBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            state_store: None,
            working_directory: None,
            plugin_router: None,
        }
    }

    /// Set the state store.
    pub fn state_store(mut self, store: Arc<dyn StateStore>) -> Self {
        self.state_store = Some(store);
        self
    }

    /// Set the working directory.
    pub fn working_directory(mut self, path: PathBuf) -> Self {
        self.working_directory = Some(path);
        self
    }

    /// Set the plugin router.
    pub fn plugin_router(mut self, router: PluginRouter) -> Self {
        self.plugin_router = Some(router);
        self
    }

    /// Build the environment, initializing all plugins.
    ///
    /// This is async because plugin initialization may require async operations
    /// (e.g., connecting to external services).
    pub async fn build(self) -> Result<Arc<StepflowEnvironment>> {
        let state_store = self.state_store.ok_or_else(|| {
            error_stack::report!(PluginError::Initializing)
                .attach_printable("state_store is required")
        })?;

        let working_directory = self.working_directory.ok_or_else(|| {
            error_stack::report!(PluginError::Initializing)
                .attach_printable("working_directory is required")
        })?;

        let plugin_router = self.plugin_router.ok_or_else(|| {
            error_stack::report!(PluginError::Initializing)
                .attach_printable("plugin_router is required")
        })?;

        let mut env = StepflowEnvironment::new();
        env.insert(state_store);
        env.insert(working_directory);
        env.insert(plugin_router);

        let env = Arc::new(env);

        // Initialize all plugins
        for plugin in env.plugins() {
            plugin
                .ensure_initialized(&env)
                .await
                .change_context(PluginError::Initializing)?;
        }

        Ok(env)
    }

    /// Build an in-memory environment for testing.
    ///
    /// This creates an environment with an in-memory state store,
    /// current directory as working directory, and empty plugin router.
    pub async fn build_in_memory() -> Result<Arc<StepflowEnvironment>> {
        let plugin_router = PluginRouter::builder()
            .build()
            .change_context(PluginError::Initializing)?;

        Self::new()
            .state_store(Arc::new(InMemoryStateStore::new()))
            .working_directory(PathBuf::from("."))
            .plugin_router(plugin_router)
            .build()
            .await
    }
}
