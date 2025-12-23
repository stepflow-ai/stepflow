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

//! Component schema provider for type checking.
//!
//! This module provides a `ComponentSchemaProvider` implementation that queries
//! component servers via the plugin system to get schema information.

use std::collections::HashMap;

use std::sync::Arc;
use stepflow_core::component::ComponentInfo;
use stepflow_core::workflow::Component;
use stepflow_plugin::routing::PluginRouter;
use stepflow_plugin::{DynPlugin, Plugin as _};
use stepflow_typecheck::{ComponentSchemaProvider, Type};

/// A schema provider that caches component schemas from the plugin system.
///
/// This provider pre-fetches component schemas at construction time by querying
/// each plugin for its available components. The cached schemas are then used
/// during type checking.
pub struct PluginSchemaProvider {
    /// Cached component schemas, keyed by component path (e.g., "/python/my_func")
    schemas: HashMap<String, CachedSchema>,
}

/// Cached schema information for a component.
#[derive(Clone)]
struct CachedSchema {
    input_schema: Option<Type>,
    output_schema: Option<Type>,
}

impl PluginSchemaProvider {
    /// Create a new schema provider by querying all plugins for component schemas.
    ///
    /// This pre-fetches component information from all configured plugins so that
    /// type checking can proceed synchronously. Schemas are stored under their
    /// routed paths (e.g., `/builtin/put_blob`) so they can be looked up using
    /// the component paths used in workflows.
    pub async fn from_router(router: &PluginRouter) -> Self {
        let mut schemas = HashMap::new();

        // Iterate through all plugins and get their component info
        for (plugin_name, plugin) in router.plugins_with_names() {
            let plugin: &Arc<DynPlugin<'static>> = plugin;
            // List all components from this plugin
            let component_infos = match plugin.list_components().await {
                Ok(infos) => infos,
                Err(e) => {
                    log::warn!("Failed to list components from plugin: {}", e);
                    continue;
                }
            };

            // Cache schema from each component info, storing by routed path
            for info in component_infos {
                let cached = CachedSchema::from_info(&info);

                // Get all routes that expose this component
                let routes = router.find_routes_for(plugin_name, info.component.path());

                if routes.is_empty() {
                    // Fall back to native path if no routes found
                    log::debug!(
                        "No routes found for component {}, storing under native path",
                        info.component.path()
                    );
                    schemas.insert(info.component.path().to_string(), cached);
                } else {
                    // Store under all resolved paths
                    for route in routes {
                        log::debug!(
                            "Storing schema for {} under routed path {}",
                            info.component.path(),
                            route.resolved_path
                        );
                        schemas.insert(route.resolved_path, cached.clone());
                    }
                }
            }
        }

        PluginSchemaProvider { schemas }
    }

    /// Create an empty schema provider (treats all components as Any -> Any).
    #[allow(dead_code)]
    pub fn empty() -> Self {
        PluginSchemaProvider {
            schemas: HashMap::new(),
        }
    }
}

impl CachedSchema {
    fn from_info(info: &ComponentInfo) -> Self {
        CachedSchema {
            input_schema: info.input_schema.clone().map(Type::Schema),
            output_schema: info.output_schema.clone().map(Type::Schema),
        }
    }
}

impl ComponentSchemaProvider for PluginSchemaProvider {
    fn get_input_schema(&self, component: &Component) -> Option<Type> {
        self.schemas
            .get(component.path())
            .and_then(|c| c.input_schema.clone())
    }

    fn get_output_schema(&self, component: &Component) -> Option<Type> {
        self.schemas
            .get(component.path())
            .and_then(|c| c.output_schema.clone())
    }

    fn infer_output_schema(&self, component: &Component, _input_schema: &Type) -> Option<Type> {
        // For now, return the static output schema.
        // In the future, this could call the components/infer_schema protocol
        // to get dynamic output schema based on the input.
        self.get_output_schema(component)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_provider() {
        let provider = PluginSchemaProvider::empty();
        let component = Component::from_string("/test/component");

        assert!(provider.get_input_schema(&component).is_none());
        assert!(provider.get_output_schema(&component).is_none());
        assert!(
            provider
                .infer_output_schema(&component, &Type::Any)
                .is_none()
        );
    }
}
