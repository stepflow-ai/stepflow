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
use stepflow_builtins::BuiltinPluginConfig;
use stepflow_plugin::routing::RoutingConfig;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Configuration error")]
    Configuration,
}

type Result<T> = std::result::Result<T, Report<ConfigError>>;

#[derive(Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StepflowConfig {
    /// Working directory for the configuration.
    ///
    /// If not set, this will be the directory containing the config.
    #[schema(value_type = Option<String>)]
    pub working_directory: Option<PathBuf>,
    #[schema(value_type = std::collections::HashMap<String, SupportedPluginConfig>)]
    pub plugins: IndexMap<String, SupportedPluginConfig>,
    /// Routing configuration for mapping components to plugins.
    #[serde(flatten)]
    pub routing: RoutingConfig,
    /// Storage configuration. If not specified, uses in-memory storage.
    #[serde(default)]
    pub storage_config: StorageConfig,
    /// Lease manager configuration for distributed coordination.
    /// If not specified, uses no-op (single orchestrator mode).
    #[serde(default)]
    pub lease_manager: LeaseManagerConfig,
    /// Recovery configuration for handling interrupted runs.
    #[serde(default)]
    pub recovery: RecoveryConfig,
    /// Blob API configuration.
    /// Controls whether the orchestrator serves blob endpoints and the URL workers use.
    #[serde(default)]
    pub blob_api: BlobApiConfig,
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

    /// Create a StepflowEnvironment from this configuration
    pub async fn create_environment(self) -> Result<Arc<stepflow_plugin::StepflowEnvironment>> {
        use stepflow_plugin::StepflowEnvironmentBuilder;
        use stepflow_plugin::routing::PluginRouter;

        // Create metadata store, blob store, and execution journal from configuration
        let stores = self.storage_config.create_stores().await?;

        // Create lease manager from configuration
        let lease_manager = self.lease_manager.create_lease_manager().await?;

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

        // Create environment using the builder (this also initializes all plugins)
        let env = StepflowEnvironmentBuilder::new()
            .metadata_store(stores.metadata_store)
            .blob_store(stores.blob_store)
            .execution_journal(stores.execution_journal)
            .lease_manager(lease_manager)
            .working_directory(working_directory)
            .plugin_router(plugin_router)
            .blob_api_url(self.blob_api.url)
            .build()
            .await
            .change_context(ConfigError::Configuration)?;

        Ok(env)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    /// This test generates the StepflowConfig JSON schema to schemas/stepflow-config.json.
    /// Run with: STEPFLOW_OVERWRITE_SCHEMA=1 cargo test -p stepflow-config test_schema_generation
    #[test]
    fn test_schema_generation() {
        use stepflow_core::json_schema::generate_json_schema_with_defs;

        // Generate schema using the utoipa-based utility
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
                    assert_eq!(
                        generated_schema_str, expected_schema_str,
                        "Generated schema does not match the reference schema at {}. \
                         Run 'STEPFLOW_OVERWRITE_SCHEMA=1 cargo test -p stepflow-config test_schema_generation' to update.",
                        schema_path
                    );
                }
                Err(_) => {
                    panic!(
                        "StepflowConfig schema file not found at {}. \
                         Run 'STEPFLOW_OVERWRITE_SCHEMA=1 cargo test -p stepflow-config test_schema_generation' to create it.",
                        schema_path
                    );
                }
            }
        }
    }
}
