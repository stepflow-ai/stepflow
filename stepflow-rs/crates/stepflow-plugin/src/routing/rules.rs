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

use serde::ser::SerializeStruct as _;
use serde::{Deserialize, Serialize};
use serde_with::{OneOrMany, serde_as};
use std::borrow::Cow;
use stepflow_core::values::ValueRef;
use stepflow_core::workflow::JsonPath;

/// A single routing rule that matches components and routes them to plugins
#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RoutingRule {
    /// Matching criteria for this rule.
    /// Must be one or more rules, any of which must match to route to the target.
    #[serde(rename = "match")]
    #[serde_as(as = "OneOrMany<_>")]
    pub match_rule: Vec<MatchRule>,

    /// Target plugin name to route to.
    pub target: Cow<'static, str>,
}

/// Match rule with JSON path support for precise input matching
#[derive(Debug, Clone)]
pub struct MatchRule {
    /// Component pattern filter
    pub component: String,

    /// Input data conditions using JSON path expressions.
    /// All conditions must match for this rule to apply.
    pub input: Vec<InputCondition>,
}

/// JSON path condition for matching specific parts of input data
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InputCondition {
    /// JSON path expression (e.g., "$.model", "$.config.temperature")
    pub path: JsonPath,

    /// Value to match against (equality comparison)
    pub value: ValueRef,
}

impl Serialize for MatchRule {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // If there is only a component filter, serialize as a string.
        if self.input.is_empty() {
            serializer.serialize_str(&self.component)
        } else {
            // Otherwise serialize as a struct with component and input
            let mut state = serializer.serialize_struct("Matcher", 2)?;
            state.serialize_field("component", &self.component)?;
            state.serialize_field("input", &self.input)?;
            state.end()
        }
    }
}

struct MatchRuleVisitor;

impl<'de> serde::de::Visitor<'de> for MatchRuleVisitor {
    type Value = MatchRule;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a string or a MatchRule struct")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_string(v.to_string())
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        // If the value is a string, treat it as a component pattern
        Ok(MatchRule {
            component: v,
            input: Vec::new(), // No input filters
        })
    }

    fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        // Deserialize as a struct with component and input
        let mut component = None;
        let mut input = Vec::new();

        let mut map_access = map;
        while let Some(key) = map_access.next_key::<String>()? {
            match key.as_str() {
                "component" => {
                    if component.is_some() {
                        return Err(serde::de::Error::duplicate_field("component"));
                    }
                    component = Some(map_access.next_value()?);
                }
                "input" => {
                    input = map_access.next_value()?;
                }
                _ => {
                    return Err(serde::de::Error::unknown_field(
                        key.as_str(),
                        &["component", "input"],
                    ));
                }
            }
        }

        Ok(MatchRule {
            component: component.ok_or_else(|| serde::de::Error::missing_field("component"))?,
            input,
        })
    }
}

impl<'de> Deserialize<'de> for MatchRule {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(MatchRuleVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type RoutingRules = Vec<RoutingRule>;

    #[test]
    fn test_routing_rules_serialization() {
        let rules = vec![
            RoutingRule {
                match_rule: vec![MatchRule {
                    component: "/openai/*".to_string(),
                    input: vec![],
                }],
                target: "openai".into(),
            },
            RoutingRule {
                match_rule: vec![MatchRule {
                    component: "/custom/*".to_string(),
                    input: vec![InputCondition {
                        path: JsonPath::parse("$.model").unwrap(),
                        value: ValueRef::new(serde_json::json!("gpt-4")),
                    }],
                }],
                target: "custom".into(),
            },
        ];

        let serialized = serde_json::to_string(&rules).unwrap();
        assert!(serialized.contains("/openai/*"));
        assert!(serialized.contains("/custom/*"));
        assert!(serialized.contains("gpt-4"));
    }

    #[test]
    fn test_routing_rules_deserialization() {
        let json_str = r#"
        [
            {
                "match": "/openai/*",
                "target": "openai"
            },
            {
                "match": {
                    "component": "/custom/*",
                    "input": [
                        {
                            "path": "$.model",
                            "value": "gpt-4"
                        }
                    ]
                },
                "target": "custom"
            }
        ]
        "#;

        let rules: RoutingRules = serde_json::from_str(json_str).unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].match_rule[0].component, "/openai/*");
        assert_eq!(rules[0].target, "openai");
        assert_eq!(rules[1].match_rule[0].component, "/custom/*");
        assert_eq!(rules[1].target, "custom");
        assert_eq!(rules[1].match_rule[0].input.len(), 1);
        assert_eq!(
            rules[1].match_rule[0].input[0].value,
            ValueRef::new(serde_json::json!("gpt-4"))
        );
    }

    #[test]
    fn test_match_rule_string_serialization() {
        let match_rule = MatchRule {
            component: "/openai/*".to_string(),
            input: vec![],
        };

        let serialized = serde_json::to_string(&match_rule).unwrap();
        assert_eq!(serialized, "\"/openai/*\"");
    }

    #[test]
    fn test_match_rule_string_deserialization() {
        let json_str = "\"/openai/*\"";
        let match_rule: MatchRule = serde_json::from_str(json_str).unwrap();
        assert_eq!(match_rule.component, "/openai/*");
        assert!(match_rule.input.is_empty());
    }

    #[test]
    fn test_match_rule_struct_serialization() {
        let match_rule = MatchRule {
            component: "/custom/*".to_string(),
            input: vec![
                InputCondition {
                    path: JsonPath::parse("$.model").unwrap(),
                    value: ValueRef::new(serde_json::json!("gpt-4")),
                },
                InputCondition {
                    path: JsonPath::parse("$.config.temperature").unwrap(),
                    value: ValueRef::new(serde_json::json!(0.7)),
                },
            ],
        };

        let serialized = serde_json::to_string(&match_rule).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&serialized).unwrap();

        assert_eq!(parsed["component"], "/custom/*");
        assert_eq!(parsed["input"][0]["path"], "$.model");
        assert_eq!(parsed["input"][0]["value"], "gpt-4");
        assert_eq!(parsed["input"][1]["path"], "$.config.temperature");
        assert_eq!(parsed["input"][1]["value"], 0.7);
    }

    #[test]
    fn test_match_rule_struct_deserialization() {
        let json_str = r#"
        {
            "component": "/custom/*",
            "input": [
                {
                    "path": "$.model",
                    "value": "gpt-4"
                },
                {
                    "path": "$.config.temperature",
                    "value": 0.7
                }
            ]
        }
        "#;

        let match_rule: MatchRule = serde_json::from_str(json_str).unwrap();
        assert_eq!(match_rule.component, "/custom/*");
        assert_eq!(match_rule.input.len(), 2);
        assert_eq!(
            match_rule.input[0].value,
            ValueRef::new(serde_json::json!("gpt-4"))
        );
        assert_eq!(
            match_rule.input[1].value,
            ValueRef::new(serde_json::json!(0.7))
        );
    }

    #[test]
    fn test_routing_rule_with_multiple_match_rules() {
        let json_str = r#"
        {
            "match": [
                "/openai/*",
                {
                    "component": "/custom/*",
                    "input": [
                        {
                            "path": "$.model",
                            "value": "gpt-4"
                        }
                    ]
                }
            ],
            "target": "mixed"
        }
        "#;

        let routing_rule: RoutingRule = serde_json::from_str(json_str).unwrap();
        assert_eq!(routing_rule.match_rule.len(), 2);
        assert_eq!(routing_rule.match_rule[0].component, "/openai/*");
        assert!(routing_rule.match_rule[0].input.is_empty());
        assert_eq!(routing_rule.match_rule[1].component, "/custom/*");
        assert_eq!(routing_rule.match_rule[1].input.len(), 1);
        assert_eq!(routing_rule.target, "mixed");
    }

    #[test]
    fn test_routing_rule_single_match_rule() {
        let json_str = r#"
        {
            "match": "/openai/*",
            "target": "openai"
        }
        "#;

        let routing_rule: RoutingRule = serde_json::from_str(json_str).unwrap();
        assert_eq!(routing_rule.match_rule.len(), 1);
        assert_eq!(routing_rule.match_rule[0].component, "/openai/*");
        assert_eq!(routing_rule.target, "openai");
    }

    #[test]
    fn test_input_filter_serialization() {
        let filter = InputCondition {
            path: JsonPath::parse("$.config.model").unwrap(),
            value: ValueRef::new(serde_json::json!("claude-3-sonnet")),
        };

        let serialized = serde_json::to_string(&filter).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&serialized).unwrap();

        assert_eq!(parsed["path"], "$.config.model");
        assert_eq!(parsed["value"], "claude-3-sonnet");
    }

    #[test]
    fn test_input_filter_deserialization() {
        let json_str = r#"
        {
            "path": "$.config.model",
            "value": "claude-3-sonnet"
        }
        "#;

        let filter: InputCondition = serde_json::from_str(json_str).unwrap();
        assert_eq!(filter.path.to_string(), "$.config.model");
        assert_eq!(
            filter.value,
            ValueRef::new(serde_json::json!("claude-3-sonnet"))
        );
    }

    #[test]
    fn test_input_filter_with_complex_values() {
        let json_str = r#"
        {
            "path": "$.config",
            "value": {
                "temperature": 0.7,
                "max_tokens": 100,
                "enabled": true
            }
        }
        "#;

        let filter: InputCondition = serde_json::from_str(json_str).unwrap();
        assert_eq!(filter.path.to_string(), "$.config");

        let expected_value = ValueRef::new(serde_json::json!({
            "temperature": 0.7,
            "max_tokens": 100,
            "enabled": true
        }));
        assert_eq!(filter.value, expected_value);
    }

    #[test]
    fn test_empty_routing_rules() {
        let rules = RoutingRules::default();
        assert!(rules.is_empty());

        let serialized = serde_json::to_string(&rules).unwrap();
        let deserialized: RoutingRules = serde_json::from_str(&serialized).unwrap();
        assert!(deserialized.is_empty());
    }

    #[test]
    fn test_routing_rules_roundtrip() {
        let original = vec![
            RoutingRule {
                match_rule: vec![MatchRule {
                    component: "/openai/*".to_string(),
                    input: vec![],
                }],
                target: "openai".into(),
            },
            RoutingRule {
                match_rule: vec![
                    MatchRule {
                        component: "/custom/*".to_string(),
                        input: vec![InputCondition {
                            path: JsonPath::parse("$.model").unwrap(),
                            value: ValueRef::new(serde_json::json!("gpt-4")),
                        }],
                    },
                    MatchRule {
                        component: "/fallback/*".to_string(),
                        input: vec![],
                    },
                ],
                target: "custom".into(),
            },
        ];

        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: RoutingRules = serde_json::from_str(&serialized).unwrap();

        assert_eq!(original.len(), deserialized.len());
        assert_eq!(
            original[0].match_rule[0].component,
            deserialized[0].match_rule[0].component
        );
        assert_eq!(original[0].target, deserialized[0].target);
        assert_eq!(
            original[1].match_rule.len(),
            deserialized[1].match_rule.len()
        );
        assert_eq!(
            original[1].match_rule[0].component,
            deserialized[1].match_rule[0].component
        );
        assert_eq!(
            original[1].match_rule[0].input[0].value,
            deserialized[1].match_rule[0].input[0].value
        );
    }
}
