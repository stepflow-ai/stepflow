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
mod builtin_plugin_config;
mod etcd_lease_manager_config;
mod filesystem_blob_store_config;
mod grpc_plugin_config;
mod lease_manager_config;
mod mcp_plugin_config;
mod nats_plugin_config;
mod recovery_config;
mod retry_config;
mod routing_config;
mod sql_state_store_config;
mod storage_config;
mod supported_plugin;

pub use blob_api_config::BlobApiConfig;
pub use builtin_plugin_config::BuiltinPluginConfig;
pub use etcd_lease_manager_config::EtcdLeaseManagerConfig;
pub use filesystem_blob_store_config::FilesystemBlobStoreConfig;
pub use grpc_plugin_config::GrpcPluginConfig;
pub use lease_manager_config::LeaseManagerConfig;
pub use mcp_plugin_config::McpPluginConfig;
pub use nats_plugin_config::NatsPluginConfig;
pub use recovery_config::RecoveryConfig;
pub use retry_config::{BackoffConfig, RetryConfig};
pub use routing_config::{InputCondition, RouteMatch, RouteRule, RoutingConfig};
pub use sql_state_store_config::{PgStateStoreConfig, SqlStateStoreConfig, SqliteStateStoreConfig};
pub use storage_config::{StorageConfig, StoreConfig};
pub use supported_plugin::{SupportedPlugin, SupportedPluginConfig};

use std::path::{Path, PathBuf};

use error_stack::ResultExt as _;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_with::{DefaultOnNull, serde_as};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Configuration error")]
    Configuration,
}

type Result<T> = std::result::Result<T, error_stack::Report<ConfigError>>;

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
    pub retry: RetryConfig,
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
            retry: RetryConfig::default(),
        }
    }
}

impl StepflowConfig {
    /// Load configuration from a JSON string.
    pub fn load_from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json)
            .change_context(ConfigError::Configuration)
            .attach_printable("Failed to parse config JSON")
    }

    /// Load configuration from a YAML file.
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stepflow_config_null_optional_fields() {
        // Simulates Python SDK sending config with explicit nulls for optional fields
        let json = serde_json::json!({
            "plugins": {
                "builtin": { "type": "builtin" }
            },
            "routes": {
                "/": [{ "plugin": "builtin" }]
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

    /// Generates the StepflowConfig JSON schema to schemas/stepflow-config.json.
    /// Run with: STEPFLOW_OVERWRITE_SCHEMA=1 cargo test -p stepflow-config test_schema_generation
    #[test]
    fn test_schema_generation() {
        use std::env;

        use stepflow_flow::json_schema::generate_json_schema_with_defs;

        let generated_json = generate_json_schema_with_defs::<StepflowConfig>();
        let generated_schema_str =
            serde_json::to_string_pretty(&generated_json).expect("Failed to serialize schema");

        let schema_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../schemas/stepflow-config.json"
        );

        if env::var("STEPFLOW_OVERWRITE_SCHEMA").is_ok() {
            if let Some(parent) = std::path::Path::new(schema_path).parent() {
                std::fs::create_dir_all(parent).expect("Failed to create schema directory");
            }

            std::fs::write(schema_path, &generated_schema_str)
                .expect("Failed to write updated schema");
            println!("Updated StepflowConfig schema at {}", schema_path);
        } else {
            match std::fs::read_to_string(schema_path) {
                Ok(expected_schema_str) => {
                    if generated_schema_str != expected_schema_str {
                        panic!(
                            "Generated schema does not match the reference schema at {}.\n\
                            Run 'STEPFLOW_OVERWRITE_SCHEMA=1 cargo test -p stepflow-config test_schema_generation' \
                            to update.",
                            schema_path
                        );
                    }
                }
                Err(_) => {
                    panic!(
                        "StepflowConfig schema file not found at {}.\n\
                        Run 'STEPFLOW_OVERWRITE_SCHEMA=1 cargo test -p stepflow-config test_schema_generation' \
                        to create it.",
                        schema_path
                    );
                }
            }
        }
    }
}
