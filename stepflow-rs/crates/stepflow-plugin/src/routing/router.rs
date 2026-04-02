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

//! Prefix-segment router with per-plugin component tries.
//!
//! Routes are keyed by a leading path segment (e.g., "/python", "/builtin").
//! Each plugin has its own `matchit::Router` built from its registered component
//! paths. Resolution splits the incoming path into prefix + remainder, looks up
//! candidate plugins, evaluates conditions, then matches the remainder against
//! the selected plugin's trie.

use std::collections::HashMap;

use crate::routing::rules::{InputCondition, RoutingConfig};
use indexmap::IndexMap;
use matchit::Router as MatchitRouter;
use stepflow_core::component::ComponentInfo;
use stepflow_core::values::ValueRef;
use stepflow_core::workflow::JsonPath;

/// Error type for router operations.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum RouterError {
    #[error("No matching prefix for path: {path}")]
    NoMatchingPrefix { path: String },
    #[error("No matching component for path: {path} (prefix: {prefix})")]
    NoMatchingComponent { prefix: String, path: String },
    #[error("No matching rule found for component: {component}")]
    NoMatchingRule { component: String },
    #[error("Failed to extract value from input path: '{0}'")]
    InputPathFailed(JsonPath),
    #[error("Unknown plugin referenced in route: {0}")]
    UnknownPlugin(String),
    #[error(
        "Invalid prefix '{prefix}': must be \"/\" or a single-segment prefix like \"/python\" (no nested segments)"
    )]
    InvalidPrefix { prefix: String },
    #[error("Failed to insert route for component '{component_id}' with path '{path}': {error}")]
    ComponentInsertionFailed {
        component_id: String,
        path: String,
        error: matchit::InsertError,
    },
}

/// Result of resolving a component path through the router.
#[derive(Debug)]
pub struct ResolvedRoute {
    /// The plugin name that should handle this component.
    pub plugin_name: String,
    /// The plugin index in the ordered plugin map.
    pub plugin_index: usize,
    /// The component ID for flat lookup by the worker.
    pub component_id: String,
    /// Parameters extracted from path pattern matching.
    pub path_params: HashMap<String, String>,
    /// Transport-level parameters from the route rule config.
    pub route_params: HashMap<String, serde_json::Value>,
}

/// Per-plugin trie built from the plugin's registered component paths.
struct PluginComponentTrie {
    /// matchit router: component path → component ID
    trie: MatchitRouter<String>,
}

impl PluginComponentTrie {
    /// Build a trie from a plugin's registered components.
    fn new(components: &[ComponentInfo]) -> error_stack::Result<Self, RouterError> {
        let mut trie = MatchitRouter::new();
        for component in components {
            let path = if component.path.is_empty() {
                format!("/{}", component.component.path())
            } else {
                component.path.clone()
            };
            let component_id = component.component.path().to_string();
            trie.insert(&path, component_id.clone()).map_err(|error| {
                error_stack::report!(RouterError::ComponentInsertionFailed {
                    component_id,
                    path,
                    error,
                })
            })?;
        }
        Ok(Self { trie })
    }

    /// Match a remainder path against this trie.
    fn match_path(&self, remainder: &str) -> Option<(&str, HashMap<String, String>)> {
        let matched = self.trie.at(remainder).ok()?;
        let component_id = matched.value;
        let params: HashMap<String, String> = matched
            .params
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Some((component_id, params))
    }
}

/// Entry in the prefix map — one candidate plugin for a given prefix.
struct PrefixEntry {
    plugin_name: String,
    plugin_index: usize,
    conditions: Vec<InputCondition>,
    route_params: HashMap<String, serde_json::Value>,
    component_allow: Option<Vec<String>>,
    component_deny: Option<Vec<String>>,
}

impl PrefixEntry {
    fn evaluate_conditions(&self, input: &ValueRef) -> error_stack::Result<bool, RouterError> {
        for condition in &self.conditions {
            let input_value = input.resolve_json_path(&condition.path).ok_or_else(|| {
                error_stack::report!(RouterError::InputPathFailed(condition.path.clone()))
            })?;
            if input_value != condition.value {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn is_component_allowed(&self, component_id: &str) -> bool {
        if let Some(deny_list) = &self.component_deny
            && deny_list.iter().any(|denied| denied == component_id)
        {
            return false;
        }
        if let Some(allow_list) = &self.component_allow
            && !allow_list.iter().any(|allowed| allowed == component_id)
        {
            return false;
        }
        true
    }
}

/// Prefix-segment router with per-plugin component tries.
///
/// Resolution flow:
/// 1. Split incoming path into prefix + remainder
/// 2. Look up prefix → candidate plugins (with optional conditions)
/// 3. For each candidate (in order): evaluate conditions, then match remainder
///    against that plugin's trie. First match wins.
/// 4. Return (plugin, component_id, path_params, route_params)
pub struct Router {
    /// Prefix → ordered list of candidate plugins with conditions.
    prefix_map: IndexMap<String, Vec<PrefixEntry>>,
    /// Per-plugin component tries, keyed by plugin name.
    /// Shared across prefixes — if one plugin is referenced by multiple
    /// prefixes, they share the same trie.
    plugin_tries: HashMap<String, PluginComponentTrie>,
}

impl Router {
    /// Build a new router from routing configuration and plugin component registrations.
    ///
    /// # Arguments
    /// * `routing_config` - Prefix-to-plugin mapping
    /// * `plugin_indices` - Map from plugin name to index in the ordered plugin map
    /// * `plugin_components` - Map from plugin name to its registered components
    pub fn new(
        routing_config: &RoutingConfig,
        plugin_indices: &HashMap<String, usize>,
        plugin_components: &HashMap<String, Vec<ComponentInfo>>,
    ) -> error_stack::Result<Self, RouterError> {
        // Build per-plugin tries
        let mut plugin_tries = HashMap::new();
        for (plugin_name, components) in plugin_components {
            let trie = PluginComponentTrie::new(components)?;
            plugin_tries.insert(plugin_name.clone(), trie);
        }

        // Build prefix map from routing config
        let mut prefix_map: IndexMap<String, Vec<PrefixEntry>> = IndexMap::new();
        for (prefix, rules) in &routing_config.routes {
            // Validate prefix: must be "/" or "/<single-segment>" (no nested segments)
            if prefix != "/" {
                let without_leading = prefix.strip_prefix('/').unwrap_or(prefix);
                if without_leading.is_empty() || without_leading.contains('/') {
                    error_stack::bail!(RouterError::InvalidPrefix {
                        prefix: prefix.clone(),
                    });
                }
            }
            let mut entries = Vec::new();
            for rule in rules {
                let plugin_index = plugin_indices
                    .get(rule.plugin.as_ref())
                    .copied()
                    .ok_or_else(|| {
                        error_stack::report!(RouterError::UnknownPlugin(rule.plugin.to_string()))
                    })?;

                entries.push(PrefixEntry {
                    plugin_name: rule.plugin.to_string(),
                    plugin_index,
                    conditions: rule.conditions.clone(),
                    route_params: rule.params.clone(),
                    component_allow: rule
                        .component_allow
                        .as_ref()
                        .map(|v| v.iter().map(|s| s.to_string()).collect()),
                    component_deny: rule
                        .component_deny
                        .as_ref()
                        .map(|v| v.iter().map(|s| s.to_string()).collect()),
                });
            }
            prefix_map.insert(prefix.clone(), entries);
        }

        Ok(Self {
            prefix_map,
            plugin_tries,
        })
    }

    /// Resolve a component path to a plugin, component ID, and parameters.
    ///
    /// Tries the first-segment prefix first, then falls back to `"/"` as a
    /// catch-all prefix if no match is found.
    pub fn resolve(
        &self,
        path: &str,
        input: &ValueRef,
    ) -> error_stack::Result<ResolvedRoute, RouterError> {
        // Split path into prefix (first segment) and remainder
        let (prefix, remainder) = split_prefix(path);

        // Try first-segment prefix, then fall back to "/" catch-all
        let candidates: Vec<(&str, &str)> = if prefix == "/" {
            vec![("/", remainder)]
        } else {
            let mut v = vec![(prefix, remainder)];
            // Fall back to "/" with the full path as remainder
            if self.prefix_map.contains_key("/") {
                v.push(("/", path));
            }
            v
        };

        let mut any_prefix_matched = false;
        for (try_prefix, try_remainder) in &candidates {
            if self.prefix_map.contains_key(*try_prefix) {
                any_prefix_matched = true;
            }
            if let Some(result) = self.try_resolve(try_prefix, try_remainder, input)? {
                return Ok(result);
            }
        }

        if any_prefix_matched {
            error_stack::bail!(RouterError::NoMatchingComponent {
                prefix: prefix.to_string(),
                path: path.to_string(),
            })
        } else {
            error_stack::bail!(RouterError::NoMatchingPrefix {
                path: path.to_string(),
            })
        }
    }

    /// Try to resolve against a specific prefix and remainder.
    fn try_resolve(
        &self,
        prefix: &str,
        remainder: &str,
        input: &ValueRef,
    ) -> error_stack::Result<Option<ResolvedRoute>, RouterError> {
        let Some(entries) = self.prefix_map.get(prefix) else {
            return Ok(None);
        };

        for entry in entries {
            // Evaluate input conditions (if any)
            if !entry.conditions.is_empty() && !entry.evaluate_conditions(input)? {
                continue;
            }

            // Try to match remainder against this plugin's trie
            let Some(trie) = self.plugin_tries.get(&entry.plugin_name) else {
                // Plugin has no trie — skip to next candidate
                continue;
            };

            let Some((component_id, path_params)) = trie.match_path(remainder) else {
                continue;
            };

            // Check allow/deny lists
            if !entry.is_component_allowed(component_id) {
                continue;
            }

            return Ok(Some(ResolvedRoute {
                plugin_name: entry.plugin_name.clone(),
                plugin_index: entry.plugin_index,
                component_id: component_id.to_string(),
                path_params,
                route_params: entry.route_params.clone(),
            }));
        }

        Ok(None)
    }

    /// Find all route prefixes that map to a given plugin.
    pub fn prefixes_for_plugin(&self, plugin_name: &str) -> Vec<String> {
        self.prefix_map
            .iter()
            .filter(|(_, entries)| entries.iter().any(|e| e.plugin_name == plugin_name))
            .map(|(prefix, _)| prefix.clone())
            .collect()
    }
}

/// Split a path into the first segment (prefix) and the remainder.
///
/// Examples:
/// - `/python/core/bar` → (`/python`, `/core/bar`)
/// - `/builtin/eval` → (`/builtin`, `/eval`)
/// - `/python` → (`/python`, `/`)
/// - `/` → (`/`, `/`)
fn split_prefix(path: &str) -> (&str, &str) {
    // Skip leading slash
    let without_leading = path.strip_prefix('/').unwrap_or(path);

    // Find the next slash
    if let Some(pos) = without_leading.find('/') {
        let prefix = &path[..pos + 1]; // Include the leading slash
        let remainder = &path[pos + 1..]; // Everything after the prefix
        (prefix, remainder)
    } else {
        // No second segment — the entire path is the prefix
        (path, "/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::rules::{InputCondition, RouteRule, RoutingConfig};
    use serde_json::json;
    use stepflow_core::workflow::{Component, JsonPath};

    fn make_component(name: &str, path: &str) -> ComponentInfo {
        ComponentInfo {
            component: Component::from_string(name),
            path: path.to_string(),
            description: None,
            input_schema: None,
            output_schema: None,
        }
    }

    fn make_rule(plugin: &str) -> RouteRule {
        RouteRule {
            conditions: vec![],
            component_allow: None,
            component_deny: None,
            plugin: plugin.to_string().into(),
            params: HashMap::new(),
        }
    }

    fn make_conditional_rule(plugin: &str, json_path: &str, value: serde_json::Value) -> RouteRule {
        RouteRule {
            conditions: vec![InputCondition {
                path: JsonPath::parse(json_path).unwrap(),
                value: ValueRef::new(value),
            }],
            component_allow: None,
            component_deny: None,
            plugin: plugin.to_string().into(),
            params: HashMap::new(),
        }
    }

    // --- split_prefix tests ---

    #[test]
    fn test_split_prefix_two_segments() {
        assert_eq!(split_prefix("/python/core/bar"), ("/python", "/core/bar"));
    }

    #[test]
    fn test_split_prefix_single_segment() {
        assert_eq!(split_prefix("/python"), ("/python", "/"));
    }

    #[test]
    fn test_split_prefix_builtin_eval() {
        assert_eq!(split_prefix("/builtin/eval"), ("/builtin", "/eval"));
    }

    #[test]
    fn test_split_prefix_deep_path() {
        assert_eq!(
            split_prefix("/python/core/foo/bar/baz"),
            ("/python", "/core/foo/bar/baz")
        );
    }

    // --- Basic routing tests ---

    #[test]
    fn test_basic_routing() {
        let mut routes = HashMap::new();
        routes.insert("/builtin".to_string(), vec![make_rule("builtin")]);

        let config = RoutingConfig { routes };
        let indices = HashMap::from([("builtin".to_string(), 0)]);
        let components = HashMap::from([(
            "builtin".to_string(),
            vec![
                make_component("eval", "/eval"),
                make_component("openai", "/openai"),
            ],
        )]);

        let router = Router::new(&config, &indices, &components).unwrap();
        let input = ValueRef::new(json!({}));

        let result = router.resolve("/builtin/eval", &input).unwrap();
        assert_eq!(result.plugin_name, "builtin");
        assert_eq!(result.component_id, "eval");
        assert!(result.path_params.is_empty());

        let result = router.resolve("/builtin/openai", &input).unwrap();
        assert_eq!(result.plugin_name, "builtin");
        assert_eq!(result.component_id, "openai");
    }

    #[test]
    fn test_wildcard_component_path() {
        let mut routes = HashMap::new();
        routes.insert("/python".to_string(), vec![make_rule("python")]);

        let config = RoutingConfig { routes };
        let indices = HashMap::from([("python".to_string(), 0)]);
        let components = HashMap::from([(
            "python".to_string(),
            vec![make_component("foo", "/core/{*path}")],
        )]);

        let router = Router::new(&config, &indices, &components).unwrap();
        let input = ValueRef::new(json!({}));

        let result = router.resolve("/python/core/bar/baz", &input).unwrap();
        assert_eq!(result.plugin_name, "python");
        assert_eq!(result.component_id, "foo");
        assert_eq!(result.path_params.get("path").unwrap(), "bar/baz");
    }

    #[test]
    fn test_no_matching_prefix() {
        let routes = HashMap::new();
        let config = RoutingConfig { routes };
        let router = Router::new(&config, &HashMap::new(), &HashMap::new()).unwrap();
        let input = ValueRef::new(json!({}));

        let result = router.resolve("/unknown/component", &input);
        assert!(result.is_err());
    }

    #[test]
    fn test_no_matching_component() {
        let mut routes = HashMap::new();
        routes.insert("/builtin".to_string(), vec![make_rule("builtin")]);

        let config = RoutingConfig { routes };
        let indices = HashMap::from([("builtin".to_string(), 0)]);
        let components =
            HashMap::from([("builtin".to_string(), vec![make_component("eval", "/eval")])]);

        let router = Router::new(&config, &indices, &components).unwrap();
        let input = ValueRef::new(json!({}));

        let result = router.resolve("/builtin/nonexistent", &input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        let router_err = err.current_context();
        assert!(
            matches!(router_err, RouterError::NoMatchingComponent { .. }),
            "Expected NoMatchingComponent, got: {router_err:?}"
        );
    }

    #[test]
    fn test_invalid_multi_segment_prefix() {
        let mut routes = HashMap::new();
        routes.insert("/python/core".to_string(), vec![make_rule("python")]);

        let config = RoutingConfig { routes };
        let indices = HashMap::from([("python".to_string(), 0)]);
        let components =
            HashMap::from([("python".to_string(), vec![make_component("eval", "/eval")])]);

        let result = Router::new(&config, &indices, &components);
        let err = result
            .err()
            .expect("Expected error for multi-segment prefix");
        let router_err = err.current_context();
        assert!(
            matches!(router_err, RouterError::InvalidPrefix { .. }),
            "Expected InvalidPrefix, got: {router_err:?}"
        );
    }

    // --- Conditional routing tests ---

    #[test]
    fn test_conditional_routing() {
        let mut routes = HashMap::new();
        routes.insert(
            "/model".to_string(),
            vec![
                make_conditional_rule("openai", "$.provider", json!("openai")),
                make_rule("anthropic"),
            ],
        );

        let config = RoutingConfig { routes };
        let indices = HashMap::from([("openai".to_string(), 0), ("anthropic".to_string(), 1)]);
        let components = HashMap::from([
            ("openai".to_string(), vec![make_component("chat", "/chat")]),
            (
                "anthropic".to_string(),
                vec![make_component("chat", "/chat")],
            ),
        ]);

        let router = Router::new(&config, &indices, &components).unwrap();

        // Should route to openai when condition matches
        let input = ValueRef::new(json!({"provider": "openai"}));
        let result = router.resolve("/model/chat", &input).unwrap();
        assert_eq!(result.plugin_name, "openai");

        // Should fall through to anthropic when condition doesn't match
        let input = ValueRef::new(json!({"provider": "other"}));
        let result = router.resolve("/model/chat", &input).unwrap();
        assert_eq!(result.plugin_name, "anthropic");
    }

    // --- Fallthrough routing tests ---

    #[test]
    fn test_fallthrough_to_plugin_with_component() {
        // Plugin1 provides an optimized version of some components,
        // plugin2 provides the rest. No explicit conditions needed —
        // routing falls through based on component availability.
        let mut routes = HashMap::new();
        routes.insert(
            "/something".to_string(),
            vec![make_rule("plugin1"), make_rule("plugin2")],
        );

        let config = RoutingConfig { routes };
        let indices = HashMap::from([("plugin1".to_string(), 0), ("plugin2".to_string(), 1)]);
        let components = HashMap::from([
            (
                "plugin1".to_string(),
                vec![make_component("fast_op", "/fast_op")],
            ),
            (
                "plugin2".to_string(),
                vec![
                    make_component("fast_op", "/fast_op"),
                    make_component("slow_op", "/slow_op"),
                ],
            ),
        ]);

        let router = Router::new(&config, &indices, &components).unwrap();
        let input = ValueRef::new(json!({}));

        // fast_op is on both plugins — plugin1 wins (first match)
        let result = router.resolve("/something/fast_op", &input).unwrap();
        assert_eq!(result.plugin_name, "plugin1");
        assert_eq!(result.component_id, "fast_op");

        // slow_op is only on plugin2 — falls through from plugin1
        let result = router.resolve("/something/slow_op", &input).unwrap();
        assert_eq!(result.plugin_name, "plugin2");
        assert_eq!(result.component_id, "slow_op");
    }

    // --- Route params tests ---

    #[test]
    fn test_route_params_passed_through() {
        let mut params = HashMap::new();
        params.insert("stream".to_string(), json!("MY_STREAM"));

        let mut routes = HashMap::new();
        routes.insert(
            "/nats".to_string(),
            vec![RouteRule {
                conditions: vec![],
                component_allow: None,
                component_deny: None,
                plugin: "nats_plugin".into(),
                params,
            }],
        );

        let config = RoutingConfig { routes };
        let indices = HashMap::from([("nats_plugin".to_string(), 0)]);
        let components = HashMap::from([(
            "nats_plugin".to_string(),
            vec![make_component("worker", "/worker")],
        )]);

        let router = Router::new(&config, &indices, &components).unwrap();
        let input = ValueRef::new(json!({}));

        let result = router.resolve("/nats/worker", &input).unwrap();
        assert_eq!(
            result.route_params.get("stream").unwrap(),
            &json!("MY_STREAM")
        );
    }

    // --- Component allow/deny tests ---

    #[test]
    fn test_component_deny_list() {
        let mut routes = HashMap::new();
        routes.insert(
            "/prefix".to_string(),
            vec![
                RouteRule {
                    conditions: vec![],
                    component_allow: None,
                    component_deny: Some(vec!["blocked".into()]),
                    plugin: "plugin1".into(),
                    params: HashMap::new(),
                },
                make_rule("plugin2"),
            ],
        );

        let config = RoutingConfig { routes };
        let indices = HashMap::from([("plugin1".to_string(), 0), ("plugin2".to_string(), 1)]);
        let components = HashMap::from([
            (
                "plugin1".to_string(),
                vec![
                    make_component("allowed", "/allowed"),
                    make_component("blocked", "/blocked"),
                ],
            ),
            (
                "plugin2".to_string(),
                vec![make_component("blocked", "/blocked")],
            ),
        ]);

        let router = Router::new(&config, &indices, &components).unwrap();
        let input = ValueRef::new(json!({}));

        // "allowed" routes to plugin1
        let result = router.resolve("/prefix/allowed", &input).unwrap();
        assert_eq!(result.plugin_name, "plugin1");

        // "blocked" falls through to plugin2
        let result = router.resolve("/prefix/blocked", &input).unwrap();
        assert_eq!(result.plugin_name, "plugin2");
    }

    // --- Multiple prefixes for same plugin ---

    #[test]
    fn test_multiple_prefixes_same_plugin() {
        let mut routes = HashMap::new();
        routes.insert("/v1".to_string(), vec![make_rule("api")]);
        routes.insert("/v2".to_string(), vec![make_rule("api")]);

        let config = RoutingConfig { routes };
        let indices = HashMap::from([("api".to_string(), 0)]);
        let components =
            HashMap::from([("api".to_string(), vec![make_component("users", "/users")])]);

        let router = Router::new(&config, &indices, &components).unwrap();
        let input = ValueRef::new(json!({}));

        let result = router.resolve("/v1/users", &input).unwrap();
        assert_eq!(result.plugin_name, "api");
        assert_eq!(result.component_id, "users");

        let result = router.resolve("/v2/users", &input).unwrap();
        assert_eq!(result.plugin_name, "api");
        assert_eq!(result.component_id, "users");
    }
}
