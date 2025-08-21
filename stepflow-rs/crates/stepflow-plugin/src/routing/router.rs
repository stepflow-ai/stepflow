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

use crate::routing::{RouteMatch, RouteRule, RoutingConfig};

use matchit::{Params, Router as MatchitRouter};
use stepflow_core::values::ValueRef;
use stepflow_core::workflow::JsonPath;

/// Router that matches component strings to target plugins using trie-based routing
#[derive(Debug)]
pub struct Router {
    /// Matchit router for efficient trie-based routing
    router: MatchitRouter<Vec<RouteInfo>>,
    /// Original routing configuration for reverse routing
    routing_config: RoutingConfig,
}

/// Information about a matched route
#[derive(Debug, Clone)]
pub struct RouteInfo {
    /// The routing rule for this route
    pub rule: RouteRule,
    /// The pattern for this route info.
    path_pattern: String,
    /// Plugin index for this route
    pub plugin_index: usize,
}

/// A step router that determines how to route a specific component
#[derive(Debug, Clone)]
pub enum ComponentRouter<'a> {
    /// Route directly to a specific plugin without input filtering
    Direct {
        route_info: &'a RouteInfo,
        params: Params<'a, 'a>,
    },
    /// Apply input filters in sequence until a match is found
    Conditional {
        original_component: &'a str,
        route_infos: &'a [RouteInfo],
        /// All parameters extracted from path template matching
        params: Params<'a, 'a>,
    },
}

/// Error type for router operations
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum RouterError {
    #[error("Invalid route pattern: {pattern}")]
    InvalidPattern { pattern: String },
    #[error("Unknown plugin referenced in route: {0}")]
    UnknownPlugin(String),
    #[error("No matching rule found for component: {component}")]
    NoMatchingRule { component: String },
    #[error("Failed to extract value from input path: '{0}'")]
    InputPathFailed(JsonPath),
    #[error("Input filter evaluation failed: {message}")]
    InputFilterFailed { message: String },
    #[error("Component filtering failed: {message}")]
    ComponentFilterFailed { message: String },
    #[error("Parameter '{parameter}' not available in pattern '{pattern}'")]
    MissingParameter { pattern: String, parameter: String },
    #[error("Failed to insert route for pattern '{path_pattern}': {error}")]
    RouteInsertionFailed {
        path_pattern: String,
        error: matchit::InsertError,
    },
}

impl Router {
    /// Create a new router from routing configuration
    pub fn new(
        routing_config: RoutingConfig,
        plugin_indices: &HashMap<String, usize>,
    ) -> error_stack::Result<Self, RouterError> {
        let mut router = MatchitRouter::new();

        // Process routing configuration and build trie
        for (path_pattern, route_rules) in &routing_config.routes {
            let mut route_infos = Vec::new();

            // Create route info for each rule in this path pattern
            for route_rule in route_rules {
                let plugin_index = plugin_indices
                    .get(route_rule.plugin.as_ref())
                    .copied()
                    .ok_or_else(|| {
                        error_stack::report!(RouterError::UnknownPlugin(
                            route_rule.plugin.to_string()
                        ))
                    })?;

                route_infos.push(RouteInfo {
                    rule: route_rule.clone(),
                    path_pattern: path_pattern.clone(),
                    plugin_index,
                });
            }

            // Insert into trie router
            router.insert(path_pattern, route_infos).map_err(|error| {
                error_stack::report!(RouterError::RouteInsertionFailed {
                    path_pattern: path_pattern.clone(),
                    error
                })
            })?;
        }

        Ok(Router {
            router,
            routing_config,
        })
    }

    /// Create a step router for a given component string
    pub fn route<'a>(
        &'a self,
        component: &'a str,
    ) -> error_stack::Result<ComponentRouter<'a>, RouterError> {
        // Try to match with trie router
        let matched = self.router.at(component).map_err(|_| {
            error_stack::report!(RouterError::NoMatchingRule {
                component: component.to_owned()
            })
        })?;

        let route_infos = matched.value;
        let params = matched.params;

        // Check if we have a single rule with no conditions and no extracted parameters (direct routing)
        if route_infos.len() == 1 && route_infos[0].rule.conditions.is_empty() {
            return Ok(ComponentRouter::Direct {
                route_info: &route_infos[0],
                params,
            });
        }

        Ok(ComponentRouter::Conditional {
            original_component: component,
            route_infos,
            params,
        })
    }

    /// Find all routes that could match a given plugin and component
    pub fn find_routes_for(&self, plugin: &str, component: &str) -> Vec<RouteMatch> {
        let mut matches = Vec::new();

        // Iterate through all routes in the configuration
        for (path_pattern, route_rules) in &self.routing_config.routes {
            for route_rule in route_rules {
                // Check if this route targets our plugin
                if plugin != route_rule.plugin.as_ref() {
                    continue;
                }

                // Try to construct a path that would match this route
                if let Some(resolved_path) =
                    self.construct_path_for_component(path_pattern, component)
                {
                    // Create a mock RouteInfo to check allow/deny rules
                    let route_info = RouteInfo {
                        rule: route_rule.clone(),
                        path_pattern: path_pattern.clone(),
                        plugin_index: 0, // Not used for component filtering
                    };

                    // Check if the component is allowed by this route
                    if !route_info.is_allowed(component) {
                        continue;
                    }

                    matches.push(RouteMatch {
                        path_pattern: path_pattern.clone(),
                        resolved_path,
                        conditions: route_rule.conditions.clone(),
                        is_conditional: !route_rule.conditions.is_empty(),
                    });
                }
            }
        }

        matches
    }

    /// Construct a resolved path for a component given a path pattern
    fn construct_path_for_component(&self, path_pattern: &str, component: &str) -> Option<String> {
        // Handle different pattern substitutions
        // TODO: Improve how we do this substitution.
        if path_pattern.contains("{component}") {
            // Simple component substitution
            let resolved = path_pattern.replace("{component}", component);
            // Clean up double slashes that might occur if both pattern and component have slashes
            Some(resolved.replace("//", "/"))
        } else if path_pattern.contains("{*component}") {
            // Wildcard component substitution - strip the leading slash from component
            let component_without_slash = component.strip_prefix('/').unwrap_or(component);
            Some(path_pattern.replace("{*component}", component_without_slash))
        } else {
            // For patterns that don't have component placeholders, we can't construct a path
            None
        }
    }
}

impl RouteInfo {
    fn resolve_component(
        &self,
        params: &Params<'_, '_>,
    ) -> error_stack::Result<String, RouterError> {
        if let Some(component_template) = &self.rule.component {
            // Replace placeholders in the component pattern with extracted parameters
            // TODO: this would likely be better done with some kind of templating
            // such as tinytemplate. We would ideally compile the pattern templates
            // and then just substitue the params.
            let mut resolved = component_template.to_string();
            for (key, value) in params.iter() {
                let placeholder = format!("{{{key}}}");
                resolved = resolved.replace(&placeholder, value);
            }
            Ok(resolved)
        } else {
            // Default behavior: use "component" parameter if available, otherwise full path
            let component = params.get("component").ok_or_else(|| {
                error_stack::report!(RouterError::MissingParameter {
                    pattern: self.path_pattern.clone(),
                    parameter: "component".to_string()
                })
            })?;
            Ok(format!("/{component}"))
        }
    }

    fn evaluate_conditions(&self, input: &ValueRef) -> error_stack::Result<bool, RouterError> {
        // Apply input conditions
        for input_condition in &self.rule.conditions {
            let input_value = input
                .resolve_json_path(&input_condition.path)
                .ok_or_else(|| {
                    error_stack::report!(RouterError::InputPathFailed(input_condition.path.clone()))
                })?;

            if input_value != input_condition.value {
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn is_allowed(&self, component: &str) -> bool {
        if let Some(deny_list) = &self.rule.component_deny
            && deny_list.iter().any(|denied| denied.as_ref() == component)
        {
            return false;
        }

        if let Some(allow_list) = &self.rule.component_allow
            && !allow_list
                .iter()
                .any(|allowed| allowed.as_ref() == component)
        {
            return false;
        }

        true
    }
}

impl<'a> ComponentRouter<'a> {
    /// Resolve the target plugin for this step router.
    ///
    /// Returns the plugin index and resolved component name.
    pub fn get_route_and_component(
        &self,
        input: ValueRef,
    ) -> error_stack::Result<(&'_ RouteInfo, String), RouterError> {
        match self {
            ComponentRouter::Direct { route_info, params } => {
                let component_name = route_info.resolve_component(params)?;
                Ok((route_info, component_name))
            }
            ComponentRouter::Conditional {
                original_component,
                route_infos,
                params,
            } => {
                for route_info in route_infos.iter() {
                    if !route_info.evaluate_conditions(&input)? {
                        continue; // Condition not met, try next rule
                    }

                    let component = route_info.resolve_component(params)?;
                    if !route_info.is_allowed(&component) {
                        continue; // Component not allowed, try next rule
                    }

                    return Ok((route_info, component));
                }

                error_stack::bail!(RouterError::NoMatchingRule {
                    component: original_component.to_string(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::rules::{InputCondition, RouteRule, RoutingConfig};
    use serde_json::json;
    use stepflow_core::workflow::JsonPath;

    fn create_test_config() -> RoutingConfig {
        let mut routes = std::collections::HashMap::new();

        routes.insert(
            "/openai/{component}".to_string(),
            vec![RouteRule {
                conditions: vec![],
                component_allow: None,
                component_deny: None,
                plugin: "openai".into(),
                component: None,
            }],
        );

        routes.insert(
            "/custom/{component}".to_string(),
            vec![RouteRule {
                conditions: vec![InputCondition {
                    path: JsonPath::parse("$.model").unwrap(),
                    value: ValueRef::new(json!("gpt-4")),
                }],
                component_allow: None,
                component_deny: None,
                plugin: "custom".into(),
                component: None,
            }],
        );

        routes.insert(
            "/fallback/{component}".to_string(),
            vec![RouteRule {
                conditions: vec![],
                component_allow: None,
                component_deny: None,
                plugin: "fallback".into(),
                component: None,
            }],
        );

        RoutingConfig { routes }
    }

    fn create_test_plugin_indices() -> HashMap<String, usize> {
        let mut plugin_indices = HashMap::new();
        plugin_indices.insert("openai".to_string(), 0);
        plugin_indices.insert("custom".to_string(), 1);
        plugin_indices.insert("fallback".to_string(), 2);
        plugin_indices
    }

    #[test]
    fn test_router_creation() {
        let config = create_test_config();
        let plugin_indices = create_test_plugin_indices();
        let router = Router::new(config, &plugin_indices);
        assert!(router.is_ok());
    }

    #[test]
    fn test_direct_routing() {
        let config = create_test_config();
        let plugin_indices = create_test_plugin_indices();
        let router = Router::new(config, &plugin_indices).unwrap();

        let step_router = router.route("/openai/chat").unwrap();
        let input = ValueRef::new(serde_json::Value::Null);
        let (route_info, component_name) = step_router.get_route_and_component(input).unwrap();

        assert_eq!(route_info.rule.plugin.as_ref(), "openai"); // Should route to openai plugin
        assert_eq!(component_name, "/chat"); // Should extract component from path
    }

    #[test]
    fn test_conditional_routing() {
        let config = create_test_config();
        let plugin_indices = create_test_plugin_indices();
        let router = Router::new(config, &plugin_indices).unwrap();

        let step_router = router.route("/custom/model").unwrap();

        // Test matching condition
        let input = ValueRef::new(json!({"model": "gpt-4"}));
        let (route_info, component_name) = step_router.get_route_and_component(input).unwrap();
        assert_eq!(route_info.rule.plugin.as_ref(), "custom"); // Should route to custom plugin
        assert_eq!(component_name, "/model"); // Should extract component from path

        // Test non-matching condition
        let input = ValueRef::new(json!({"model": "gpt-3"}));
        let result = step_router.get_route_and_component(input);
        assert!(result.is_err()); // Should fail for non-matching condition
    }

    #[test]
    fn test_step_router_resolve_direct() {
        let config = create_test_config();
        let plugin_indices = create_test_plugin_indices();
        let router = Router::new(config, &plugin_indices).unwrap();

        let step_router = router.route("/openai/chat").unwrap();
        let input = ValueRef::new(serde_json::Value::Null);
        let (route_info, component_name) = step_router.get_route_and_component(input).unwrap();

        assert_eq!(route_info.rule.plugin.as_ref(), "openai"); // Should route to openai plugin
        assert_eq!(component_name, "/chat");
        // Also verify plugin index
        assert_eq!(route_info.plugin_index, 0);
    }

    #[test]
    fn test_step_router_resolve_conditional_match() {
        // Create a real router to test conditional resolution
        let config = create_test_config();
        let plugin_indices = create_test_plugin_indices();
        let router = Router::new(config, &plugin_indices).unwrap();

        let step_router = router.route("/custom/model").unwrap();
        let input = ValueRef::new(json!({"model": "gpt-4"}));
        let (route_info, component_name) = step_router.get_route_and_component(input).unwrap();

        assert_eq!(route_info.rule.plugin.as_ref(), "custom"); // Should route to custom plugin
        assert_eq!(component_name, "/model");
        // Also verify plugin index
        assert_eq!(route_info.plugin_index, 1);
    }

    #[test]
    fn test_step_router_resolve_conditional_no_match() {
        // Create a real router to test conditional resolution
        let config = create_test_config();
        let plugin_indices = create_test_plugin_indices();
        let router = Router::new(config, &plugin_indices).unwrap();

        let step_router = router.route("/custom/model").unwrap();
        let input = ValueRef::new(json!({"model": "gpt-3"})); // Doesn't match "gpt-4"
        let result = step_router.get_route_and_component(input);
        assert_eq!(
            result.unwrap_err().current_context(),
            &RouterError::NoMatchingRule {
                component: "/custom/model".to_string()
            },
        );
    }

    #[test]
    fn test_path_style_component_matching() {
        let mut routes = std::collections::HashMap::new();

        routes.insert(
            "/openai/chat/{subcomponent}".to_string(),
            vec![RouteRule {
                conditions: vec![],
                component_allow: None,
                component_deny: None,
                plugin: "openai".into(),
                component: Some("/{subcomponent}".into()),
            }],
        );

        routes.insert(
            "/anthropic/{model}/claude".to_string(),
            vec![RouteRule {
                conditions: vec![],
                component_allow: None,
                component_deny: None,
                plugin: "anthropic".into(),
                component: Some("/{model}".into()),
            }],
        );

        let config = RoutingConfig { routes };
        let mut plugin_indices = HashMap::new();
        plugin_indices.insert("openai".to_string(), 0);
        plugin_indices.insert("anthropic".to_string(), 1);
        let router = Router::new(config, &plugin_indices).unwrap();

        // Test OpenAI pattern
        let step_router = router.route("/openai/chat/completions").unwrap();
        let input = ValueRef::new(serde_json::Value::Null);
        let (route_info, component_name) = step_router.get_route_and_component(input).unwrap();
        assert_eq!(route_info.rule.plugin.as_ref(), "openai"); // Should route to openai plugin
        assert_eq!(component_name, "/completions"); // Should extract subcomponent parameter

        // Test Anthropic pattern
        let step_router = router.route("/anthropic/models/claude").unwrap();
        let input = ValueRef::new(serde_json::Value::Null);
        let (route_info, component_name) = step_router.get_route_and_component(input).unwrap();
        assert_eq!(route_info.rule.plugin.as_ref(), "anthropic"); // Should route to anthropic plugin
        assert_eq!(component_name, "/models"); // Should extract model parameter
    }

    #[test]
    fn test_no_matching_rule() {
        let config = create_test_config();
        let plugin_indices = create_test_plugin_indices();
        let router = Router::new(config, &plugin_indices).unwrap();

        let result = router.route("/unknown/component");
        assert_eq!(
            result.unwrap_err().current_context(),
            &RouterError::NoMatchingRule {
                component: "/unknown/component".to_string()
            },
        );
    }

    #[test]
    fn test_fallthrough_routing() {
        let mut routes = std::collections::HashMap::new();

        routes.insert(
            "/service/{component}".to_string(),
            vec![
                RouteRule {
                    conditions: vec![InputCondition {
                        path: JsonPath::parse("$.priority").unwrap(),
                        value: ValueRef::new(json!("high")),
                    }],
                    component_allow: None,
                    component_deny: None,
                    plugin: "high_priority".into(),
                    component: None,
                },
                RouteRule {
                    conditions: vec![],
                    component_allow: None,
                    component_deny: None,
                    plugin: "default".into(),
                    component: None,
                },
            ],
        );

        let config = RoutingConfig { routes };
        let mut plugin_indices = HashMap::new();
        plugin_indices.insert("high_priority".to_string(), 0);
        plugin_indices.insert("default".to_string(), 1);
        let router = Router::new(config, &plugin_indices).unwrap();
        let step_router = router.route("/service/process").unwrap();

        // Test high priority match
        let input = ValueRef::new(json!({"priority": "high"}));
        let (route_info, component_name) = step_router.get_route_and_component(input).unwrap();
        assert_eq!(route_info.rule.plugin.as_ref(), "high_priority"); // Should route to high priority plugin
        assert_eq!(component_name, "/process"); // Should extract component parameter

        // Test fallback match
        let input = ValueRef::new(json!({"priority": "low"}));
        let (route_info, component_name) = step_router.get_route_and_component(input).unwrap();
        assert_eq!(route_info.rule.plugin.as_ref(), "default"); // Should route to default plugin
        assert_eq!(component_name, "/process"); // Should extract component parameter
    }

    #[test]
    fn test_routing_behavior_consistency() {
        // Test that the same route with the same input produces consistent results
        let config = create_test_config();
        let plugin_indices = create_test_plugin_indices();
        let router = Router::new(config, &plugin_indices).unwrap();

        let step_router = router.route("/openai/chat").unwrap();
        let input = ValueRef::new(serde_json::Value::Null);

        // Route multiple times and ensure consistency
        let result1 = step_router.get_route_and_component(input.clone()).unwrap();
        let result2 = step_router.get_route_and_component(input).unwrap();

        assert_eq!(result1.0.rule.plugin, result2.0.rule.plugin);
        assert_eq!(result1.1, result2.1);
    }

    #[test]
    fn test_rule_map_structure() {
        let mut routes = std::collections::HashMap::new();

        routes.insert(
            "/pattern1/{component}".to_string(),
            vec![RouteRule {
                conditions: vec![],
                component_allow: None,
                component_deny: None,
                plugin: "target1".into(),
                component: None,
            }],
        );

        routes.insert(
            "/pattern2/{component}".to_string(),
            vec![RouteRule {
                conditions: vec![],
                component_allow: None,
                component_deny: None,
                plugin: "target1".into(),
                component: None,
            }],
        );

        routes.insert(
            "/pattern3/{component}".to_string(),
            vec![RouteRule {
                conditions: vec![],
                component_allow: None,
                component_deny: None,
                plugin: "target2".into(),
                component: None,
            }],
        );

        let config = RoutingConfig { routes };
        let mut plugin_indices = HashMap::new();
        plugin_indices.insert("target1".to_string(), 0);
        plugin_indices.insert("target2".to_string(), 1);
        let router = Router::new(config, &plugin_indices).unwrap();

        // Test that different patterns route correctly
        let step_router1 = router.route("/pattern1/test").unwrap();
        let input = ValueRef::new(serde_json::Value::Null);
        let (route_info, component_name) = step_router1.get_route_and_component(input).unwrap();
        assert_eq!(route_info.rule.plugin.as_ref(), "target1"); // Should route to target1 plugin
        assert_eq!(component_name, "/test"); // Should extract component parameter

        let step_router3 = router.route("/pattern3/test").unwrap();
        let input = ValueRef::new(serde_json::Value::Null);
        let (route_info, component_name) = step_router3.get_route_and_component(input).unwrap();
        assert_eq!(route_info.rule.plugin.as_ref(), "target2"); // Should route to target2 plugin
        assert_eq!(component_name, "/test"); // Should extract component parameter
    }

    #[test]
    fn test_multiple_parameter_extraction() {
        let mut routes = std::collections::HashMap::new();

        routes.insert(
            "/api/{version}/{service}/{action}".to_string(),
            vec![RouteRule {
                conditions: vec![],
                component_allow: None,
                component_deny: None,
                plugin: "api_router".into(),
                component: Some("{service}_{action}".into()),
            }],
        );

        routes.insert(
            "/ml/{model}/inference/{*path}".to_string(),
            vec![RouteRule {
                conditions: vec![],
                component_allow: None,
                component_deny: None,
                plugin: "ml_service".into(),
                component: Some("{model}/inference".into()),
            }],
        );

        let config = RoutingConfig { routes };
        let mut plugin_indices = HashMap::new();
        plugin_indices.insert("api_router".to_string(), 0);
        plugin_indices.insert("ml_service".to_string(), 1);
        let router = Router::new(config, &plugin_indices).unwrap();

        // Test API route with multiple parameters
        let step_router = router.route("/api/v1/user/create").unwrap();
        let input = ValueRef::new(serde_json::Value::Null);
        let (route_info, component_name) = step_router.get_route_and_component(input).unwrap();
        assert_eq!(route_info.rule.plugin.as_ref(), "api_router"); // Should route to api_router plugin
        assert_eq!(component_name, "user_create"); // Should resolve template with parameters

        // Test ML route with catch-all parameter
        let step_router = router.route("/ml/gpt4/inference/chat/completions").unwrap();
        let input = ValueRef::new(serde_json::Value::Null);
        let (route_info, component_name) = step_router.get_route_and_component(input).unwrap();
        assert_eq!(route_info.rule.plugin.as_ref(), "ml_service"); // Should route to ml_service plugin
        assert_eq!(component_name, "gpt4/inference"); // Should resolve template with parameters
    }

    #[test]
    fn test_component_resolution_fallback() {
        let mut routes = std::collections::HashMap::new();

        routes.insert(
            "/fallback/{component}".to_string(),
            vec![RouteRule {
                conditions: vec![],
                component_allow: None,
                component_deny: None,
                plugin: "fallback_service".into(),
                component: None, // No component pattern specified
            }],
        );

        routes.insert(
            "/custom/{name}".to_string(),
            vec![RouteRule {
                conditions: vec![],
                component_allow: None,
                component_deny: None,
                plugin: "custom_service".into(),
                component: Some("/{name}".into()), // Use name parameter as component
            }],
        );

        let config = RoutingConfig { routes };
        let mut plugin_indices = HashMap::new();
        plugin_indices.insert("fallback_service".to_string(), 0);
        plugin_indices.insert("custom_service".to_string(), 1);
        let router = Router::new(config, &plugin_indices).unwrap();

        // Test fallback to "component" parameter
        let step_router = router.route("/fallback/my_component").unwrap();
        let input = ValueRef::new(serde_json::Value::Null);
        let (route_info, component_name) = step_router.get_route_and_component(input).unwrap();
        assert_eq!(route_info.rule.plugin.as_ref(), "fallback_service"); // Should route to fallback_service plugin
        assert_eq!(component_name, "/my_component"); // Should extract component parameter

        // Test fallback to available parameter when no component parameter exists
        let step_router = router.route("/custom/my_name").unwrap();
        let input = ValueRef::new(serde_json::Value::Null);
        let (route_info, component_name) = step_router.get_route_and_component(input).unwrap();
        assert_eq!(route_info.rule.plugin.as_ref(), "custom_service"); // Should route to custom_service plugin
        assert_eq!(component_name, "/my_name"); // Should extract name parameter as component
    }

    #[test]
    fn test_extracted_params_access() {
        let mut routes = std::collections::HashMap::new();

        routes.insert(
            "/service/{version}/{endpoint}".to_string(),
            vec![RouteRule {
                conditions: vec![],
                component_allow: None,
                component_deny: None,
                plugin: "service".into(),
                component: Some("/{endpoint}".into()),
            }],
        );

        let config = RoutingConfig { routes };
        let mut plugin_indices = HashMap::new();
        plugin_indices.insert("service".to_string(), 0);
        let router = Router::new(config, &plugin_indices).unwrap();

        // Test routing with extracted params
        let step_router = router.route("/service/v2/users").unwrap();
        let input = ValueRef::new(serde_json::Value::Null);
        let (route_info, component_name) = step_router.get_route_and_component(input).unwrap();
        assert_eq!(route_info.rule.plugin.as_ref(), "service"); // Should route to service plugin
        assert_eq!(component_name, "/users"); // Should extract endpoint parameter as component
    }
}
