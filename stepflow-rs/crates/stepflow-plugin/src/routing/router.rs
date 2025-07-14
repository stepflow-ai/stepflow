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

use crate::routing::MatchRule;

use super::rules::RoutingRules;
use error_stack::ResultExt as _;
use globset::{Glob, GlobSet, GlobSetBuilder};
use stepflow_core::values::ValueRef;
use stepflow_core::workflow::JsonPath;

/// Router that matches component strings to target plugins using glob patterns
#[derive(Debug)]
pub struct Router {
    /// Compiled glob patterns for efficient matching
    glob_set: GlobSet,
    /// Match rule and target index for the corresponding pattern
    rules: Vec<(MatchRule, usize)>,
}

/// A step router that determines how to route a specific component
#[derive(Debug, Clone)]
pub enum ComponentRouter<'a> {
    /// Route directly to a specific plugin without input filtering
    Direct(usize),
    /// Apply input filters in sequence until a match is found
    Conditional {
        component: String,
        rule_indices: Vec<usize>,
        rules: &'a [(MatchRule, usize)],
    },
}

/// Error type for router operations
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum RouterError {
    #[error("Invalid glob pattern: {pattern}")]
    InvalidGlob { pattern: String },
    #[error("Invalid glob set")]
    InvalidGlobSet,
    #[error("No matching rule found for component: {component}")]
    NoMatchingRule { component: String },
    #[error("Failed to extract value from input path: '{0}'")]
    InputPathFailed(JsonPath),
    #[error("Input filter evaluation failed: {message}")]
    InputFilterFailed { message: String },
}

impl Router {
    /// Create a new router from routing rules
    pub fn new(routing_rules: RoutingRules) -> error_stack::Result<Self, RouterError> {
        let mut builder = GlobSetBuilder::new();
        let mut rules = Vec::new();

        // Build glob patterns and create mapping
        for (rule_index, rule) in routing_rules.rules.into_iter().enumerate() {
            for match_rule in rule.match_rule {
                let pattern = &match_rule.component;
                let glob = Glob::new(pattern).change_context_lazy(|| RouterError::InvalidGlob {
                    pattern: pattern.clone(),
                })?;
                builder.add(glob);
                rules.push((match_rule, rule_index));
            }
        }

        let glob_set = builder
            .build()
            .change_context(RouterError::InvalidGlobSet)?;

        Ok(Router { glob_set, rules })
    }

    /// Create a step router for a given component string
    pub fn route<'a>(
        &'a self,
        component: String,
    ) -> error_stack::Result<ComponentRouter<'a>, RouterError> {
        let matches = self.glob_set.matches(&component);

        error_stack::ensure!(
            !matches.is_empty(),
            RouterError::NoMatchingRule { component }
        );

        if matches.len() == 1 {
            let (match_rule, target) = &self.rules[matches[0]];
            if match_rule.input.is_empty() {
                return Ok(ComponentRouter::Direct(*target));
            }
        }

        Ok(ComponentRouter::Conditional {
            component,
            rule_indices: matches,
            rules: &self.rules,
        })
    }
}

impl<'a> ComponentRouter<'a> {
    pub fn direct(&self) -> Option<usize> {
        match self {
            ComponentRouter::Direct(target) => Some(*target),
            ComponentRouter::Conditional { .. } => None,
        }
    }

    /// Resolve the target plugin for this step router, optionally with input data
    pub fn resolve(&self, input: ValueRef) -> error_stack::Result<usize, RouterError> {
        match self {
            ComponentRouter::Direct(target) => Ok(*target),
            ComponentRouter::Conditional {
                component,
                rule_indices,
                rules,
            } => {
                'outer: for rule_index in rule_indices {
                    let (match_rule, target) = &rules[*rule_index];
                    for input_condition in &match_rule.input {
                        let input_value = input
                            .resolve_json_path(&input_condition.path)
                            .ok_or_else(|| {
                                error_stack::report!(RouterError::InputPathFailed(
                                    input_condition.path.clone()
                                ))
                            })?;

                        if input_value != input_condition.value {
                            continue 'outer; // No match, try next rule
                        }
                    }

                    // If we reach here, we have a match.
                    return Ok(*target);
                }

                error_stack::bail!(RouterError::NoMatchingRule {
                    component: component.clone()
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::rules::{InputCondition, MatchRule, RoutingRule};
    use serde_json::json;
    use stepflow_core::workflow::JsonPath;

    fn create_test_rules() -> RoutingRules {
        RoutingRules {
            rules: vec![
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
                            value: ValueRef::new(json!("gpt-4")),
                        }],
                    }],
                    target: "custom".into(),
                },
                RoutingRule {
                    match_rule: vec![MatchRule {
                        component: "/fallback/*".to_string(),
                        input: vec![],
                    }],
                    target: "fallback".into(),
                },
            ],
        }
    }

    #[test]
    fn test_router_creation() {
        let rules = create_test_rules();
        let router = Router::new(rules);
        assert!(router.is_ok());
    }

    #[test]
    fn test_direct_routing() {
        let rules = create_test_rules();
        let router = Router::new(rules).unwrap();

        let step_router = router.route("/openai/chat".to_string()).unwrap();
        match step_router {
            ComponentRouter::Direct(target) => assert_eq!(target, 0), // First rule index
            _ => panic!("Expected direct routing"),
        }
    }

    #[test]
    fn test_conditional_routing() {
        let rules = create_test_rules();
        let router = Router::new(rules).unwrap();

        let step_router = router.route("/custom/model".to_string()).unwrap();
        match step_router {
            ComponentRouter::Conditional {
                component,
                rule_indices,
                ..
            } => {
                assert_eq!(component, "/custom/model");
                assert!(!rule_indices.is_empty());
            }
            _ => panic!("Expected conditional routing"),
        }
    }

    #[test]
    fn test_step_router_resolve_direct() {
        let step_router = ComponentRouter::Direct(0);
        let input = ValueRef::new(serde_json::Value::Null);
        let result = step_router.resolve(input).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn test_step_router_resolve_conditional_match() {
        // Create a real router to test conditional resolution
        let rules = create_test_rules();
        let router = Router::new(rules).unwrap();

        let step_router = router.route("/custom/model".to_string()).unwrap();
        let input = ValueRef::new(json!({"model": "gpt-4"}));
        let result = step_router.resolve(input).unwrap();
        assert_eq!(result, 1); // Second rule index
    }

    #[test]
    fn test_step_router_resolve_conditional_no_match() {
        // Create a real router to test conditional resolution
        let rules = create_test_rules();
        let router = Router::new(rules).unwrap();

        let step_router = router.route("/custom/model".to_string()).unwrap();
        let input = ValueRef::new(json!({"model": "gpt-3"})); // Doesn't match "gpt-4"
        let result = step_router.resolve(input);
        assert_eq!(
            result.unwrap_err().current_context(),
            &RouterError::NoMatchingRule {
                component: "/custom/model".to_string()
            },
        );
    }

    #[test]
    fn test_path_style_component_matching() {
        let rules = RoutingRules {
            rules: vec![
                RoutingRule {
                    match_rule: vec![MatchRule {
                        component: "/openai/chat/*".to_string(),
                        input: vec![],
                    }],
                    target: "openai".into(),
                },
                RoutingRule {
                    match_rule: vec![MatchRule {
                        component: "/anthropic/*/claude".to_string(),
                        input: vec![],
                    }],
                    target: "anthropic".into(),
                },
            ],
        };

        let router = Router::new(rules).unwrap();

        // Test OpenAI pattern
        let step_router = router
            .route("/openai/chat/completions".to_string())
            .unwrap();
        match step_router {
            ComponentRouter::Direct(target) => assert_eq!(target, 0), // First rule index
            _ => panic!("Expected direct routing"),
        }

        // Test Anthropic pattern
        let step_router = router
            .route("/anthropic/models/claude".to_string())
            .unwrap();
        match step_router {
            ComponentRouter::Direct(target) => assert_eq!(target, 1), // Second rule index
            _ => panic!("Expected direct routing"),
        }
    }

    #[test]
    fn test_no_matching_rule() {
        let rules = create_test_rules();
        let router = Router::new(rules).unwrap();

        let result = router.route("/unknown/component".to_string());
        assert_eq!(
            result.unwrap_err().current_context(),
            &RouterError::NoMatchingRule {
                component: "/unknown/component".to_string()
            },
        );
    }

    #[test]
    fn test_fallback_routing() {
        let rules = RoutingRules {
            rules: vec![
                RoutingRule {
                    match_rule: vec![MatchRule {
                        component: "/service/*".to_string(),
                        input: vec![InputCondition {
                            path: JsonPath::parse("$.priority").unwrap(),
                            value: ValueRef::new(json!("high")),
                        }],
                    }],
                    target: "high_priority".into(),
                },
                RoutingRule {
                    match_rule: vec![MatchRule {
                        component: "/service/*".to_string(),
                        input: vec![],
                    }],
                    target: "default".into(),
                },
            ],
        };

        let router = Router::new(rules).unwrap();
        let step_router = router.route("/service/process".to_string()).unwrap();

        match &step_router {
            ComponentRouter::Conditional { rule_indices, .. } => {
                assert_eq!(rule_indices.len(), 2);

                // Test high priority match
                let input = ValueRef::new(json!({"priority": "high"}));
                let result = step_router.resolve(input).unwrap();
                assert_eq!(result, 0); // First rule index

                // Test fallback match
                let input = ValueRef::new(json!({"priority": "low"}));
                let result = step_router.resolve(input).unwrap();
                assert_eq!(result, 1); // Second rule index (fallback)
            }
            _ => panic!("Expected conditional routing"),
        }
    }

    #[test]
    fn test_step_router_direct_method() {
        let step_router_direct = ComponentRouter::Direct(5);
        assert_eq!(step_router_direct.direct(), Some(5));

        // Create a conditional step router using the real API
        let rules = create_test_rules();
        let router = Router::new(rules).unwrap();
        let step_router_conditional = router.route("/custom/model".to_string()).unwrap();
        assert_eq!(step_router_conditional.direct(), None);
    }

    #[test]
    fn test_rule_map_structure() {
        let rules = RoutingRules {
            rules: vec![
                RoutingRule {
                    match_rule: vec![
                        MatchRule {
                            component: "/pattern1/*".to_string(),
                            input: vec![],
                        },
                        MatchRule {
                            component: "/pattern2/*".to_string(),
                            input: vec![],
                        },
                    ],
                    target: "target1".into(),
                },
                RoutingRule {
                    match_rule: vec![MatchRule {
                        component: "/pattern3/*".to_string(),
                        input: vec![],
                    }],
                    target: "target2".into(),
                },
            ],
        };

        let router = Router::new(rules).unwrap();

        // Verify the rules structure
        assert_eq!(router.rules.len(), 3); // 2 match rules from first rule + 1 from second rule

        // Test that different patterns route correctly
        let step_router1 = router.route("/pattern1/test".to_string()).unwrap();
        if let ComponentRouter::Direct(target) = step_router1 {
            assert_eq!(target, 0); // First rule index
        } else {
            panic!("Expected direct routing");
        }

        let step_router3 = router.route("/pattern3/test".to_string()).unwrap();
        if let ComponentRouter::Direct(target) = step_router3 {
            assert_eq!(target, 1); // Second rule index
        } else {
            panic!("Expected direct routing");
        }
    }
}
