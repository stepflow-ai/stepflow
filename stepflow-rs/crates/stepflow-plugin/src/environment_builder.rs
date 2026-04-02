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

//! Environment initialization for Stepflow.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use error_stack::ResultExt as _;
use stepflow_core::StepflowEnvironment;
use stepflow_state::{
    ActiveExecutions, ActiveRecoveries, CheckpointStore, ExecutionJournal, InMemoryStateStore,
    LeaseManager, NoOpCheckpointStore, NoOpJournal, NoOpLeaseManager,
};

use crate::routing::PluginRouter;
use crate::{Plugin as _, PluginError, PluginRouterExt as _, Result};

/// Orchestrator service URL stored in the environment for worker callbacks.
///
/// Workers use this URL to call back to the orchestrator (e.g., submit sub-runs,
/// get run status) during component execution.
#[derive(Debug, Clone)]
pub struct OrchestratorServiceUrl(Option<String>);

impl OrchestratorServiceUrl {
    /// Create a new OrchestratorServiceUrl configuration.
    pub fn new(url: Option<String>) -> Self {
        Self(url)
    }

    /// Get the orchestrator service URL if configured.
    pub fn url(&self) -> Option<&str> {
        self.0.as_deref()
    }
}

/// Blob API configuration stored in the environment for worker communication.
///
/// Contains the URL workers should use for direct HTTP blob access and the
/// threshold for automatic blobification.
#[derive(Debug, Clone)]
pub struct BlobApiUrl {
    url: Option<String>,
    blob_threshold: usize,
}

impl BlobApiUrl {
    /// Create a new BlobApiUrl configuration.
    pub fn new(url: Option<String>, blob_threshold: usize) -> Self {
        Self {
            url,
            blob_threshold,
        }
    }

    /// Get the blob API URL if configured.
    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    /// Get the blob threshold (0 means disabled).
    pub fn blob_threshold(&self) -> usize {
        self.blob_threshold
    }
}

/// Finalize a [`StepflowEnvironment`] by initializing stores and plugins.
///
/// Inserts defaults for optional resources (NoOp stores, default configs),
/// initializes the checkpoint store backend, inserts `ActiveExecutions`
/// tracking, and ensures all registered plugins are initialized.
///
/// Call this after inserting all required resources into the environment.
///
/// # Required resources
///
/// The environment must contain at minimum:
/// - `Arc<dyn MetadataStore>`
/// - `Arc<dyn BlobStore>`
/// - `PathBuf` (working directory)
/// - `Arc<PluginRouter>`
///
/// # Example
///
/// ```ignore
/// use stepflow_plugin::initialize_environment;
///
/// let env = Arc::new(StepflowEnvironment::new());
/// env.insert(metadata_store);
/// env.insert(blob_store);
/// env.insert(working_directory);
/// env.insert(Arc::new(plugin_router) as Arc<PluginRouter>);
/// initialize_environment(&env).await?;
/// ```
pub async fn initialize_environment(env: &Arc<StepflowEnvironment>) -> Result<()> {
    use std::path::PathBuf;

    use stepflow_core::RetryConfig;
    use stepflow_state::{BlobStore, MetadataStore};

    use crate::execution_config::ExecutionConfig;

    // Validate required resources
    if !env.contains::<Arc<dyn MetadataStore>>() {
        return Err(error_stack::report!(PluginError::Initializing)
            .attach_printable("metadata_store is required"));
    }
    if !env.contains::<Arc<dyn BlobStore>>() {
        return Err(error_stack::report!(PluginError::Initializing)
            .attach_printable("blob_store is required"));
    }
    if !env.contains::<PathBuf>() {
        return Err(error_stack::report!(PluginError::Initializing)
            .attach_printable("working_directory is required"));
    }
    if !env.contains::<Arc<PluginRouter>>() {
        return Err(error_stack::report!(PluginError::Initializing)
            .attach_printable("plugin_router is required"));
    }

    // Insert defaults for optional stores if not already set
    if !env.contains::<Arc<dyn ExecutionJournal>>() {
        env.insert::<Arc<dyn ExecutionJournal>>(Arc::new(NoOpJournal::new()));
    }
    if !env.contains::<Arc<dyn CheckpointStore>>() {
        env.insert::<Arc<dyn CheckpointStore>>(Arc::new(NoOpCheckpointStore));
    }
    if !env.contains::<Arc<dyn LeaseManager>>() {
        env.insert::<Arc<dyn LeaseManager>>(Arc::new(NoOpLeaseManager::new(Duration::from_secs(
            30,
        ))));
    }

    // Initialize the checkpoint store backend (e.g., create tables)
    env.get::<Arc<dyn CheckpointStore>>()
        .expect("checkpoint store was just inserted above")
        .initialize_checkpoint_store()
        .await
        .change_context(PluginError::Initializing)?;

    // Insert defaults for optional configuration if not already set
    if !env.contains::<BlobApiUrl>() {
        env.insert(BlobApiUrl::new(None, 0));
    }
    if !env.contains::<OrchestratorServiceUrl>() {
        env.insert(OrchestratorServiceUrl::new(None));
    }
    if !env.contains::<ExecutionConfig>() {
        env.insert(ExecutionConfig {
            checkpoint_interval: 0,
        });
    }
    if !env.contains::<RetryConfig>() {
        env.insert(RetryConfig::default());
    }

    // Create TaskRegistry for task result delivery if not already set
    if !env.contains::<Arc<crate::TaskRegistry>>() {
        env.insert(Arc::new(crate::TaskRegistry::new()));
    }

    // Create ActiveExecutions for tracking running executions if not already set
    if !env.contains::<ActiveExecutions>() {
        env.insert(ActiveExecutions::new());
    }

    // Create ActiveRecoveries for tracking runs being recovered if not already set
    if !env.contains::<ActiveRecoveries>() {
        env.insert(ActiveRecoveries::new());
    }

    // Initialize all plugins
    for plugin in env.plugins() {
        plugin
            .ensure_initialized(env)
            .await
            .change_context(PluginError::Initializing)?;
    }

    Ok(())
}

/// Build an in-memory environment for testing.
///
/// Creates an environment with an in-memory state store (used for metadata,
/// blobs, journal, and checkpoints), current directory as working directory,
/// and an empty plugin router.
pub async fn build_in_memory_environment() -> Result<Arc<StepflowEnvironment>> {
    use stepflow_state::{BlobStore, MetadataStore};

    let plugin_router = PluginRouter::builder()
        .build()
        .await
        .change_context(PluginError::Initializing)?;

    let store = Arc::new(InMemoryStateStore::new());

    let env = Arc::new(StepflowEnvironment::new());
    env.insert(store.clone() as Arc<dyn MetadataStore>);
    env.insert(store.clone() as Arc<dyn BlobStore>);
    env.insert(store.clone() as Arc<dyn ExecutionJournal>);
    env.insert(store as Arc<dyn CheckpointStore>);
    env.insert(PathBuf::from("."));
    env.insert(Arc::new(plugin_router) as Arc<PluginRouter>);

    initialize_environment(&env).await?;

    Ok(env)
}
