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

mod blob_api_config;
mod lease_manager_config;
mod recovery_config;
mod storage_config;
mod supported_plugin;

pub use blob_api_config::BlobApiConfig;
pub use lease_manager_config::LeaseManagerConfig;
pub use recovery_config::RecoveryConfig;
pub use storage_config::{StorageConfig, StoreConfig, Stores};
pub use supported_plugin::{SupportedPlugin, SupportedPluginConfig};

use std::path::{Path, PathBuf};
use std::sync::Arc;

use error_stack::{Report, ResultExt as _};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_with::{DefaultOnNull, serde_as};
use stepflow_builtins::BuiltinPluginConfig;
use stepflow_plugin::routing::RoutingConfig;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Configuration error")]
    Configuration,
}

type Result<T> = std::result::Result<T, Report<ConfigError>>;

#[serde_as]
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StepflowConfig {
    /// Working directory for the configuration.
    ///
    /// If not set, this will be the directory containing the config.
    #[schemars(with = "Option<String>")]
    pub working_directory: Option<PathBuf>,
    #[schemars(with = "std::collections::HashMap<String, SupportedPluginConfig>")]
    pub plugins: IndexMap<String, SupportedPluginConfig>,
    /// Routing configuration for mapping components to plugins.
    #[serde(flatten)]
    pub routing: RoutingConfig,
    /// Storage configuration. If not specified, uses in-memory storage.
    #[serde(default)]
    #[serde_as(as = "DefaultOnNull")]
    pub storage_config: StorageConfig,
    /// Lease manager configuration for distributed coordination.
    /// If not specified, uses no-op (single orchestrator mode).
    #[serde(default)]
    #[serde_as(as = "DefaultOnNull")]
    pub lease_manager: LeaseManagerConfig,
    /// Recovery configuration for handling interrupted runs.
    #[serde(default)]
    #[serde_as(as = "DefaultOnNull")]
    pub recovery: RecoveryConfig,
    /// Blob API configuration.
    /// Controls whether the orchestrator serves blob endpoints and the URL workers use.
    #[serde(default)]
    #[serde_as(as = "DefaultOnNull")]
    pub blob_api: BlobApiConfig,
    /// Retry configuration.
    /// Controls backoff for all retries and the retry limit for transport errors
    /// (subprocess crash, network timeout, connection refused).
    #[serde(default)]
    #[serde_as(as = "DefaultOnNull")]
    pub retry: stepflow_core::RetryConfig,
}

impl Default for StepflowConfig {
    fn default() -> Self {
        let mut plugins = IndexMap::new();
        plugins.insert(
            "builtin".to_string(),
            SupportedPluginConfig {
                plugin: SupportedPlugin::Builtin(BuiltinPluginConfig),
            },
        );

        Self {
            working_directory: None,
            plugins,
            routing: RoutingConfig::default(),
            storage_config: StorageConfig::default(),
            lease_manager: LeaseManagerConfig::default(),
            recovery: RecoveryConfig::default(),
            blob_api: BlobApiConfig::default(),
            retry: stepflow_core::RetryConfig::default(),
        }
    }
}

impl StepflowConfig {
    /// Load configuration from a JSON string
    pub fn load_from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json)
            .change_context(ConfigError::Configuration)
            .attach_printable("Failed to parse config JSON")
    }

    /// Load configuration from a YAML file
    pub async fn load_from_file(path: &Path) -> Result<Self> {
        let contents = tokio::fs::read_to_string(path)
            .await
            .change_context(ConfigError::Configuration)
            .attach_printable_lazy(|| format!("Failed to read config file: {}", path.display()))?;

        let mut config: StepflowConfig = serde_yaml_ng::from_str(&contents)
            .change_context(ConfigError::Configuration)
            .attach_printable("Failed to parse config YAML")?;

        // Set working directory to parent of config file if not specified
        if config.working_directory.is_none() {
            config.working_directory = path.parent().map(|p| p.to_path_buf());
        }

        Ok(config)
    }

    /// Create a StepflowEnvironment from this configuration.
    ///
    /// `orchestrator_url` is the URL workers use to call back to this
    /// orchestrator (e.g., for sub-run submission, task completion).
    /// Typically resolved from `STEPFLOW_ORCHESTRATOR_URL` with a
    /// `localhost:<port>` fallback by the caller.
    ///
    /// If `orchestrator_id` is provided, it will be stored in the environment
    /// for distributed lease management (acquire/release during run execution).
    ///
    /// If `grpc_address` is provided, gRPC services are assumed to be
    /// multiplexed on an existing server at that address. Pull plugins will
    /// use this address instead of starting a standalone gRPC server.
    pub async fn create_environment(
        self,
        orchestrator_id: Option<stepflow_state::OrchestratorId>,
        orchestrator_url: Option<String>,
        grpc_address: Option<String>,
    ) -> Result<Arc<stepflow_plugin::StepflowEnvironment>> {
        use stepflow_core::StepflowEnvironment;
        use stepflow_plugin::routing::PluginRouter;
        use stepflow_plugin::{
            BlobApiUrl, ExecutionConfig, OrchestratorServiceUrl, initialize_environment,
        };

        // Create metadata store, blob store, and execution journal from configuration
        let stores = self.storage_config.create_stores().await?;

        // Create lease manager from configuration, using the recovery TTL
        let lease_ttl = std::time::Duration::from_secs(self.recovery.lease_ttl_secs);
        let lease_manager = self.lease_manager.create_lease_manager(lease_ttl).await?;

        let working_directory = self
            .working_directory
            .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

        // Build the plugin router
        log::info!("Routing Config: {:?}", self.routing);
        let mut plugin_router_builder = PluginRouter::builder().with_routing_config(self.routing);

        // Register plugins from IndexMap
        for (plugin_name, plugin_config) in self.plugins {
            let plugin = plugin_config
                .instantiate(&working_directory)
                .await
                .attach_printable_lazy(|| {
                    format!("Failed to instantiate plugin for '{plugin_name}'")
                })?;
            plugin_router_builder = plugin_router_builder.register_plugin(plugin_name, plugin);
        }

        let plugin_router = plugin_router_builder
            .build()
            .change_context(ConfigError::Configuration)?;

        // Build the environment directly
        let env = Arc::new(StepflowEnvironment::new());
        env.insert(stores.metadata_store);
        env.insert(stores.blob_store);
        env.insert(stores.execution_journal);
        env.insert(stores.checkpoint_store);
        env.insert(lease_manager);
        env.insert(working_directory);
        env.insert(Arc::new(plugin_router) as Arc<PluginRouter>);
        let blob_threshold = self.blob_api.effective_blob_threshold();
        env.insert(BlobApiUrl::new(self.blob_api.url, blob_threshold));
        env.insert(OrchestratorServiceUrl::new(orchestrator_url));
        env.insert(ExecutionConfig {
            checkpoint_interval: self.recovery.checkpoint_interval,
        });
        env.insert(self.retry);
        if let Some(id) = orchestrator_id {
            env.insert(id);
        }

        // Insert the shared task registry for result delivery.
        let task_registry = Arc::new(stepflow_plugin::TaskRegistry::new());
        env.insert(task_registry.clone());

        // Insert the gRPC server for pull-based plugins.
        let grpc_server = Arc::new(stepflow_grpc::StepflowGrpcServer::new(task_registry));

        // If gRPC services are multiplexed on an existing server, tell the
        // gRPC server so pull plugins use that address instead of starting a
        // standalone server.
        if let Some(addr) = grpc_address {
            grpc_server.set_multiplexed_address(addr).await;
        }

        env.insert(grpc_server);

        initialize_environment(&env)
            .await
            .change_context(ConfigError::Configuration)?;

        Ok(env)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This test generates the StepflowConfig JSON schema to schemas/stepflow-config.json.
    /// Run with: STEPFLOW_OVERWRITE_SCHEMA=1 cargo test -p stepflow-config --features etcd test_schema_generation
    ///
    /// The `etcd` feature gate ensures the reference schema always includes all
    /// optional types regardless of which features a dependent crate enables.
    #[test]
    #[cfg(feature = "etcd")]
    fn test_schema_generation() {
        use std::env;

        use stepflow_core::json_schema::generate_json_schema_with_defs;

        // Generate schema using schemars
        let generated_json = generate_json_schema_with_defs::<StepflowConfig>();
        let generated_schema_str =
            serde_json::to_string_pretty(&generated_json).expect("Failed to serialize schema");

        // Get path to schemas/stepflow-config.json
        let schema_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../schemas/stepflow-config.json"
        );

        if env::var("STEPFLOW_OVERWRITE_SCHEMA").is_ok() {
            // Ensure the directory exists
            if let Some(parent) = std::path::Path::new(schema_path).parent() {
                std::fs::create_dir_all(parent).expect("Failed to create schema directory");
            }

            std::fs::write(schema_path, &generated_schema_str)
                .expect("Failed to write updated schema");
            println!("Updated StepflowConfig schema at {}", schema_path);
        } else {
            // Try to read the existing schema for comparison
            match std::fs::read_to_string(schema_path) {
                Ok(expected_schema_str) => {
                    if generated_schema_str != expected_schema_str {
                        panic!(
                            "Generated schema does not match the reference schema at {}.\n\
                            Run 'STEPFLOW_OVERWRITE_SCHEMA=1 cargo test -p stepflow-config test_schema_generation --all-features' \
                            to update.",
                            schema_path
                        );
                    }
                }
                Err(_) => {
                    panic!(
                        "StepflowConfig schema file not found at {}.\n\
                        Run 'STEPFLOW_OVERWRITE_SCHEMA=1 cargo test -p stepflow-config test_schema_generation --all-features' \
                        to create it.",
                        schema_path
                    );
                }
            }
        }
    }

    #[test]
    fn test_stepflow_config_null_optional_fields() {
        // Simulates Python SDK sending config with explicit nulls for optional fields
        let json = serde_json::json!({
            "plugins": {
                "builtin": { "type": "builtin" }
            },
            "routes": {
                "/{*component}": [{ "plugin": "builtin" }]
            },
            "workingDirectory": null,
            "storageConfig": null,
            "leaseManager": null,
            "recovery": null,
            "blobApi": null,
            "retry": null,
        });
        let config: StepflowConfig = serde_json::from_value(json).unwrap();
        assert!(config.working_directory.is_none());
        // All defaulted fields should use their defaults when null
        assert!(matches!(
            config.storage_config,
            StorageConfig::Simple(StoreConfig::InMemory)
        ));
        assert!(matches!(config.lease_manager, LeaseManagerConfig::NoOp));
        assert!(config.recovery.enabled);
        assert!(config.blob_api.enabled);
        assert_eq!(config.retry.transport_max_retries, 3);
    }
}
