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

use std::collections::HashMap;
use std::sync::Arc;

use super::Router;
use crate::routing::{RouteMatch, RouteRule, RoutingConfig};
use crate::{DynPlugin, PluginError, Result};
use error_stack::ResultExt as _;
use indexmap::IndexMap;
use stepflow_core::values::ValueRef;

/// Integrated plugin router that combines routing logic with plugin management.
///
/// This router directly resolves component paths and input data to specific plugin instances,
/// eliminating the need for separate routing rule index lookups.
pub struct PluginRouter {
    /// The underlying router for component path resolution
    router: Router,
    /// Ordered mapping from plugin names to their instances
    /// IndexMap maintains insertion order and provides both name-based and index-based access
    plugins: IndexMap<String, Arc<DynPlugin<'static>>>,
}

impl PluginRouter {
    /// Create a new builder for constructing a PluginRouter
    pub fn builder() -> PluginRouterBuilder {
        PluginRouterBuilder::new()
    }

    /// Get a plugin and resolved component name for the given component path and input data
    pub fn get_plugin_and_component(
        &self,
        component_path: &str,
        input: ValueRef,
    ) -> Result<(&Arc<DynPlugin<'static>>, String)> {
        // Route the component path to get a component router
        let component_router = self
            .router
            .route(component_path)
            .change_context(PluginError::InvalidInput)?;

        // Resolve the input to get the route rule and component name
        let (route_rule, resolved_component) = component_router
            .get_route_and_component(input)
            .change_context(PluginError::InvalidInput)?;

        log::info!(
            "Component '{component_path}' routed to '{resolved_component}' on plugin '{}'",
            route_rule.rule.plugin
        );

        // Get the plugin by index.
        let plugin = self
            .plugins
            .get_index(route_rule.plugin_index)
            .expect("Plugin index should be valid")
            .1;

        Ok((plugin, resolved_component))
    }

    /// Get all registered plugins
    /// This returns each plugin exactly once, even if it's used by multiple routing rules
    pub fn plugins(&self) -> impl Iterator<Item = &Arc<DynPlugin<'static>>> {
        self.plugins.values()
    }

    /// Get all registered plugins with their names
    /// This returns each plugin exactly once with its corresponding name
    pub fn plugins_with_names(&self) -> impl Iterator<Item = (&str, &Arc<DynPlugin<'static>>)> {
        self.plugins
            .iter()
            .map(|(name, plugin)| (name.as_str(), plugin))
    }

    /// Find all routes that could match a given plugin and component
    pub fn find_routes_for(&self, plugin: &str, component: &str) -> Vec<RouteMatch> {
        self.router.find_routes_for(plugin, component)
    }
}

/// Builder for constructing a PluginRouter
pub struct PluginRouterBuilder {
    /// Routing configuration for component path resolution
    routing_config: RoutingConfig,
    /// Ordered map from plugin name to plugin instance
    plugins: IndexMap<String, Arc<DynPlugin<'static>>>,
}

impl PluginRouterBuilder {
    fn new() -> Self {
        Self {
            routing_config: RoutingConfig {
                routes: HashMap::new(),
            },
            plugins: IndexMap::new(),
        }
    }

    /// Add a routing path with rules to the plugin router being built
    pub fn with_routing_path(mut self, path: String, rules: Vec<RouteRule>) -> Self {
        self.routing_config.routes.insert(path, rules);
        self
    }

    /// Set the complete routing configuration
    pub fn with_routing_config(mut self, config: RoutingConfig) -> Self {
        self.routing_config = config;
        self
    }

    /// Register a plugin with the given name
    pub fn register_plugin(mut self, name: String, plugin: Box<DynPlugin<'static>>) -> Self {
        self.plugins.insert(name, Arc::from(plugin));
        self
    }

    /// Build the PluginRouter
    pub fn build(self) -> Result<PluginRouter> {
        // Validate that all plugins referenced in rules exist
        for (_path, rules) in self.routing_config.routes.iter() {
            for rule in rules {
                let plugin_name = rule.plugin.as_ref();
                if !self.plugins.contains_key(plugin_name) {
                    return Err(PluginError::PluginNotFound(plugin_name.to_string()).into());
                }
            }
        }

        // Create plugin indices map for the router
        let plugin_indices: HashMap<String, usize> = self
            .plugins
            .keys()
            .enumerate()
            .map(|(index, name)| (name.clone(), index))
            .collect();

        // Create the underlying router
        let router = Router::new(self.routing_config, &plugin_indices)
            .change_context(PluginError::CreatePlugin)?;

        Ok(PluginRouter {
            router,
            plugins: self.plugins,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::rules::{RouteRule, RoutingConfig};
    use serde_json::json;

    struct TestPlugin;

    impl crate::Plugin for TestPlugin {
        async fn ensure_initialized(
            &self,
            _env: &Arc<crate::StepflowEnvironment>,
        ) -> crate::Result<()> {
            Ok(())
        }

        async fn list_components(
            &self,
        ) -> crate::Result<Vec<stepflow_core::component::ComponentInfo>> {
            Ok(vec![])
        }

        async fn component_info(
            &self,
            _component: &stepflow_core::workflow::Component,
        ) -> crate::Result<stepflow_core::component::ComponentInfo> {
            Ok(stepflow_core::component::ComponentInfo {
                component: stepflow_core::workflow::Component::from_string("test"),
                description: None,
                input_schema: None,
                output_schema: None,
            })
        }

        async fn execute(
            &self,
            _component: &stepflow_core::workflow::Component,
            _run_context: &std::sync::Arc<crate::RunContext>,
            _step: Option<&stepflow_core::workflow::StepId>,
            _input: stepflow_core::workflow::ValueRef,
        ) -> crate::Result<stepflow_core::FlowResult> {
            Ok(stepflow_core::FlowResult::Success(
                stepflow_core::workflow::ValueRef::new(json!({})),
            ))
        }
    }

    #[test]
    fn test_plugin_router_basic() {
        let test_plugin = DynPlugin::boxed(TestPlugin);

        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "/test/{component}".to_string(),
            vec![RouteRule {
                conditions: vec![],
                component_allow: None,
                component_deny: None,
                plugin: "test".into(),
                component: None,
            }],
        );

        let config = RoutingConfig { routes };

        let router = PluginRouter::builder()
            .with_routing_config(config)
            .register_plugin("test".to_string(), test_plugin)
            .build()
            .unwrap();

        let input = ValueRef::new(json!({}));
        let (_plugin, path) = router
            .get_plugin_and_component("/test/example", input)
            .unwrap();
        assert_eq!(path, "/example");
    }

    #[test]
    fn test_plugin_router_multiple_rules_same_plugin() {
        let test_plugin = DynPlugin::boxed(TestPlugin);

        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "/test/{component}".to_string(),
            vec![RouteRule {
                conditions: vec![],
                component_allow: None,
                component_deny: None,
                plugin: "test".into(),
                component: None,
            }],
        );
        routes.insert(
            "/other/{component}".to_string(),
            vec![RouteRule {
                conditions: vec![],
                component_allow: None,
                component_deny: None,
                plugin: "test".into(),
                component: None,
            }],
        );

        let config = RoutingConfig { routes };

        let router = PluginRouter::builder()
            .with_routing_config(config)
            .register_plugin("test".to_string(), test_plugin)
            .build()
            .unwrap();

        let input = ValueRef::new(json!({}));
        let (plugin1, path1) = router
            .get_plugin_and_component("/test/example", input.clone())
            .unwrap();
        let (plugin2, path2) = router
            .get_plugin_and_component("/other/example", input)
            .unwrap();

        // Both should be the same Arc instance
        assert!(Arc::ptr_eq(plugin1, plugin2));
        assert_eq!(path1, "/example");
        assert_eq!(path2, "/example");
    }

    #[test]
    fn test_plugin_router_missing_plugin() {
        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "/missing/{component}".to_string(),
            vec![RouteRule {
                conditions: vec![],
                component_allow: None,
                component_deny: None,
                plugin: "nonexistent".into(),
                component: None,
            }],
        );

        let config = RoutingConfig { routes };

        let result = PluginRouter::builder().with_routing_config(config).build();

        assert!(result.is_err());
    }
}
