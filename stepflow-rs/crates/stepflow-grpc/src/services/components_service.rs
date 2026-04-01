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

use std::sync::Arc;

use stepflow_plugin::{Plugin as _, PluginRouterExt as _, StepflowEnvironment};
use tonic::{Request, Response, Status};

use crate::proto::stepflow::v1::components_service_server::ComponentsService;
use crate::proto::stepflow::v1::{
    ComponentInfoEntry, ListRegisteredComponentsRequest, ListRegisteredComponentsResponse,
    PluginDiscoveryError,
};

/// gRPC implementation of [ComponentsService].
#[derive(Debug)]
pub struct ComponentsServiceImpl {
    env: Arc<StepflowEnvironment>,
}

impl ComponentsServiceImpl {
    pub fn new(env: Arc<StepflowEnvironment>) -> Self {
        Self { env }
    }
}

#[tonic::async_trait]
impl ComponentsService for ComponentsServiceImpl {
    async fn list_registered_components(
        &self,
        request: Request<ListRegisteredComponentsRequest>,
    ) -> Result<Response<ListRegisteredComponentsResponse>, Status> {
        let req = request.into_inner();
        let include_schemas = !req.exclude_schemas;

        let mut all_components: Vec<(String, _)> = Vec::new();
        let mut failed_plugins = Vec::new();

        for (name, plugin) in self.env.plugins_with_names() {
            match plugin.list_components().await {
                Ok(mut components) => {
                    // Update the routing trie if component registrations changed
                    // (e.g., a new gRPC worker connected since the last poll).
                    if let Err(e) = self.env.update_plugin_components(&name, components.clone()) {
                        log::warn!("Failed to update routing trie for plugin '{name}': {e}");
                    }
                    if !include_schemas {
                        for c in components.iter_mut() {
                            c.input_schema = None;
                            c.output_schema = None;
                        }
                    }
                    all_components.extend(components.into_iter().map(|c| (name.clone(), c)));
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

        all_components.sort_by(|a, b| a.1.component.cmp(&b.1.component));

        let entries = all_components
            .into_iter()
            .map(|(plugin_name, c)| {
                let input_schema = c.input_schema.and_then(|s| {
                    serde_json::to_value(&s)
                        .ok()
                        .and_then(|v| serde_json::from_value(v).ok())
                });
                let output_schema = c.output_schema.and_then(|s| {
                    serde_json::to_value(&s)
                        .ok()
                        .and_then(|v| serde_json::from_value(v).ok())
                });
                ComponentInfoEntry {
                    component_id: c.component.to_string(),
                    description: c.description,
                    input_schema,
                    output_schema,
                    path: c.component.path().to_string(),
                    plugin: plugin_name,
                }
            })
            .collect();

        let complete = failed_plugins.is_empty();
        Ok(Response::new(ListRegisteredComponentsResponse {
            components: entries,
            complete,
            failed_plugins,
        }))
    }
}
