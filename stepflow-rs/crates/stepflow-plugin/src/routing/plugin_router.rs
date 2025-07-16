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

use super::{Router, PathTransformer};
use crate::routing::RoutingRule;
use crate::{DynPlugin, PluginError, Result, Plugin};
use error_stack::ResultExt as _;
use stepflow_core::workflow::{Component, ValueRef};

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
    /// Path transformers indexed by routing rule index
    /// Used to transform component paths before sending to plugins
    transformers: Vec<PathTransformer>,
    /// Original routing rules for component listing and reverse transformation
    routing_rules: Vec<RoutingRule>,
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
        let (plugin, _) = self.get_plugin_with_transformed_path(component_path, input)?;
        Ok(plugin)
    }

    /// Get a plugin and transformed component path for the given component path and input data
    pub fn get_plugin_with_transformed_path(
        &self,
        component_path: &str,
        input: ValueRef,
    ) -> Result<(&Arc<DynPlugin<'static>>, Component)> {
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
        let plugin = self.plugins
            .get(rule_index)
            .ok_or(PluginError::InvalidRuleIndex(rule_index))?;

        // Transform the component path using the corresponding transformer
        let transformer = self.transformers
            .get(rule_index)
            .ok_or(PluginError::InvalidRuleIndex(rule_index))?;
        
        let transformed_path = transformer
            .transform_to_plugin(component_path)
            .change_context(PluginError::InvalidInput)?;
        
        let transformed_component = Component::from_string(&transformed_path);

        Ok((plugin, transformed_component))
    }

    /// Get all registered plugins
    /// This returns each plugin exactly once, even if it's used by multiple routing rules
    pub fn plugins(&self) -> impl Iterator<Item = &Arc<DynPlugin<'static>>> {
        self.unique_plugins.iter()
    }

    /// List components with routing context applied
    /// This method queries all plugins for their components and applies:
    /// 1. Component filtering based on routing rules
    /// 2. Reverse path transformation to show components as they appear through routing
    pub async fn list_components_with_routing(&self) -> Result<Vec<stepflow_core::component::ComponentInfo>> {
        use crate::routing::{ComponentFilter, PathTransformer};
        use stepflow_core::component::ComponentInfo;
        
        let mut all_components = Vec::new();
        
        // Process each routing rule
        for (rule_index, rule) in self.routing_rules.iter().enumerate() {
            // Get the plugin for this rule
            let plugin = self.plugins.get(rule_index).ok_or(PluginError::InvalidRuleIndex(rule_index))?;
            
            // Get all components from this plugin
            let plugin_components = plugin.list_components().await?;
            
            // Create component filter for this rule
            let match_pattern = &rule.match_rule[0].component; // Use first match pattern
            let routing_target = rule.target.as_routing_target(match_pattern);
            let component_filter = ComponentFilter::from_target(&routing_target)?;
            let path_transformer = PathTransformer::from_target(&routing_target);
            
            // Filter and transform components
            for component_info in plugin_components {
                // Apply component filter
                if !component_filter.matches_component(&component_info.component) {
                    continue;
                }
                
                // Apply reverse path transformation
                let transformed_path = path_transformer.transform_from_plugin(component_info.component.path_string());
                let transformed_component = Component::from_string(&transformed_path);
                
                // Create new component info with transformed path
                let transformed_info = ComponentInfo {
                    component: transformed_component,
                    description: component_info.description,
                    input_schema: component_info.input_schema,
                    output_schema: component_info.output_schema,
                };
                
                all_components.push(transformed_info);
            }
        }
        
        // Remove duplicates and sort
        all_components.sort_by(|a, b| a.component.to_string().cmp(&b.component.to_string()));
        all_components.dedup_by(|a, b| a.component == b.component);
        
        Ok(all_components)
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
        let mut transformers = Vec::new();
        
        for rule in self.routing_rules.iter() {
            let plugin_name = rule.target.plugin();
            if let Some(plugin) = self.plugins.get(plugin_name) {
                plugins.push(plugin.clone());
            } else {
                return Err(PluginError::PluginNotFound(plugin_name.to_string()).into());
            }
            
            // Create path transformer for each rule
            // For simple targets, auto-detect strip segments from the first match pattern
            let match_pattern = &rule.match_rule[0].component; // Use first match pattern for auto-detection
            let routing_target = rule.target.as_routing_target(match_pattern);
            let transformer = PathTransformer::from_target(&routing_target);
            transformers.push(transformer);
        }

        // Create a vector of unique plugins (no duplicates)
        let unique_plugins: Vec<Arc<DynPlugin<'static>>> = self.plugins.into_values().collect();

        // Create the underlying router
        let router = Router::new(self.routing_rules.clone()).change_context(PluginError::CreatePlugin)?;

        Ok(PluginRouter {
            router,
            plugins,
            unique_plugins,
            transformers,
            routing_rules: self.routing_rules,
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

    // Component listing tests
    #[tokio::test]
    async fn test_component_listing_with_path_transformation() {
        use crate::routing::rules::{RoutingTarget, Target};
        use stepflow_core::component::ComponentInfo;
        use stepflow_core::workflow::Component;

        // Create a mock plugin that reports components with simple names
        struct MockPlugin;
        
        impl crate::Plugin for MockPlugin {
            async fn init(&self, _context: &Arc<dyn crate::Context>) -> crate::Result<()> {
                Ok(())
            }

            async fn list_components(&self) -> crate::Result<Vec<ComponentInfo>> {
                Ok(vec![
                    ComponentInfo {
                        component: Component::from_string("/udf"),
                        description: Some("User-defined function".to_string()),
                        input_schema: None,
                        output_schema: None,
                    },
                    ComponentInfo {
                        component: Component::from_string("/analyzer"),
                        description: Some("Data analyzer".to_string()),
                        input_schema: None,
                        output_schema: None,
                    },
                    ComponentInfo {
                        component: Component::from_string("/debug_tool"),
                        description: Some("Debug tool".to_string()),
                        input_schema: None,
                        output_schema: None,
                    },
                ])
            }

            async fn component_info(&self, component: &Component) -> crate::Result<ComponentInfo> {
                match component.path_string() {
                    "/udf" => Ok(ComponentInfo {
                        component: component.clone(),
                        description: Some("User-defined function".to_string()),
                        input_schema: None,
                        output_schema: None,
                    }),
                    "/analyzer" => Ok(ComponentInfo {
                        component: component.clone(),
                        description: Some("Data analyzer".to_string()),
                        input_schema: None,
                        output_schema: None,
                    }),
                    "/debug_tool" => Ok(ComponentInfo {
                        component: component.clone(),
                        description: Some("Debug tool".to_string()),
                        input_schema: None,
                        output_schema: None,
                    }),
                    _ => Err(crate::PluginError::UnknownComponent(component.clone()).into()),
                }
            }

            async fn execute(
                &self,
                _component: &Component,
                _context: crate::ExecutionContext,
                _input: stepflow_core::values::ValueRef,
            ) -> crate::Result<stepflow_core::FlowResult> {
                Ok(stepflow_core::FlowResult::Success {
                    result: stepflow_core::values::ValueRef::new(serde_json::json!({"mock": "result"})),
                })
            }
        }

        // Create routing rules with path transformation and component filtering
        let rules = vec![
            RoutingRule {
                match_rule: vec![MatchRule {
                    component: "/python/*".to_string(),
                    input: vec![],
                }],
                target: Target::Complex(RoutingTarget {
                    plugin: "python".to_string(),
                    strip_segments: vec!["python".to_string()],
                    components: Some(vec!["udf".to_string(), "analyzer".to_string()]),
                    exclude_components: Some(vec!["debug_*".to_string()]),
                }),
            },
        ];

        let plugin_router = PluginRouter::builder()
            .with_routing_rules(rules)
            .register_plugin("python".to_string(), DynPlugin::boxed(MockPlugin))
            .build()
            .unwrap();

        // Test component listing with routing
        let components = plugin_router.list_components_with_routing().await.unwrap();

        // Should have 2 components: udf and analyzer (debug_tool filtered out)
        assert_eq!(components.len(), 2);

        // Check that paths are transformed correctly
        let component_paths: Vec<String> = components.iter().map(|c| c.component.to_string()).collect();
        assert!(component_paths.contains(&"/python/udf".to_string()));
        assert!(component_paths.contains(&"/python/analyzer".to_string()));
        assert!(!component_paths.contains(&"/python/debug_tool".to_string()));

        // Check descriptions are preserved
        let udf_component = components.iter().find(|c| c.component.to_string() == "/python/udf").unwrap();
        assert_eq!(udf_component.description, Some("User-defined function".to_string()));
    }

    #[tokio::test]
    async fn test_component_listing_with_multiple_segments() {
        use crate::routing::rules::{RoutingTarget, Target};
        use stepflow_core::component::ComponentInfo;
        use stepflow_core::workflow::Component;

        struct MockPlugin;
        
        impl crate::Plugin for MockPlugin {
            async fn init(&self, _context: &Arc<dyn crate::Context>) -> crate::Result<()> {
                Ok(())
            }

            async fn list_components(&self) -> crate::Result<Vec<ComponentInfo>> {
                Ok(vec![
                    ComponentInfo {
                        component: Component::from_string("/classifier"),
                        description: Some("ML classifier".to_string()),
                        input_schema: None,
                        output_schema: None,
                    },
                    ComponentInfo {
                        component: Component::from_string("/regressor"),
                        description: Some("ML regressor".to_string()),
                        input_schema: None,
                        output_schema: None,
                    },
                ])
            }

            async fn component_info(&self, component: &Component) -> crate::Result<ComponentInfo> {
                match component.path_string() {
                    "/classifier" => Ok(ComponentInfo {
                        component: component.clone(),
                        description: Some("ML classifier".to_string()),
                        input_schema: None,
                        output_schema: None,
                    }),
                    "/regressor" => Ok(ComponentInfo {
                        component: component.clone(),
                        description: Some("ML regressor".to_string()),
                        input_schema: None,
                        output_schema: None,
                    }),
                    _ => Err(crate::PluginError::UnknownComponent(component.clone()).into()),
                }
            }

            async fn execute(
                &self,
                _component: &Component,
                _context: crate::ExecutionContext,
                _input: stepflow_core::values::ValueRef,
            ) -> crate::Result<stepflow_core::FlowResult> {
                Ok(stepflow_core::FlowResult::Success {
                    result: stepflow_core::values::ValueRef::new(serde_json::json!({"mock": "result"})),
                })
            }
        }

        // Create routing rules with multiple strip segments
        let rules = vec![
            RoutingRule {
                match_rule: vec![MatchRule {
                    component: "/ai/python/ml/*".to_string(),
                    input: vec![],
                }],
                target: Target::Complex(RoutingTarget {
                    plugin: "ml".to_string(),
                    strip_segments: vec!["ai".to_string(), "python".to_string(), "ml".to_string()],
                    components: None,
                    exclude_components: None,
                }),
            },
        ];

        let plugin_router = PluginRouter::builder()
            .with_routing_rules(rules)
            .register_plugin("ml".to_string(), DynPlugin::boxed(MockPlugin))
            .build()
            .unwrap();

        // Test component listing with routing
        let components = plugin_router.list_components_with_routing().await.unwrap();

        // Should have 2 components with the full path prefix
        assert_eq!(components.len(), 2);

        let component_paths: Vec<String> = components.iter().map(|c| c.component.to_string()).collect();
        assert!(component_paths.contains(&"/ai/python/ml/classifier".to_string()));
        assert!(component_paths.contains(&"/ai/python/ml/regressor".to_string()));
    }

    #[tokio::test]
    async fn test_component_listing_with_include_exclude_filters() {
        use crate::routing::rules::{RoutingTarget, Target};
        use stepflow_core::component::ComponentInfo;
        use stepflow_core::workflow::Component;

        struct MockPlugin;
        
        impl crate::Plugin for MockPlugin {
            async fn init(&self, _context: &Arc<dyn crate::Context>) -> crate::Result<()> {
                Ok(())
            }

            async fn list_components(&self) -> crate::Result<Vec<ComponentInfo>> {
                Ok(vec![
                    ComponentInfo {
                        component: Component::from_string("/ml_classifier"),
                        description: Some("ML classifier".to_string()),
                        input_schema: None,
                        output_schema: None,
                    },
                    ComponentInfo {
                        component: Component::from_string("/ml_regressor"),
                        description: Some("ML regressor".to_string()),
                        input_schema: None,
                        output_schema: None,
                    },
                    ComponentInfo {
                        component: Component::from_string("/ml_debug_tool"),
                        description: Some("Debug tool".to_string()),
                        input_schema: None,
                        output_schema: None,
                    },
                    ComponentInfo {
                        component: Component::from_string("/data_processor"),
                        description: Some("Data processor".to_string()),
                        input_schema: None,
                        output_schema: None,
                    },
                    ComponentInfo {
                        component: Component::from_string("/other_tool"),
                        description: Some("Other tool".to_string()),
                        input_schema: None,
                        output_schema: None,
                    },
                ])
            }

            async fn component_info(&self, component: &Component) -> crate::Result<ComponentInfo> {
                // Simple implementation for testing
                Ok(ComponentInfo {
                    component: component.clone(),
                    description: Some("Mock component".to_string()),
                    input_schema: None,
                    output_schema: None,
                })
            }

            async fn execute(
                &self,
                _component: &Component,
                _context: crate::ExecutionContext,
                _input: stepflow_core::values::ValueRef,
            ) -> crate::Result<stepflow_core::FlowResult> {
                Ok(stepflow_core::FlowResult::Success {
                    result: stepflow_core::values::ValueRef::new(serde_json::json!({"mock": "result"})),
                })
            }
        }

        // Create routing rules with include and exclude filters
        let rules = vec![
            RoutingRule {
                match_rule: vec![MatchRule {
                    component: "/ai/*".to_string(),
                    input: vec![],
                }],
                target: Target::Complex(RoutingTarget {
                    plugin: "ai".to_string(),
                    strip_segments: vec!["ai".to_string()],
                    components: Some(vec!["ml_*".to_string(), "data_*".to_string()]),
                    exclude_components: Some(vec!["*_debug_*".to_string()]),
                }),
            },
        ];

        let plugin_router = PluginRouter::builder()
            .with_routing_rules(rules)
            .register_plugin("ai".to_string(), DynPlugin::boxed(MockPlugin))
            .build()
            .unwrap();

        // Test component listing with routing
        let components = plugin_router.list_components_with_routing().await.unwrap();

        // Should have 3 components: ml_classifier, ml_regressor, data_processor
        // ml_debug_tool excluded by exclude filter, other_tool excluded by include filter
        assert_eq!(components.len(), 3);

        let component_paths: Vec<String> = components.iter().map(|c| c.component.to_string()).collect();
        assert!(component_paths.contains(&"/ai/ml_classifier".to_string()));
        assert!(component_paths.contains(&"/ai/ml_regressor".to_string()));
        assert!(component_paths.contains(&"/ai/data_processor".to_string()));
        assert!(!component_paths.contains(&"/ai/ml_debug_tool".to_string()));
        assert!(!component_paths.contains(&"/ai/other_tool".to_string()));
    }

    #[tokio::test]
    async fn test_component_listing_with_multiple_routes_same_plugin() {
        use crate::routing::rules::{RoutingTarget, Target};
        use stepflow_core::component::ComponentInfo;
        use stepflow_core::workflow::Component;

        struct MockPlugin;
        
        impl crate::Plugin for MockPlugin {
            async fn init(&self, _context: &Arc<dyn crate::Context>) -> crate::Result<()> {
                Ok(())
            }

            async fn list_components(&self) -> crate::Result<Vec<ComponentInfo>> {
                Ok(vec![
                    ComponentInfo {
                        component: Component::from_string("/udf"),
                        description: Some("User-defined function".to_string()),
                        input_schema: None,
                        output_schema: None,
                    },
                    ComponentInfo {
                        component: Component::from_string("/analyzer"),
                        description: Some("Data analyzer".to_string()),
                        input_schema: None,
                        output_schema: None,
                    },
                    ComponentInfo {
                        component: Component::from_string("/transformer"),
                        description: Some("Data transformer".to_string()),
                        input_schema: None,
                        output_schema: None,
                    },
                ])
            }

            async fn component_info(&self, component: &Component) -> crate::Result<ComponentInfo> {
                Ok(ComponentInfo {
                    component: component.clone(),
                    description: Some("Mock component".to_string()),
                    input_schema: None,
                    output_schema: None,
                })
            }

            async fn execute(
                &self,
                _component: &Component,
                _context: crate::ExecutionContext,
                _input: stepflow_core::values::ValueRef,
            ) -> crate::Result<stepflow_core::FlowResult> {
                Ok(stepflow_core::FlowResult::Success {
                    result: stepflow_core::values::ValueRef::new(serde_json::json!({"mock": "result"})),
                })
            }
        }

        // Create multiple routing rules pointing to the same plugin with different filters
        let rules = vec![
            RoutingRule {
                match_rule: vec![MatchRule {
                    component: "/basic/python/*".to_string(),
                    input: vec![],
                }],
                target: Target::Complex(RoutingTarget {
                    plugin: "python".to_string(),
                    strip_segments: vec!["basic".to_string(), "python".to_string()],
                    components: Some(vec!["udf".to_string()]),
                    exclude_components: None,
                }),
            },
            RoutingRule {
                match_rule: vec![MatchRule {
                    component: "/advanced/python/*".to_string(),
                    input: vec![],
                }],
                target: Target::Complex(RoutingTarget {
                    plugin: "python".to_string(),
                    strip_segments: vec!["advanced".to_string(), "python".to_string()],
                    components: Some(vec!["analyzer".to_string(), "transformer".to_string()]),
                    exclude_components: None,
                }),
            },
        ];

        let plugin_router = PluginRouter::builder()
            .with_routing_rules(rules)
            .register_plugin("python".to_string(), DynPlugin::boxed(MockPlugin))
            .build()
            .unwrap();

        // Test component listing with routing
        let components = plugin_router.list_components_with_routing().await.unwrap();

        // Should have 3 components total (no duplicates)
        assert_eq!(components.len(), 3);

        let component_paths: Vec<String> = components.iter().map(|c| c.component.to_string()).collect();
        assert!(component_paths.contains(&"/basic/python/udf".to_string()));
        assert!(component_paths.contains(&"/advanced/python/analyzer".to_string()));
        assert!(component_paths.contains(&"/advanced/python/transformer".to_string()));
    }

    #[tokio::test]
    async fn test_component_listing_with_simple_targets() {
        use stepflow_core::component::ComponentInfo;
        use stepflow_core::workflow::Component;

        struct MockPlugin;
        
        impl crate::Plugin for MockPlugin {
            async fn init(&self, _context: &Arc<dyn crate::Context>) -> crate::Result<()> {
                Ok(())
            }

            async fn list_components(&self) -> crate::Result<Vec<ComponentInfo>> {
                // Mock plugin returns components with plugin prefix (like builtin plugins)
                Ok(vec![
                    ComponentInfo {
                        component: Component::from_string("/python/component1"),
                        description: Some("Component 1".to_string()),
                        input_schema: None,
                        output_schema: None,
                    },
                    ComponentInfo {
                        component: Component::from_string("/python/component2"),
                        description: Some("Component 2".to_string()),
                        input_schema: None,
                        output_schema: None,
                    },
                ])
            }

            async fn component_info(&self, component: &Component) -> crate::Result<ComponentInfo> {
                Ok(ComponentInfo {
                    component: component.clone(),
                    description: Some("Mock component".to_string()),
                    input_schema: None,
                    output_schema: None,
                })
            }

            async fn execute(
                &self,
                _component: &Component,
                _context: crate::ExecutionContext,
                _input: stepflow_core::values::ValueRef,
            ) -> crate::Result<stepflow_core::FlowResult> {
                Ok(stepflow_core::FlowResult::Success {
                    result: stepflow_core::values::ValueRef::new(serde_json::json!({"mock": "result"})),
                })
            }
        }

        // Create routing rules with simple targets (no auto-detected path transformation)
        let rules = vec![
            RoutingRule {
                match_rule: vec![MatchRule {
                    component: "/python/*".to_string(),
                    input: vec![],
                }],
                target: "python".into(),
            },
        ];

        let plugin_router = PluginRouter::builder()
            .with_routing_rules(rules)
            .register_plugin("python".to_string(), DynPlugin::boxed(MockPlugin))
            .build()
            .unwrap();

        // Test component listing with routing
        let components = plugin_router.list_components_with_routing().await.unwrap();

        // Should have 2 components with no path transformation (plugin already returns full paths)
        assert_eq!(components.len(), 2);

        let component_paths: Vec<String> = components.iter().map(|c| c.component.to_string()).collect();
        assert!(component_paths.contains(&"/python/component1".to_string()));
        assert!(component_paths.contains(&"/python/component2".to_string()));
    }
}
