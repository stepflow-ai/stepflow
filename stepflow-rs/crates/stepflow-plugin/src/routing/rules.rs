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

use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use stepflow_core::values::ValueRef;
use stepflow_core::workflow::JsonPath;

/// Prefix-keyed routing configuration.
///
/// Keys are single-segment path prefixes (e.g., "/python", "/builtin") or the
/// root catch-all "/". Multi-segment prefixes (e.g., "/python/core") are not
/// allowed. Each plugin's registered component paths are mounted under the
/// prefix. The orchestrator builds a per-plugin trie from the plugin's
/// component registrations.
#[derive(Serialize, Deserialize, Debug, Clone, Default, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoutingConfig {
    /// Prefix-to-routing rules mapping.
    ///
    /// Keys must be either "/" (catch-all) or a single-segment prefix like
    /// "/python", "/builtin". Multi-segment prefixes are rejected at build time.
    /// Each plugin's registered component paths are mounted under the prefix.
    ///
    /// Value: ordered list of routing rules. When multiple rules exist for a prefix,
    /// they are evaluated in order — the first rule whose conditions match is used.
    pub routes: HashMap<String, Vec<RouteRule>>,
}

/// A single routing rule mapping a prefix to a plugin.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RouteRule {
    /// Optional input conditions that must match for this rule to apply.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<InputCondition>,

    /// Optional component allowlist — only these component IDs are allowed.
    ///
    /// If omitted, all components are allowed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<Vec<String>>")]
    pub component_allow: Option<Vec<Cow<'static, str>>>,

    /// Optional component denylist — these component IDs are blocked.
    ///
    /// If omitted, no components are blocked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<Vec<String>>")]
    pub component_deny: Option<Vec<Cow<'static, str>>>,

    /// Plugin name to route to.
    #[schemars(with = "String")]
    pub plugin: Cow<'static, str>,

    /// Additional parameters passed to the plugin at task dispatch time.
    ///
    /// These are transport-specific overrides (e.g., `stream` for NATS,
    /// `queueName` for gRPC) that the plugin merges with its own defaults.
    /// Unknown fields in route rules are captured here via `#[serde(flatten)]`.
    #[serde(flatten)]
    #[schemars(with = "std::collections::HashMap<String, serde_json::Value>")]
    pub params: std::collections::HashMap<String, serde_json::Value>,
}

/// JSON path condition for matching specific parts of input data
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InputCondition {
    /// JSON path expression (e.g., "$.model", "$.config.temperature")
    pub path: JsonPath,

    /// Value to match against (equality comparison)
    pub value: ValueRef,
}

/// Information about a route match for reverse routing
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RouteMatch {
    /// The path pattern that matches this route (e.g., "/builtin")
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
            "/openai".to_string(),
            vec![RouteRule {
                conditions: vec![],
                component_allow: None,
                component_deny: None,
                plugin: "openai".into(),
                params: std::collections::HashMap::new(),
            }],
        );
        routes.insert(
            "/python".to_string(),
            vec![
                RouteRule {
                    conditions: vec![InputCondition {
                        path: JsonPath::parse("$.model").unwrap(),
                        value: ValueRef::new(serde_json::json!("llama2")),
                    }],
                    component_allow: None,
                    component_deny: None,
                    plugin: "llama2_pool".into(),
                    params: std::collections::HashMap::new(),
                },
                RouteRule {
                    conditions: vec![],
                    component_allow: None,
                    component_deny: None,
                    plugin: "default_pool".into(),
                    params: std::collections::HashMap::new(),
                },
            ],
        );

        let config = RoutingConfig { routes };
        let serialized = serde_json::to_string(&config).unwrap();
        assert!(serialized.contains("/openai"));
        assert!(serialized.contains("/python"));
        assert!(serialized.contains("llama2"));
    }

    #[test]
    fn test_routing_config_deserialization() {
        let json_str = r#"
        {
            "routes": {
                "/openai": [
                    {
                        "plugin": "openai"
                    }
                ],
                "/python": [
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

        let openai_rules = &config.routes["/openai"];
        assert_eq!(openai_rules.len(), 1);
        assert_eq!(openai_rules[0].plugin, "openai");

        let python_rules = &config.routes["/python"];
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
            params: std::collections::HashMap::new(),
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
            params: std::collections::HashMap::new(),
        };

        let serialized = serde_json::to_string(&rule).unwrap();
        let deserialized: RouteRule = serde_json::from_str(&serialized).unwrap();

        assert!(deserialized.conditions.is_empty());
        assert!(deserialized.component_allow.is_none());
        assert!(deserialized.component_deny.is_none());
        assert_eq!(deserialized.plugin, "simple_plugin");
    }

    /// Extra fields in YAML (like `subject`) are captured in the `params` map.
    #[test]
    fn test_route_rule_with_params_deserialization() {
        let yaml = r#"
routes:
  "/nats":
    - plugin: nats
      stream: "FOO_TASKS"
"#;
        let config: RoutingConfig = serde_yaml_ng::from_str(yaml).unwrap();
        let rules = &config.routes["/nats"];
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].plugin, "nats");
        assert_eq!(
            rules[0].params.get("stream"),
            Some(&serde_json::Value::String("FOO_TASKS".to_string()))
        );
    }

    /// Serialize a RouteRule with params, then deserialize — params are preserved.
    #[test]
    fn test_route_rule_params_round_trip() {
        let mut params = std::collections::HashMap::new();
        params.insert("stream".to_string(), serde_json::json!("BAR_TASKS"));
        params.insert("priority".to_string(), serde_json::json!(5));

        let rule = RouteRule {
            conditions: vec![],
            component_allow: None,
            component_deny: None,
            plugin: "nats".into(),
            params,
        };

        let json = serde_json::to_string(&rule).unwrap();
        let deserialized: RouteRule = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.plugin, "nats");
        assert_eq!(
            deserialized.params.get("stream"),
            Some(&serde_json::json!("BAR_TASKS"))
        );
        assert_eq!(
            deserialized.params.get("priority"),
            Some(&serde_json::json!(5))
        );
    }

    /// Empty params map should not add any extra fields to JSON output.
    #[test]
    fn test_route_rule_empty_params_not_serialized_json() {
        let rule = RouteRule {
            conditions: vec![],
            component_allow: None,
            component_deny: None,
            plugin: "simple".into(),
            params: std::collections::HashMap::new(),
        };

        let json: serde_json::Value = serde_json::to_value(&rule).unwrap();
        let obj = json.as_object().unwrap();

        // Only the `plugin` field should be present (conditions, allow, deny
        // are skipped when empty/None).
        assert_eq!(
            obj.len(),
            1,
            "Only 'plugin' should be serialized, got: {obj:?}"
        );
        assert!(obj.contains_key("plugin"));
    }

    #[test]
    fn test_route_serialization_yaml() {
        let mut routes = HashMap::new();
        routes.insert(
            "/mock".to_string(),
            vec![RouteRule {
                conditions: vec![],
                component_allow: None,
                component_deny: None,
                plugin: "mock".into(),
                params: std::collections::HashMap::new(),
            }],
        );

        let routes = RoutingConfig { routes };
        let serialized = serde_yaml_ng::to_string(&routes).unwrap();
        assert_eq!(serialized, "routes:\n  /mock:\n  - plugin: mock\n");
    }
}
