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
use std::time::Duration;

use error_stack::ResultExt as _;
use stepflow_core::StepflowEnvironment;
use stepflow_state::{
    ActiveExecutions, BlobStore, ExecutionJournal, InMemoryStateStore, LeaseManager, MetadataStore,
    NoOpJournal, NoOpLeaseManager, OrchestratorId,
};

use crate::routing::PluginRouter;
use crate::{Plugin as _, PluginError, PluginRouterExt as _, Result};

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
    metadata_store: Option<Arc<dyn MetadataStore>>,
    blob_store: Option<Arc<dyn BlobStore>>,
    execution_journal: Option<Arc<dyn ExecutionJournal>>,
    lease_manager: Option<Arc<dyn LeaseManager>>,
    working_directory: Option<PathBuf>,
    plugin_router: Option<PluginRouter>,
    blob_api_url: Option<String>,
    blob_threshold: usize,
    orchestrator_id: Option<OrchestratorId>,
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
            metadata_store: None,
            blob_store: None,
            execution_journal: None,
            lease_manager: None,
            working_directory: None,
            plugin_router: None,
            blob_api_url: None,
            blob_threshold: 0,
            orchestrator_id: None,
        }
    }

    /// Set the metadata store.
    pub fn metadata_store(mut self, store: Arc<dyn MetadataStore>) -> Self {
        self.metadata_store = Some(store);
        self
    }

    /// Set the blob store for content-addressed storage.
    ///
    /// For implementations where the metadata store also implements BlobStore
    /// (like InMemoryStateStore or SqliteStateStore), you can pass the same
    /// object as both metadata_store and blob_store.
    pub fn blob_store(mut self, store: Arc<dyn BlobStore>) -> Self {
        self.blob_store = Some(store);
        self
    }

    /// Set the execution journal for recovery.
    ///
    /// If not set, a no-op journal is used (events are discarded).
    /// For implementations where the metadata store also implements
    /// ExecutionJournal (like InMemoryStateStore or SqliteStateStore),
    /// you can pass the same object as both.
    pub fn execution_journal(mut self, journal: Arc<dyn ExecutionJournal>) -> Self {
        self.execution_journal = Some(journal);
        self
    }

    /// Set the lease manager for distributed run coordination.
    ///
    /// If not set, a `NoOpLeaseManager` is used which always grants leases
    /// (single-orchestrator mode). For distributed deployments, use an
    /// implementation like etcd-based lease management to coordinate run
    /// ownership across orchestrators.
    pub fn lease_manager(mut self, manager: Arc<dyn LeaseManager>) -> Self {
        self.lease_manager = Some(manager);
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

    /// Set the orchestrator ID for distributed lease management.
    ///
    /// When set, the orchestrator ID is stored in the environment and used
    /// for lease acquisition/release during run execution.
    pub fn orchestrator_id(mut self, id: OrchestratorId) -> Self {
        self.orchestrator_id = Some(id);
        self
    }

    /// Set the blob API URL for workers.
    ///
    /// When set, workers will use direct HTTP requests to this URL for blob operations.
    ///
    /// This URL must be the base blobs collection endpoint. Workers will:
    /// - `POST {url}` to create a blob
    /// - `GET {url}/{blob_id}` to fetch a blob
    ///
    /// Example: `http://localhost:7840/api/v1/blobs` or `http://blob-service/api/v1/blobs`
    pub fn blob_api_url(mut self, url: Option<String>) -> Self {
        self.blob_api_url = url;
        self
    }

    /// Set the blob threshold for automatic blobification.
    ///
    /// When a top-level field in a component's input or output exceeds this size
    /// (in bytes of JSON serialization), it is stored as a blob and replaced with
    /// a `$blob` reference. Set to 0 to disable (default).
    pub fn blob_threshold(mut self, threshold: usize) -> Self {
        self.blob_threshold = threshold;
        self
    }

    /// Build the environment, initializing all plugins.
    ///
    /// This is async because plugin initialization may require async operations
    /// (e.g., connecting to external services).
    pub async fn build(self) -> Result<Arc<StepflowEnvironment>> {
        let metadata_store = self.metadata_store.ok_or_else(|| {
            error_stack::report!(PluginError::Initializing)
                .attach_printable("metadata_store is required")
        })?;

        let blob_store = self.blob_store.ok_or_else(|| {
            error_stack::report!(PluginError::Initializing)
                .attach_printable("blob_store is required")
        })?;

        let working_directory = self.working_directory.ok_or_else(|| {
            error_stack::report!(PluginError::Initializing)
                .attach_printable("working_directory is required")
        })?;

        let plugin_router = self.plugin_router.ok_or_else(|| {
            error_stack::report!(PluginError::Initializing)
                .attach_printable("plugin_router is required")
        })?;

        // Use provided journal or default to no-op (discards events)
        let journal: Arc<dyn ExecutionJournal> = self
            .execution_journal
            .unwrap_or_else(|| Arc::new(NoOpJournal::new()));

        // Use provided lease manager or default to no-op (single-orchestrator mode)
        let lease_manager: Arc<dyn LeaseManager> = self
            .lease_manager
            .unwrap_or_else(|| Arc::new(NoOpLeaseManager::new(Duration::from_secs(30))));

        let mut env = StepflowEnvironment::new();
        env.insert(metadata_store);
        env.insert(blob_store);
        env.insert(journal);
        env.insert(lease_manager);
        env.insert(working_directory);
        env.insert(plugin_router);

        // Store blob API configuration for workers
        env.insert(BlobApiUrl::new(self.blob_api_url, self.blob_threshold));

        // Store orchestrator ID if set (distributed mode)
        if let Some(id) = self.orchestrator_id {
            env.insert(id);
        }

        // Always create ActiveExecutions for tracking running executions
        env.insert(ActiveExecutions::new());

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
    /// The in-memory state store is also used as the execution journal and blob store.
    pub async fn build_in_memory() -> Result<Arc<StepflowEnvironment>> {
        let plugin_router = PluginRouter::builder()
            .build()
            .change_context(PluginError::Initializing)?;

        let store = Arc::new(InMemoryStateStore::new());

        Self::new()
            .metadata_store(store.clone())
            .blob_store(store.clone())
            .execution_journal(store)
            .working_directory(PathBuf::from("."))
            .plugin_router(plugin_router)
            .build()
            .await
    }
}
