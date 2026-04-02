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

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use super::Router;
use super::router::ResolvedRoute;
use crate::routing::{RouteRule, RoutingConfig};
use crate::{ComponentCache, DynPlugin, PluginError, Result};
use error_stack::ResultExt as _;
use indexmap::IndexMap;
use stepflow_core::component::ComponentInfo;
use stepflow_core::values::ValueRef;
use tokio::sync::Notify;

/// Integrated plugin router that combines routing logic with plugin management.
///
/// Uses prefix-segment routing with per-plugin component tries.
/// The router is built from route configuration + component registrations
/// obtained by polling each plugin's `list_components()`.
///
/// The trie is rebuilt automatically when component registrations change
/// (e.g., when a gRPC worker connects and reports its components via
/// `ListComponents`). Reads use `RwLock` for lock-free concurrent access;
/// writes (rebuilds) are rare and take a brief exclusive lock.
pub struct PluginRouter {
    /// The underlying prefix-segment router with per-plugin tries.
    /// Wrapped in RwLock so we can rebuild it when component registrations change.
    router: RwLock<Router>,
    /// Ordered mapping from plugin names to their instances.
    /// IndexMap maintains insertion order and provides both name-based and index-based access.
    plugins: IndexMap<String, Arc<DynPlugin<'static>>>,
    /// Plugin indices map (cached for trie rebuilds).
    plugin_indices: HashMap<String, usize>,
    /// Routing configuration (cached for trie rebuilds).
    routing_config: RoutingConfig,
    /// Cached component registrations from plugins.
    component_cache: ComponentCache,
    /// True when all plugins have reported their components.
    ready: AtomicBool,
    /// Wakes waiters when `ready` becomes true.
    ready_notify: Notify,
    /// Plugins that haven't yet reported components (empty or failed at startup).
    pending_plugins: Mutex<HashSet<String>>,
}

impl PluginRouter {
    /// Create a new builder for constructing a PluginRouter.
    pub fn builder() -> PluginRouterBuilder {
        PluginRouterBuilder::new()
    }

    /// Resolve a component path and input to a plugin, component ID, path params,
    /// and route params.
    pub fn resolve(
        &self,
        component_path: &str,
        input: ValueRef,
    ) -> Result<(&Arc<DynPlugin<'static>>, ResolvedRoute)> {
        let router = self.router.read().expect("Router lock poisoned");
        let resolved = router
            .resolve(component_path, &input)
            .change_context(PluginError::InvalidInput)?;

        log::info!(
            "Component '{}' routed to '{}' on plugin '{}'",
            component_path,
            resolved.component_id,
            resolved.plugin_name,
        );

        let plugin = self
            .plugins
            .get_index(resolved.plugin_index)
            .expect("Plugin index should be valid")
            .1;

        Ok((plugin, resolved))
    }

    /// Backwards-compatible method that returns (plugin, resolved_component, route_params).
    ///
    /// TODO: Remove once all callers migrate to `resolve()`.
    pub fn get_plugin_and_component(
        &self,
        component_path: &str,
        input: ValueRef,
    ) -> Result<(
        &Arc<DynPlugin<'static>>,
        String,
        std::collections::HashMap<String, serde_json::Value>,
    )> {
        let (plugin, resolved) = self.resolve(component_path, input)?;
        Ok((plugin, resolved.component_id, resolved.route_params))
    }

    /// Update the cached component registrations for a plugin and rebuild
    /// the routing trie if the registrations changed.
    ///
    /// Called when a `ListComponents` discovery task completes, e.g., after
    /// a gRPC worker connects and reports its available components.
    ///
    /// Returns `true` if the trie was rebuilt, `false` if registrations
    /// were unchanged.
    pub fn update_plugin_components(
        &self,
        plugin_name: &str,
        components: Vec<ComponentInfo>,
    ) -> Result<bool> {
        let changed = self
            .component_cache
            .update_if_changed(plugin_name, components);
        if changed {
            self.rebuild_router()?;

            // Remove from pending and check if all plugins are now ready
            let mut pending = self
                .pending_plugins
                .lock()
                .expect("pending_plugins lock poisoned");
            pending.remove(plugin_name);
            if pending.is_empty() && !self.ready.load(Ordering::Acquire) {
                self.ready.store(true, Ordering::Release);
                self.ready_notify.notify_waiters();
                log::info!("All plugins have reported their components — router is ready");
            }
        }
        Ok(changed)
    }

    /// Rebuild the router from the current component cache.
    fn rebuild_router(&self) -> Result<()> {
        let plugin_components = self.component_cache.all_entries();
        let new_router = Router::new(
            &self.routing_config,
            &self.plugin_indices,
            &plugin_components,
        )
        .change_context(PluginError::CreatePlugin)?;

        let mut router = self.router.write().expect("Router lock poisoned");
        *router = new_router;
        log::info!("Rebuilt routing trie with updated component registrations");
        Ok(())
    }

    /// Get all registered plugins.
    /// This returns each plugin exactly once, even if it's used by multiple routing rules.
    pub fn plugins(&self) -> impl Iterator<Item = &Arc<DynPlugin<'static>>> {
        self.plugins.values()
    }

    /// Get all registered plugins with their names.
    /// This returns each plugin exactly once with its corresponding name.
    pub fn plugins_with_names(&self) -> impl Iterator<Item = (&str, &Arc<DynPlugin<'static>>)> {
        self.plugins
            .iter()
            .map(|(name, plugin)| (name.as_str(), plugin))
    }

    /// Get the component cache.
    pub fn component_cache(&self) -> &ComponentCache {
        &self.component_cache
    }

    /// Check whether all plugins have reported their components.
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    /// Wait until all plugins have reported their components.
    ///
    /// Returns immediately if the router is already ready.
    pub async fn wait_ready(&self) {
        if self.is_ready() {
            return;
        }
        // Loop to guard against spurious wakeups
        loop {
            let notified = self.ready_notify.notified();
            if self.is_ready() {
                return;
            }
            notified.await;
            if self.is_ready() {
                return;
            }
        }
    }

    /// Wait until all plugins have reported their components, with a timeout.
    ///
    /// Returns `true` if the router became ready, `false` if the timeout elapsed.
    pub async fn wait_ready_timeout(&self, timeout: Duration) -> bool {
        if self.is_ready() {
            return true;
        }
        tokio::time::timeout(timeout, self.wait_ready())
            .await
            .is_ok()
    }

    /// Find all route prefixes that map to a given plugin.
    ///
    /// Returns prefix strings (e.g., "/builtin", "/python") for the given plugin.
    pub fn prefixes_for_plugin(&self, plugin_name: &str) -> Vec<String> {
        let router = self.router.read().expect("Router lock poisoned");
        router.prefixes_for_plugin(plugin_name)
    }
}

/// Builder for constructing a PluginRouter.
pub struct PluginRouterBuilder {
    /// Routing configuration for component path resolution.
    routing_config: RoutingConfig,
    /// Ordered map from plugin name to plugin instance.
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

    /// Add a routing prefix with rules to the plugin router being built.
    pub fn with_routing_path(mut self, path: String, rules: Vec<RouteRule>) -> Self {
        self.routing_config.routes.insert(path, rules);
        self
    }

    /// Set the complete routing configuration.
    pub fn with_routing_config(mut self, config: RoutingConfig) -> Self {
        self.routing_config = config;
        self
    }

    /// Register a plugin with the given name.
    pub fn register_plugin(mut self, name: String, plugin: Box<DynPlugin<'static>>) -> Self {
        self.plugins.insert(name, Arc::from(plugin));
        self
    }

    /// Build the PluginRouter.
    ///
    /// This polls `list_components()` from each plugin to build per-plugin
    /// component tries, then combines them with the routing configuration.
    ///
    /// For plugins whose workers haven't connected yet (e.g., gRPC pull-based),
    /// `list_components()` may return empty. The trie will be rebuilt when those
    /// workers connect and report their components via `update_plugin_components()`.
    pub async fn build(self) -> Result<PluginRouter> {
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

        // Poll list_components() from each plugin and cache the results
        let component_cache = ComponentCache::new();
        let failures = component_cache
            .refresh_all(
                self.plugins
                    .iter()
                    .map(|(name, plugin)| (name.as_str(), plugin)),
            )
            .await;

        if !failures.is_empty() {
            log::warn!(
                "Component discovery failed for {} plugin(s): {} \
                 (tries will be rebuilt when workers connect)",
                failures.len(),
                failures
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }

        // Build per-plugin component maps from cached registrations
        let plugin_components = component_cache.all_entries();

        // Determine which plugins are still pending (returned empty or failed).
        // Plugins that already returned non-empty components are considered ready.
        let failed_plugin_names: HashSet<String> =
            failures.iter().map(|(name, _)| name.clone()).collect();
        let mut pending: HashSet<String> = HashSet::new();
        for name in self.plugins.keys() {
            let has_components = plugin_components
                .get(name.as_str())
                .is_some_and(|c| !c.is_empty());
            if !has_components || failed_plugin_names.contains(name.as_str()) {
                pending.insert(name.clone());
            }
        }
        let is_ready = pending.is_empty();
        if is_ready {
            log::info!("All plugins reported components at build time — router is ready");
        } else {
            log::info!(
                "Waiting for {} plugin(s) to report components: {}",
                pending.len(),
                pending.iter().cloned().collect::<Vec<_>>().join(", ")
            );
        }

        // Create the underlying router
        let router = Router::new(&self.routing_config, &plugin_indices, &plugin_components)
            .change_context(PluginError::CreatePlugin)?;

        Ok(PluginRouter {
            router: RwLock::new(router),
            plugins: self.plugins,
            plugin_indices,
            routing_config: self.routing_config,
            component_cache,
            ready: AtomicBool::new(is_ready),
            ready_notify: Notify::new(),
            pending_plugins: Mutex::new(pending),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::rules::{RouteRule, RoutingConfig};
    use serde_json::json;
    use stepflow_core::component::ComponentInfo;
    use stepflow_core::workflow::Component;

    struct TestPlugin {
        components: Vec<ComponentInfo>,
    }

    impl TestPlugin {
        fn with_component(name: &str, path: &str) -> Self {
            Self {
                components: vec![ComponentInfo {
                    component: Component::from_string(name),
                    path: path.to_string(),
                    description: None,
                    input_schema: None,
                    output_schema: None,
                }],
            }
        }
    }

    impl crate::Plugin for TestPlugin {
        async fn ensure_initialized(
            &self,
            _env: &Arc<crate::StepflowEnvironment>,
        ) -> crate::Result<()> {
            Ok(())
        }

        async fn list_components(&self) -> crate::Result<Vec<ComponentInfo>> {
            Ok(self.components.clone())
        }

        async fn component_info(&self, _component: &Component) -> crate::Result<ComponentInfo> {
            Ok(ComponentInfo {
                component: Component::from_string("test"),
                path: "/test".to_string(),
                description: None,
                input_schema: None,
                output_schema: None,
            })
        }

        async fn start_task(
            &self,
            _request: &crate::TaskRequest,
            _run_context: &std::sync::Arc<crate::RunContext>,
            _step: Option<&stepflow_core::workflow::StepId>,
        ) -> crate::Result<()> {
            Ok(())
        }

        async fn prepare_for_retry(&self) -> crate::Result<()> {
            Ok(())
        }
    }

    fn make_rule(plugin: &str) -> RouteRule {
        RouteRule {
            conditions: vec![],
            component_allow: None,
            component_deny: None,
            plugin: plugin.to_string().into(),
            params: std::collections::HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_plugin_router_basic() {
        let test_plugin = DynPlugin::boxed(TestPlugin::with_component("example", "/example"));

        let mut routes = std::collections::HashMap::new();
        routes.insert("/test".to_string(), vec![make_rule("test")]);
        let config = RoutingConfig { routes };

        let router = PluginRouter::builder()
            .with_routing_config(config)
            .register_plugin("test".to_string(), test_plugin)
            .build()
            .await
            .unwrap();

        let input = ValueRef::new(json!({}));
        let (_plugin, component_id, _route_params) = router
            .get_plugin_and_component("/test/example", input)
            .unwrap();
        assert_eq!(component_id, "example");
    }

    #[tokio::test]
    async fn test_plugin_router_multiple_rules_same_plugin() {
        let test_plugin = DynPlugin::boxed(TestPlugin::with_component("example", "/example"));

        let mut routes = std::collections::HashMap::new();
        routes.insert("/test".to_string(), vec![make_rule("test")]);
        routes.insert("/other".to_string(), vec![make_rule("test")]);
        let config = RoutingConfig { routes };

        let router = PluginRouter::builder()
            .with_routing_config(config)
            .register_plugin("test".to_string(), test_plugin)
            .build()
            .await
            .unwrap();

        let input = ValueRef::new(json!({}));
        let (plugin1, id1, _) = router
            .get_plugin_and_component("/test/example", input.clone())
            .unwrap();
        let (plugin2, id2, _) = router
            .get_plugin_and_component("/other/example", input)
            .unwrap();

        // Both should be the same Arc instance
        assert!(Arc::ptr_eq(plugin1, plugin2));
        assert_eq!(id1, "example");
        assert_eq!(id2, "example");
    }

    #[tokio::test]
    async fn test_plugin_router_missing_plugin() {
        let mut routes = std::collections::HashMap::new();
        routes.insert("/missing".to_string(), vec![make_rule("nonexistent")]);
        let config = RoutingConfig { routes };

        let result = PluginRouter::builder()
            .with_routing_config(config)
            .build()
            .await;

        assert!(result.is_err());
    }

    /// Test that `update_plugin_components` rebuilds the trie so that
    /// newly registered components become routable.
    #[tokio::test]
    async fn test_dynamic_trie_rebuild() {
        // Start with a plugin that returns no components (simulates worker not yet connected)
        let test_plugin = DynPlugin::boxed(TestPlugin { components: vec![] });

        let mut routes = std::collections::HashMap::new();
        routes.insert("/worker".to_string(), vec![make_rule("worker")]);
        let config = RoutingConfig { routes };

        let router = PluginRouter::builder()
            .with_routing_config(config)
            .register_plugin("worker".to_string(), test_plugin)
            .build()
            .await
            .unwrap();

        // Component doesn't exist yet
        let input = ValueRef::new(json!({}));
        assert!(router.resolve("/worker/echo", input.clone()).is_err());

        // Simulate worker connecting and reporting its components
        let changed = router
            .update_plugin_components(
                "worker",
                vec![ComponentInfo {
                    component: Component::from_string("echo"),
                    path: "/echo".to_string(),
                    description: None,
                    input_schema: None,
                    output_schema: None,
                }],
            )
            .unwrap();
        assert!(changed);

        // Now it resolves
        let (_plugin, resolved) = router.resolve("/worker/echo", input).unwrap();
        assert_eq!(resolved.component_id, "echo");
    }
}
