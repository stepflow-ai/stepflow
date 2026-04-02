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

//! Cache for component registrations from plugins.
//!
//! The orchestrator polls `list_components()` from each plugin and caches
//! the results. This cache is used to build the routing trie and to serve
//! the ComponentsService API without re-querying plugins on every request.

use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use error_stack::ResultExt as _;
use stepflow_core::component::ComponentInfo;

use crate::{DynPlugin, Plugin as _, PluginError, Result};

/// Cache for component registrations from plugins.
///
/// Uses `DashMap` for lock-free concurrent reads and per-shard writes,
/// avoiding a single global lock on the entire cache.
pub struct ComponentCache {
    /// Per-plugin component registrations, keyed by plugin name.
    entries: DashMap<String, Vec<ComponentInfo>>,
}

impl ComponentCache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
        }
    }

    /// Refresh the component list for a specific plugin.
    pub async fn refresh_plugin(&self, name: &str, plugin: &Arc<DynPlugin<'static>>) -> Result<()> {
        let components = plugin
            .list_components()
            .await
            .change_context(PluginError::ComponentInfo)?;
        self.entries.insert(name.to_string(), components);
        Ok(())
    }

    /// Refresh component lists for all given plugins.
    ///
    /// Plugins that fail are logged but don't prevent other plugins from being refreshed.
    /// Returns a list of `(plugin_name, error)` for plugins that failed.
    pub async fn refresh_all(
        &self,
        plugins: impl Iterator<Item = (&str, &Arc<DynPlugin<'static>>)>,
    ) -> Vec<(String, error_stack::Report<PluginError>)> {
        let mut failures = Vec::new();
        for (name, plugin) in plugins {
            if let Err(e) = self.refresh_plugin(name, plugin).await {
                log::warn!("Failed to refresh components for plugin '{name}': {e}");
                failures.push((name.to_string(), e));
            }
        }
        failures
    }

    /// Get cached registrations for a specific plugin.
    pub fn get_plugin(&self, name: &str) -> Option<Vec<ComponentInfo>> {
        self.entries.get(name).map(|v| v.value().clone())
    }

    /// Get all cached registrations as a snapshot.
    pub fn all_entries(&self) -> HashMap<String, Vec<ComponentInfo>> {
        self.entries
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }

    /// Update a plugin's component registrations only if they differ from the
    /// cached value. Returns `true` if the cache was updated (registrations changed).
    ///
    /// This is used to detect when a worker connects with new/different components
    /// so the routing trie can be rebuilt.
    pub fn update_if_changed(&self, name: &str, components: Vec<ComponentInfo>) -> bool {
        // Compare by component IDs and paths — schemas/descriptions don't affect routing
        let is_same = self.entries.get(name).is_some_and(|existing| {
            if existing.len() != components.len() {
                return false;
            }
            existing
                .iter()
                .zip(components.iter())
                .all(|(a, b)| a.component.path() == b.component.path() && a.path == b.path)
        });

        if is_same {
            return false;
        }

        self.entries.insert(name.to_string(), components);
        true
    }

    /// Check if any components are cached.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for ComponentCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stepflow_core::workflow::Component;

    fn make_component_info(name: &str) -> ComponentInfo {
        ComponentInfo {
            component: Component::from_string(name),
            path: format!("/{name}"),
            description: None,
            input_schema: None,
            output_schema: None,
        }
    }

    #[test]
    fn test_new_cache_is_empty() {
        let cache = ComponentCache::new();
        assert!(cache.is_empty());
        assert!(cache.get_plugin("foo").is_none());
    }

    #[test]
    fn test_insert_and_retrieve() {
        let cache = ComponentCache::new();
        let components = vec![make_component_info("eval"), make_component_info("openai")];
        cache.entries.insert("builtin".to_string(), components);

        assert!(!cache.is_empty());
        let retrieved = cache.get_plugin("builtin").unwrap();
        assert_eq!(retrieved.len(), 2);
        assert_eq!(retrieved[0].component.path(), "eval");
        assert_eq!(retrieved[1].component.path(), "openai");
    }

    #[test]
    fn test_all_entries() {
        let cache = ComponentCache::new();
        cache
            .entries
            .insert("plugin_a".to_string(), vec![make_component_info("foo")]);
        cache
            .entries
            .insert("plugin_b".to_string(), vec![make_component_info("bar")]);

        let all = cache.all_entries();
        assert_eq!(all.len(), 2);
        assert!(all.contains_key("plugin_a"));
        assert!(all.contains_key("plugin_b"));
    }

    #[test]
    fn test_overwrite_on_refresh() {
        let cache = ComponentCache::new();
        cache
            .entries
            .insert("plugin".to_string(), vec![make_component_info("old")]);
        cache
            .entries
            .insert("plugin".to_string(), vec![make_component_info("new")]);

        let retrieved = cache.get_plugin("plugin").unwrap();
        assert_eq!(retrieved.len(), 1);
        assert_eq!(retrieved[0].component.path(), "new");
    }
}
