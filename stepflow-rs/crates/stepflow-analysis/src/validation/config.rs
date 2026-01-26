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

use indexmap::IndexMap;
use stepflow_plugin::routing::RoutingConfig;

use crate::{DiagnosticKind, Diagnostics, diagnostic, make_path};

/// Validate configuration structure and consistency
pub fn validate_config(
    plugins: &IndexMap<String, impl std::fmt::Debug>,
    routing: &RoutingConfig,
    diagnostics: &mut Diagnostics,
) {
    // Check that at least one plugin is configured
    if plugins.is_empty() {
        diagnostics.add(
            diagnostic!(DiagnosticKind::NoPluginsConfigured, "No plugins configured")
                .at(make_path!("plugins")),
        );
    }

    // Check that routing rules exist
    if routing.routes.is_empty() {
        diagnostics.add(
            diagnostic!(
                DiagnosticKind::NoRoutingRulesConfigured,
                "No routing rules configured"
            )
            .at(make_path!("routes")),
        );
    }

    // Validate routing rules reference existing plugins
    for (path, rules) in &routing.routes {
        for (rule_index, rule) in rules.iter().enumerate() {
            if !plugins.contains_key(rule.plugin.as_ref()) {
                let route_path = path.clone();
                let plugin = rule.plugin.as_ref().to_string();
                diagnostics.add(
                    diagnostic!(
                        DiagnosticKind::InvalidRouteReference,
                        "Route '{route_path}' references unknown plugin '{plugin}'",
                        { route_path, rule_index, plugin }
                    )
                    .at(make_path!(
                        "routes",
                        path.to_string(),
                        rule_index,
                        "plugin"
                    )),
                );
            }
        }
    }

    // Check for unused plugins (plugins not referenced by any routing rule)
    for plugin_name in plugins.keys() {
        let is_referenced = routing
            .routes
            .values()
            .flatten()
            .any(|rule| rule.plugin.as_ref() == plugin_name);
        if !is_referenced {
            let plugin = plugin_name.clone();
            diagnostics.add(
                diagnostic!(
                    DiagnosticKind::UnusedPlugin,
                    "Plugin '{plugin}' is not used by any route",
                    { plugin }
                )
                .at(make_path!("plugins", plugin_name.to_string())),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;

    use super::validate_config;
    use crate::{DiagnosticKind, Diagnostics};

    #[test]
    fn test_valid_config() {
        use std::collections::HashMap;
        use stepflow_plugin::routing::{RouteRule, RoutingConfig};

        let mut plugins = IndexMap::new();
        plugins.insert("builtin".to_string(), ());

        let mut routes = HashMap::new();
        routes.insert(
            "/{*component}".to_string(),
            vec![RouteRule {
                plugin: "builtin".into(),
                conditions: vec![],
                component_allow: None,
                component_deny: None,
                component: None,
            }],
        );

        let routing = RoutingConfig { routes };

        let mut diagnostics = Diagnostics::new();
        validate_config(&plugins, &routing, &mut diagnostics);

        assert_eq!(diagnostics.num_fatal, 0, "Expected no fatal diagnostics");
        assert_eq!(diagnostics.num_error, 0, "Expected no error diagnostics");
    }

    #[test]
    fn test_no_plugins_configured() {
        use stepflow_plugin::routing::RoutingConfig;

        let plugins: IndexMap<String, ()> = IndexMap::new();
        let routing = RoutingConfig::default();

        let mut diagnostics = Diagnostics::new();
        validate_config(&plugins, &routing, &mut diagnostics);

        assert_eq!(diagnostics.num_fatal, 0);
        assert_eq!(diagnostics.num_error, 0);
        assert_eq!(diagnostics.num_warning, 2, "Expected warning diagnostics");
        assert!(
            diagnostics
                .diagnostics
                .iter()
                .any(|d| d.kind == DiagnosticKind::NoPluginsConfigured.name())
        );
        assert!(
            diagnostics
                .diagnostics
                .iter()
                .any(|d| d.kind == DiagnosticKind::NoRoutingRulesConfigured.name())
        );
    }

    #[test]
    fn test_invalid_route_reference() {
        use std::collections::HashMap;
        use stepflow_plugin::routing::{RouteRule, RoutingConfig};

        let mut plugins = IndexMap::new();
        plugins.insert("builtin".to_string(), ());

        let mut routes = HashMap::new();
        routes.insert(
            "/{*component}".to_string(),
            vec![RouteRule {
                plugin: "nonexistent".into(), // Invalid plugin reference
                conditions: vec![],
                component_allow: None,
                component_deny: None,
                component: None,
            }],
        );

        let routing = RoutingConfig { routes };

        let mut diagnostics = Diagnostics::new();
        validate_config(&plugins, &routing, &mut diagnostics);

        assert!(diagnostics.num_error > 0, "Expected error diagnostics");
        assert!(
            diagnostics
                .diagnostics
                .iter()
                .any(|d| d.kind == DiagnosticKind::InvalidRouteReference.name())
        );
    }

    #[test]
    fn test_unused_plugin() {
        use stepflow_plugin::routing::RoutingConfig;

        let mut plugins = IndexMap::new();
        plugins.insert("builtin".to_string(), ());
        plugins.insert("unused".to_string(), ()); // Plugin not referenced by any route

        let routing = RoutingConfig::default(); // No routes

        let mut diagnostics = Diagnostics::new();
        validate_config(&plugins, &routing, &mut diagnostics);

        assert!(diagnostics.num_warning > 0, "Expected warning diagnostics");
        assert!(
            diagnostics
                .diagnostics
                .iter()
                .any(|d| d.kind == DiagnosticKind::UnusedPlugin.name())
        );
    }
}
