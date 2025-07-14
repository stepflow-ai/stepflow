// Licensed to the Apache Software Foundation (ASF) under one or more contributor license agreements.
// See the NOTICE file distributed with this work for additional information regarding copyright
// ownership.  The ASF licenses this file to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance with the License.  You may obtain a
// copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied.  See the License for the specific language governing permissions and limitations under
// the License.

use std::collections::HashMap;
use std::sync::Arc;

use super::Router;
use crate::routing::RoutingRule;
use crate::{DynPlugin, PluginError, Result};
use error_stack::ResultExt as _;
use stepflow_core::workflow::ValueRef;

/// Integrated plugin router that combines routing logic with plugin management.
///
/// This router directly resolves component paths and input data to specific plugin instances,
/// eliminating the need for separate routing rule index lookups.
pub struct PluginRouter {
    /// The underlying router for component path resolution
    router: Router,
    /// Plugin instances indexed by routing rule index
    /// Each routing rule index maps directly to a plugin instance
    plugins: Vec<Arc<DynPlugin<'static>>>,
    /// Unique plugin instances (no duplicates)
    /// Used for initialization to ensure each plugin is only initialized once
    unique_plugins: Vec<Arc<DynPlugin<'static>>>,
}

impl PluginRouter {
    /// Create a new builder for constructing a PluginRouter
    pub fn builder() -> PluginRouterBuilder {
        PluginRouterBuilder::new()
    }

    /// Get a plugin for the given component path and input data
    pub fn get_plugin(
        &self,
        component_path: &str,
        input: ValueRef,
    ) -> Result<&Arc<DynPlugin<'static>>> {
        // NOTE: If the component router step is expensive, we could instead
        // have this return the `ComponentRouter` and allow the caller to store
        // that. This would allow determining if there is a static route, as
        // well as re-using the path based routing.

        // Route the component path to get a component router
        let component_router = self
            .router
            .route(component_path.to_string())
            .change_context(PluginError::InvalidInput)?;

        // Resolve the input to get the rule index
        let rule_index = component_router
            .route_input(input)
            .change_context(PluginError::InvalidInput)?;

        // Get the plugin directly by rule index
        self.plugins
            .get(rule_index)
            .ok_or(PluginError::InvalidRuleIndex(rule_index))
            .map_err(|e| e.into())
    }

    /// Get all registered plugins
    /// This returns each plugin exactly once, even if it's used by multiple routing rules
    pub fn plugins(&self) -> impl Iterator<Item = &Arc<DynPlugin<'static>>> {
        self.unique_plugins.iter()
    }
}

/// Builder for constructing a PluginRouter
pub struct PluginRouterBuilder {
    /// Routing rules for component path resolution
    routing_rules: Vec<RoutingRule>,
    /// Map from plugin name to plugin instance
    plugins: HashMap<String, Arc<DynPlugin<'static>>>,
}

impl PluginRouterBuilder {
    fn new() -> Self {
        Self {
            routing_rules: Vec::new(),
            plugins: HashMap::new(),
        }
    }

    /// Add a routing rule to the plugin router being built
    pub fn with_routing_rule(mut self, rule: RoutingRule) -> Self {
        self.routing_rules.push(rule);
        self
    }

    /// Add multiple routing rules to the plugin router being built
    pub fn with_routing_rules(mut self, rules: impl IntoIterator<Item = RoutingRule>) -> Self {
        self.routing_rules.extend(rules);
        self
    }

    /// Register a plugin with the given name
    pub fn register_plugin(mut self, name: String, plugin: Box<DynPlugin<'static>>) -> Self {
        self.plugins.insert(name, Arc::from(plugin));
        self
    }

    /// Build the PluginRouter
    pub fn build(self) -> Result<PluginRouter> {
        // Expand the plugins HashMap into a Vec where each index corresponds to a routing rule
        let mut plugins = Vec::new();
        for rule in self.routing_rules.iter() {
            let plugin_name = rule.target.as_ref();
            if let Some(plugin) = self.plugins.get(plugin_name) {
                plugins.push(plugin.clone());
            } else {
                return Err(PluginError::PluginNotFound(plugin_name.to_string()).into());
            }
        }

        // Create a vector of unique plugins (no duplicates)
        let unique_plugins: Vec<Arc<DynPlugin<'static>>> = self.plugins.into_values().collect();

        // Create the underlying router
        let router = Router::new(self.routing_rules).change_context(PluginError::CreatePlugin)?;

        Ok(PluginRouter {
            router,
            plugins,
            unique_plugins,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::rules::{MatchRule, RoutingRule};
    use serde_json::json;

    struct TestPlugin;

    impl crate::Plugin for TestPlugin {
        async fn init(&self, _context: &Arc<dyn crate::Context>) -> crate::Result<()> {
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
            _context: crate::ExecutionContext,
            _input: stepflow_core::workflow::ValueRef,
        ) -> crate::Result<stepflow_core::FlowResult> {
            Ok(stepflow_core::FlowResult::Success {
                result: stepflow_core::workflow::ValueRef::new(json!({})),
            })
        }
    }

    #[test]
    fn test_plugin_router_basic() {
        let test_plugin = DynPlugin::boxed(TestPlugin);

        let rules = vec![RoutingRule {
            match_rule: vec![MatchRule {
                component: "/test/*".to_string(),
                input: vec![],
            }],
            target: "test".into(),
        }];

        let router = PluginRouter::builder()
            .with_routing_rules(rules)
            .register_plugin("test".to_string(), test_plugin)
            .build()
            .unwrap();

        let input = ValueRef::new(json!({}));
        let _plugin = router.get_plugin("/test/example", input).unwrap();
        // Just verify we can get the plugin back without panicking
    }

    #[test]
    fn test_plugin_router_multiple_rules_same_plugin() {
        let test_plugin = DynPlugin::boxed(TestPlugin);

        let rules = vec![
            RoutingRule {
                match_rule: vec![MatchRule {
                    component: "/test/*".to_string(),
                    input: vec![],
                }],
                target: "test".into(),
            },
            RoutingRule {
                match_rule: vec![MatchRule {
                    component: "/other/*".to_string(),
                    input: vec![],
                }],
                target: "test".into(),
            },
        ];

        let router = PluginRouter::builder()
            .with_routing_rules(rules)
            .register_plugin("test".to_string(), test_plugin)
            .build()
            .unwrap();

        let input = ValueRef::new(json!({}));
        let plugin1 = router.get_plugin("/test/example", input.clone()).unwrap();
        let plugin2 = router.get_plugin("/other/example", input).unwrap();

        // Both should be the same Arc instance
        assert!(Arc::ptr_eq(plugin1, plugin2));
    }

    #[test]
    fn test_plugin_router_missing_plugin() {
        let rules = vec![RoutingRule {
            match_rule: vec![MatchRule {
                component: "/missing/*".to_string(),
                input: vec![],
            }],
            target: "nonexistent".into(),
        }];

        let result = PluginRouter::builder().with_routing_rules(rules).build();

        assert!(result.is_err());
    }
}
