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

//! Environment instantiation from a [`StepflowConfig`].
//!
//! This module bridges the pure-data configuration types in `stepflow-config`
//! with the concrete runtime implementations in `stepflow-state`,
//! `stepflow-state-sql`, `stepflow-builtins`, `stepflow-grpc`, etc.
//!
//! State crates, routing, and plugin crates all import their config types
//! directly from `stepflow-config`. Plugin crates provide factory functions
//! that accept the shared config types, eliminating the need for serde bridging.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use error_stack::ResultExt as _;
use stepflow_config::{
    ConfigError, LeaseManagerConfig, StepflowConfig, StorageConfig, StoreConfig, SupportedPlugin,
    SupportedPluginConfig,
};
use stepflow_plugin::routing::PluginRouter;
use stepflow_plugin::{
    BlobApiUrl, DynPlugin, ExecutionConfig, OrchestratorServiceUrl, PluginFactory as _,
    StepflowEnvironment,
};
use stepflow_state::{
    BlobStore, CheckpointStore, ExecutionJournal, FilesystemBlobStore, InMemoryStateStore,
    InstrumentedBlobStore, LeaseManager, MetadataStore, NoOpLeaseManager, OrchestratorId,
};

type Result<T> = std::result::Result<T, error_stack::Report<ConfigError>>;

// ---------------------------------------------------------------------------
// Store instantiation
// ---------------------------------------------------------------------------

/// Collection of stores created from storage configuration.
pub(crate) struct Stores {
    pub metadata_store: Arc<dyn MetadataStore>,
    pub blob_store: Arc<dyn BlobStore>,
    pub execution_journal: Arc<dyn ExecutionJournal>,
    pub checkpoint_store: Arc<dyn CheckpointStore>,
}

/// A concrete store instance that can provide different store trait implementations.
#[derive(Clone)]
enum ConcreteStore {
    InMemory(Arc<InMemoryStateStore>),
    Sql(Arc<stepflow_state_sql::SqlStateStore>),
    Filesystem(Arc<FilesystemBlobStore>),
}

impl ConcreteStore {
    fn as_metadata(&self) -> Result<Arc<dyn MetadataStore>> {
        match self {
            ConcreteStore::InMemory(s) => Ok(s.clone()),
            ConcreteStore::Sql(s) => Ok(s.clone()),
            ConcreteStore::Filesystem(_) => Err(error_stack::report!(ConfigError::Configuration))
                .attach_printable("Filesystem store only supports blob storage, not metadata"),
        }
    }

    fn as_blob(&self) -> Result<Arc<dyn BlobStore>> {
        let inner: Arc<dyn BlobStore> = match self {
            ConcreteStore::InMemory(s) => s.clone(),
            ConcreteStore::Sql(s) => s.clone(),
            ConcreteStore::Filesystem(s) => s.clone(),
        };
        Ok(Arc::new(InstrumentedBlobStore::new(inner)))
    }

    fn as_journal(&self) -> Result<Arc<dyn ExecutionJournal>> {
        match self {
            ConcreteStore::InMemory(s) => Ok(s.clone()),
            ConcreteStore::Sql(s) => Ok(s.clone()),
            ConcreteStore::Filesystem(_) => Err(error_stack::report!(ConfigError::Configuration))
                .attach_printable(
                    "Filesystem store only supports blob storage, not execution journal",
                ),
        }
    }

    fn as_checkpoint(&self) -> Result<Arc<dyn CheckpointStore>> {
        match self {
            ConcreteStore::InMemory(s) => Ok(s.clone()),
            ConcreteStore::Sql(s) => Ok(s.clone()),
            ConcreteStore::Filesystem(_) => Err(error_stack::report!(ConfigError::Configuration))
                .attach_printable(
                    "Filesystem store only supports blob storage, not checkpoint storage",
                ),
        }
    }
}

/// Create a [`ConcreteStore`] from a [`StoreConfig`].
///
/// State crates import their config types directly from `stepflow-config`,
/// so no serde bridging is needed here.
async fn create_concrete(config: &StoreConfig) -> Result<ConcreteStore> {
    match config {
        StoreConfig::InMemory => Ok(ConcreteStore::InMemory(Arc::new(InMemoryStateStore::new()))),
        StoreConfig::Sqlite(sql_config) | StoreConfig::Postgres(sql_config) => {
            let store = stepflow_state_sql::SqlStateStore::new(sql_config.clone())
                .await
                .change_context(ConfigError::Configuration)?;
            Ok(ConcreteStore::Sql(Arc::new(store)))
        }
        StoreConfig::Filesystem(fs_config) => {
            let store = match &fs_config.directory {
                Some(dir) => FilesystemBlobStore::new(dir.into())
                    .await
                    .change_context(ConfigError::Configuration)?,
                None => FilesystemBlobStore::temp().change_context(ConfigError::Configuration)?,
            };
            Ok(ConcreteStore::Filesystem(Arc::new(store)))
        }
    }
}

/// Create [`Stores`] from a [`StorageConfig`], with smart deduplication.
///
/// When multiple stores have identical configurations, they share a single
/// backend instance. Each store's `initialize_*` method is called to set up
/// only the tables needed for its role.
pub(crate) async fn create_stores(storage_config: &StorageConfig) -> Result<Stores> {
    let (metadata_config, blobs_config, journal_config) = storage_config.get_configs();

    // Deduplicate: create each unique config only once
    let mut cache: HashMap<StoreConfig, ConcreteStore> = HashMap::new();
    for config in [&metadata_config, &blobs_config, &journal_config] {
        if !cache.contains_key(config) {
            let store = create_concrete(config).await?;
            cache.insert(config.clone(), store);
        }
    }

    let metadata_concrete = cache[&metadata_config].clone();
    let blobs_concrete = cache[&blobs_config].clone();
    let journal_concrete = cache[&journal_config].clone();

    let metadata_store = metadata_concrete
        .as_metadata()
        .attach_printable("metadata store configuration")?;
    let blob_store = blobs_concrete
        .as_blob()
        .attach_printable("blob store configuration")?;
    let execution_journal = journal_concrete
        .as_journal()
        .attach_printable("journal configuration")?;
    let checkpoint_store = journal_concrete
        .as_checkpoint()
        .attach_printable("checkpoint store configuration")?;

    // Initialize each store (creates only the tables needed for its role)
    metadata_store
        .initialize_metadata_store()
        .await
        .change_context(ConfigError::Configuration)
        .attach_printable("failed to initialize metadata store")?;
    blob_store
        .initialize_blob_store()
        .await
        .change_context(ConfigError::Configuration)
        .attach_printable("failed to initialize blob store")?;
    execution_journal
        .initialize_journal()
        .await
        .change_context(ConfigError::Configuration)
        .attach_printable("failed to initialize execution journal")?;
    checkpoint_store
        .initialize_checkpoint_store()
        .await
        .change_context(ConfigError::Configuration)
        .attach_printable("failed to initialize checkpoint store")?;

    Ok(Stores {
        metadata_store,
        blob_store,
        execution_journal,
        checkpoint_store,
    })
}

// ---------------------------------------------------------------------------
// Lease manager instantiation
// ---------------------------------------------------------------------------

/// Create a [`LeaseManager`] from a [`LeaseManagerConfig`].
pub(crate) async fn create_lease_manager(
    config: &LeaseManagerConfig,
    lease_ttl: std::time::Duration,
) -> Result<Arc<dyn LeaseManager>> {
    match config {
        LeaseManagerConfig::NoOp => Ok(Arc::new(NoOpLeaseManager::new(lease_ttl))),
        #[cfg(feature = "etcd")]
        LeaseManagerConfig::Etcd(config) => {
            let manager = stepflow_state_etcd::EtcdLeaseManager::connect(config, lease_ttl)
                .await
                .change_context(ConfigError::Configuration)
                .attach_printable("Failed to connect to etcd for lease management")?;
            Ok(Arc::new(manager))
        }
        #[cfg(not(feature = "etcd"))]
        LeaseManagerConfig::Etcd(_) => Err(error_stack::report!(ConfigError::Configuration))
            .attach_printable(
                "etcd lease manager requires the `etcd` feature flag \
                 (compile stepflow with `--features etcd`)",
            ),
    }
}

// ---------------------------------------------------------------------------
// Plugin instantiation
// ---------------------------------------------------------------------------

/// Instantiate a plugin from a [`SupportedPluginConfig`].
///
/// Calls the factory function in each plugin crate directly, using the
/// shared config types from `stepflow-config`. No serde bridging needed.
async fn instantiate_plugin(
    plugin_config: SupportedPluginConfig,
    working_directory: &Path,
) -> Result<Box<DynPlugin<'static>>> {
    match plugin_config.plugin {
        SupportedPlugin::Builtin(config) => {
            stepflow_builtins::BuiltinPluginFactory::create_dyn(config, working_directory)
                .await
                .change_context(ConfigError::Configuration)
        }
        SupportedPlugin::Mock(value) => {
            // Mock configs may contain non-string map keys (e.g., {input: "a"} as
            // behavior keys), so they arrive as serde_yaml_ng::Value. Round-trip
            // through YAML string to deserialize into the concrete MockPlugin type.
            let yaml_str = serde_yaml_ng::to_string(&value)
                .change_context(ConfigError::Configuration)
                .attach_printable("Failed to serialize mock plugin config to YAML")?;
            let mock: stepflow_mock::MockPlugin = serde_yaml_ng::from_str(&yaml_str)
                .change_context(ConfigError::Configuration)
                .attach_printable("Failed to deserialize mock plugin config")?;
            stepflow_mock::MockPluginFactory::create_dyn(mock, working_directory)
                .await
                .change_context(ConfigError::Configuration)
        }
        SupportedPlugin::Mcp(config) => {
            stepflow_components_mcp::McpPluginFactory::create_dyn(config, working_directory)
                .await
                .change_context(ConfigError::Configuration)
        }
        SupportedPlugin::Grpc(config) => {
            stepflow_grpc::GrpcPluginFactory::create_dyn(config, working_directory)
                .await
                .change_context(ConfigError::Configuration)
        }
        #[cfg(feature = "nats")]
        SupportedPlugin::Nats(config) => {
            stepflow_nats::NatsPluginFactory::create_dyn(config, working_directory)
                .await
                .change_context(ConfigError::Configuration)
        }
        #[cfg(not(feature = "nats"))]
        SupportedPlugin::Nats(_) => Err(error_stack::report!(ConfigError::Configuration))
            .attach_printable(
                "NATS plugin requires the `nats` feature flag \
                 (compile stepflow with `--features nats`)",
            ),
    }
}

// ---------------------------------------------------------------------------
// Environment creation
// ---------------------------------------------------------------------------

/// Create a [`StepflowEnvironment`] from a [`StepflowConfig`].
///
/// This is the main entry point for turning a declarative configuration into a
/// fully wired runtime environment. It creates stores, the lease manager, the
/// plugin router, and populates the environment with all required components.
///
/// Plugin initialization is controlled by `skip_plugin_init`. When `true`,
/// [`stepflow_plugin::initialize_environment`] is not called — the caller is
/// responsible for calling it after the server is accepting connections (so
/// worker subprocesses can connect back).
///
/// `orchestrator_url` is the URL workers use to call back to this orchestrator.
///
/// `grpc_address` is the address advertised to pull-based workers for gRPC
/// connections. If `None`, workers will not know about a gRPC endpoint.
pub(crate) async fn create_environment_from_config(
    config: StepflowConfig,
    orchestrator_id: Option<OrchestratorId>,
    orchestrator_url: Option<String>,
    grpc_address: Option<String>,
    skip_plugin_init: bool,
) -> Result<Arc<StepflowEnvironment>> {
    // Create stores from storage configuration
    let stores = create_stores(&config.storage_config).await?;

    // Create lease manager
    let lease_ttl = std::time::Duration::from_secs(config.recovery.lease_ttl_secs);
    let lease_manager = create_lease_manager(&config.lease_manager, lease_ttl).await?;

    let working_directory = config
        .working_directory
        .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

    // Build the plugin router — RoutingConfig is shared between stepflow-config
    // and stepflow-plugin (stepflow-plugin re-exports from stepflow-config).
    log::info!("Routing Config: {:?}", config.routing);
    let mut plugin_router_builder = PluginRouter::builder().with_routing_config(config.routing);

    for (plugin_name, plugin_config) in config.plugins {
        let plugin = instantiate_plugin(plugin_config, &working_directory)
            .await
            .attach_printable_lazy(|| {
                format!("Failed to instantiate plugin for '{plugin_name}'")
            })?;
        plugin_router_builder = plugin_router_builder.register_plugin(plugin_name, plugin);
    }

    let plugin_router = plugin_router_builder
        .build()
        .await
        .change_context(ConfigError::Configuration)?;

    // Build the environment
    let env = Arc::new(StepflowEnvironment::new());
    env.insert(stores.metadata_store);
    env.insert(stores.blob_store);
    env.insert(stores.execution_journal);
    env.insert(stores.checkpoint_store);
    env.insert(lease_manager);
    env.insert(working_directory);
    env.insert(Arc::new(plugin_router) as Arc<PluginRouter>);
    let blob_threshold = config.blob_api.effective_blob_threshold();
    env.insert(BlobApiUrl::new(config.blob_api.url, blob_threshold));
    env.insert(OrchestratorServiceUrl::new(orchestrator_url));
    env.insert(ExecutionConfig {
        checkpoint_interval: config.recovery.checkpoint_interval,
    });
    env.insert(config.retry);
    if let Some(id) = orchestrator_id {
        env.insert(id);
    }

    // Insert the shared task registry for result delivery.
    let task_registry = Arc::new(stepflow_plugin::TaskRegistry::new());
    env.insert(task_registry.clone());

    // Insert the gRPC server for queue-based plugins (gRPC, NATS, etc.).
    let grpc_server = Arc::new(stepflow_grpc::StepflowGrpcServer::new(task_registry));

    // If gRPC services are multiplexed on an existing server, set the
    // address so pull plugins know where workers should connect.
    if let Some(addr) = grpc_address {
        grpc_server.set_address(addr).await;
    }

    env.insert(grpc_server);

    if !skip_plugin_init {
        stepflow_plugin::initialize_environment(&env)
            .await
            .change_context(ConfigError::Configuration)?;
    }

    Ok(env)
}
