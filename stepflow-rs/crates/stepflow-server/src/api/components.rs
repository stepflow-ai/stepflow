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

use aide::transform::TransformOperation;
use axum::{
    extract::{Query, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_core::component::ComponentInfo;
use stepflow_plugin::{Plugin as _, PluginRouterExt as _, StepflowEnvironment};

use crate::error::ErrorResponse;

/// Error from a plugin that failed component discovery.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginDiscoveryError {
    /// Plugin name from the orchestrator config.
    pub plugin: String,
    /// Human-readable error message.
    pub error: String,
}

/// Response for listing components
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListComponentsResponse {
    /// List of available components
    pub components: Vec<ComponentInfo>,
    /// True if all plugins responded successfully.
    pub complete: bool,
    /// Plugins that failed component discovery (empty when complete is true).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failed_plugins: Vec<PluginDiscoveryError>,
}

/// Query parameters for listing components
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListComponentsQuery {
    /// Whether to include schemas in the response (default: true)
    #[serde(default = "default_include_schemas")]
    pub include_schemas: bool,
}

fn default_include_schemas() -> bool {
    true
}

pub fn list_components_docs(op: TransformOperation<'_>) -> TransformOperation<'_> {
    op.id("listComponents")
        .summary("List all available components")
        .description("List all available components from registered plugins.")
        .tag("Component")
        .response_with::<400, crate::error::ErrorResponse, _>(|res| {
            res.description("Invalid query parameters")
        })
}

/// List all available components from plugins
pub async fn list_components(
    State(executor): State<Arc<StepflowEnvironment>>,
    Query(query): Query<ListComponentsQuery>,
) -> Result<Json<ListComponentsResponse>, ErrorResponse> {
    let include_schemas = query.include_schemas;

    let mut all_components = Vec::new();
    let mut failed_plugins = Vec::new();

    for (name, plugin) in executor.plugins_with_names() {
        match plugin.list_components().await {
            Ok(mut components) => {
                if !include_schemas {
                    for component in components.iter_mut() {
                        component.input_schema = None;
                        component.output_schema = None;
                    }
                }
                all_components.extend(components);
            }
            Err(e) => {
                log::warn!("Component discovery failed for plugin '{name}': {e}");
                failed_plugins.push(PluginDiscoveryError {
                    plugin: name,
                    error: format!("{e}"),
                });
            }
        }
    }

    // Sort components by their name for consistent output
    all_components.sort_by(|a, b| a.component.cmp(&b.component));

    let complete = failed_plugins.is_empty();
    Ok(Json(ListComponentsResponse {
        components: all_components,
        complete,
        failed_plugins,
    }))
}
