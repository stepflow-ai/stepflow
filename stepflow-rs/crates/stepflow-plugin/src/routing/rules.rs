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

use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use stepflow_core::values::ValueRef;
use stepflow_core::workflow::JsonPath;

/// Path-keyed routing configuration
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct RoutingConfig {
    /// Path-to-routing rules mapping
    ///
    /// Keys describe paths. For example "/python/{component}" or "/openai/{component}".
    /// Placeholders may match a single segment (e.g., "{component}") or multiple segments (e.g., "{*component}").
    ///
    /// Value: ordered list of routing rules to apply to that path.
    ///
    /// Routes will be applied in the order they are listed, with the first matching rule being used.
    pub routes: HashMap<String, Vec<RouteRule>>,
}

/// A single routing rule for a specific path
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct RouteRule {
    /// Optional input conditions that must match for this rule to apply
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<InputCondition>,

    /// Optional component allowlist - only these components are allowed to match this rule
    ///
    /// If omitted, all components are allowed to match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_allow: Option<Vec<Cow<'static, str>>>,

    /// Optional component denylist - these components are blocked from matching this rule
    ///
    /// If omitted, no components are blocked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_deny: Option<Vec<Cow<'static, str>>>,

    /// Plugin name to route to
    pub plugin: Cow<'static, str>,

    /// Component name to pass to the plugin.
    /// Defaults to `/{component}` if not specified, meaning the extracted component name is used.
    ///
    /// May be a pattern referencing path placeholders, e.g., "{component}" or "{*component}".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<Cow<'static, str>>,
}

/// JSON path condition for matching specific parts of input data
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct InputCondition {
    /// JSON path expression (e.g., "$.model", "$.config.temperature")
    pub path: JsonPath,

    /// Value to match against (equality comparison)
    pub value: ValueRef,
}

/// Information about a route match for reverse routing
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct RouteMatch {
    /// The path pattern that matches this route (e.g., "/builtin/{component}")
    pub path_pattern: String,
    /// The resolved path for this specific component (e.g., "/builtin/eval")
    pub resolved_path: String,
    /// Input conditions that must be met for this route to be available
    pub conditions: Vec<InputCondition>,
    /// Whether this route is conditional (derived from conditions.is_empty())
    pub is_conditional: bool,
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_routing_config_serialization() {
        let mut routes = HashMap::new();
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
            "/python/{component}".to_string(),
            vec![
                RouteRule {
                    conditions: vec![InputCondition {
                        path: JsonPath::parse("$.model").unwrap(),
                        value: ValueRef::new(serde_json::json!("llama2")),
                    }],
                    component_allow: None,
                    component_deny: None,
                    plugin: "llama2_pool".into(),
                    component: None,
                },
                RouteRule {
                    conditions: vec![],
                    component_allow: None,
                    component_deny: None,
                    plugin: "default_pool".into(),
                    component: None,
                },
            ],
        );

        let config = RoutingConfig { routes };
        let serialized = serde_json::to_string(&config).unwrap();
        assert!(serialized.contains("/openai/{component}"));
        assert!(serialized.contains("/python/{component}"));
        assert!(serialized.contains("llama2"));
    }

    #[test]
    fn test_routing_config_deserialization() {
        let json_str = r#"
        {
            "routes": {
                "/openai/{component}": [
                    {
                        "plugin": "openai"
                    }
                ],
                "/python/{component}": [
                    {
                        "conditions": [
                            {
                                "path": "$.model",
                                "value": "llama2"
                            }
                        ],
                        "plugin": "llama2_pool"
                    },
                    {
                        "plugin": "default_pool"
                    }
                ]
            }
        }
        "#;

        let config: RoutingConfig = serde_json::from_str(json_str).unwrap();
        assert_eq!(config.routes.len(), 2);

        let openai_rules = &config.routes["/openai/{component}"];
        assert_eq!(openai_rules.len(), 1);
        assert_eq!(openai_rules[0].plugin, "openai");

        let python_rules = &config.routes["/python/{component}"];
        assert_eq!(python_rules.len(), 2);
        assert_eq!(python_rules[0].conditions.len(), 1);
        assert_eq!(python_rules[0].plugin, "llama2_pool");
        assert_eq!(python_rules[1].plugin, "default_pool");
    }

    #[test]
    fn test_route_rule_with_conditions() {
        let rule = RouteRule {
            conditions: vec![
                InputCondition {
                    path: JsonPath::parse("$.model").unwrap(),
                    value: ValueRef::new(serde_json::json!("gpt-4")),
                },
                InputCondition {
                    path: JsonPath::parse("$.temperature").unwrap(),
                    value: ValueRef::new(serde_json::json!(0.7)),
                },
            ],
            component_allow: Some(vec!["allowed_component".into()]),
            component_deny: Some(vec!["denied_component".into()]),
            plugin: "test_plugin".into(),
            component: Some("rewritten_component".into()),
        };

        let serialized = serde_json::to_string(&rule).unwrap();
        let deserialized: RouteRule = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.conditions.len(), 2);
        assert_eq!(deserialized.component_allow.as_ref().unwrap().len(), 1);
        assert_eq!(deserialized.component_deny.as_ref().unwrap().len(), 1);
        assert_eq!(deserialized.plugin, "test_plugin");
    }

    #[test]
    fn test_route_rule_minimal() {
        let rule = RouteRule {
            conditions: vec![],
            component_allow: None,
            component_deny: None,
            plugin: "simple_plugin".into(),
            component: None,
        };

        let serialized = serde_json::to_string(&rule).unwrap();
        let deserialized: RouteRule = serde_json::from_str(&serialized).unwrap();

        assert!(deserialized.conditions.is_empty());
        assert!(deserialized.component_allow.is_none());
        assert!(deserialized.component_deny.is_none());
        assert_eq!(deserialized.plugin, "simple_plugin");
    }

    #[test]
    fn test_route_serialization_yaml() {
        let mut routes = HashMap::new();
        routes.insert(
            "/mock/{component}".to_string(),
            vec![RouteRule {
                conditions: vec![],
                component_allow: None,
                component_deny: None,
                plugin: "mock".into(),
                component: None,
            }],
        );

        let routes = RoutingConfig { routes };
        let serialized = serde_yaml_ng::to_string(&routes).unwrap();
        assert_eq!(
            serialized,
            "routes:\n  /mock/{component}:\n  - plugin: mock\n"
        );
    }
}
