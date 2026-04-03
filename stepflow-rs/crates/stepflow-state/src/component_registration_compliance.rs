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

//! Compliance test suite for component registration methods on `MetadataStore`.
//!
//! # Usage
//!
//! ```ignore
//! #[tokio::test]
//! async fn component_registration_compliance() {
//!     ComponentRegistrationComplianceTests::run_all_isolated(|| async {
//!         MyStore::new().await.unwrap()
//!     }).await;
//! }
//! ```

use std::future::Future;

use stepflow_core::component::ComponentInfo;
use stepflow_core::workflow::Component;

use crate::MetadataStore;

/// Compliance test suite for component registration methods on MetadataStore.
pub struct ComponentRegistrationComplianceTests;

fn make_component(name: &str) -> ComponentInfo {
    ComponentInfo {
        component: Component::from_string(name),
        path: format!("/{name}"),
        description: Some(format!("Test component {name}")),
        input_schema: None,
        output_schema: None,
    }
}

impl ComponentRegistrationComplianceTests {
    /// Run all compliance tests with a fresh store for each test.
    pub async fn run_all_isolated<M, F, Fut>(factory: F)
    where
        M: MetadataStore,
        F: Fn() -> Fut,
        Fut: Future<Output = M>,
    {
        Self::test_upsert_and_list(&factory().await).await;
        Self::test_upsert_removes_stale(&factory().await).await;
        Self::test_has_registrations(&factory().await).await;
        Self::test_max_last_updated(&factory().await).await;
        Self::test_filter_by_plugin(&factory().await).await;
        Self::test_empty_components_clears_all(&factory().await).await;
    }

    /// Upserting components stores them and they can be listed back.
    pub async fn test_upsert_and_list(store: &impl MetadataStore) {
        let components = vec![make_component("foo"), make_component("bar")];

        store
            .upsert_plugin_components("test_plugin", &components)
            .await
            .unwrap();

        let stored = store.get_registered_components(None).await.unwrap();
        assert_eq!(stored.len(), 2, "should have 2 stored components");

        let names: Vec<String> = stored
            .iter()
            .map(|r| r.info.component.path().to_string())
            .collect();
        assert!(names.contains(&"foo".to_string()));
        assert!(names.contains(&"bar".to_string()));

        for reg in &stored {
            assert_eq!(reg.plugin, "test_plugin");
        }
    }

    /// Upserting a subset removes entries no longer in the list.
    pub async fn test_upsert_removes_stale(store: &impl MetadataStore) {
        let components_v1 = vec![
            make_component("a"),
            make_component("b"),
            make_component("c"),
        ];
        store
            .upsert_plugin_components("p", &components_v1)
            .await
            .unwrap();

        assert_eq!(
            store
                .get_registered_components(Some("p"))
                .await
                .unwrap()
                .len(),
            3
        );

        // Now upsert with only a and c — b should be removed
        let components_v2 = vec![make_component("a"), make_component("c")];
        store
            .upsert_plugin_components("p", &components_v2)
            .await
            .unwrap();

        let stored = store.get_registered_components(Some("p")).await.unwrap();
        assert_eq!(stored.len(), 2, "b should have been removed");

        let names: Vec<String> = stored
            .iter()
            .map(|r| r.info.component.path().to_string())
            .collect();
        assert!(names.contains(&"a".to_string()));
        assert!(names.contains(&"c".to_string()));
        assert!(!names.contains(&"b".to_string()));
    }

    /// `has_component_registrations` returns false for empty, true after insert.
    pub async fn test_has_registrations(store: &impl MetadataStore) {
        assert!(
            !store
                .has_component_registrations("empty_plugin")
                .await
                .unwrap()
        );

        store
            .upsert_plugin_components("empty_plugin", &[make_component("x")])
            .await
            .unwrap();

        assert!(
            store
                .has_component_registrations("empty_plugin")
                .await
                .unwrap()
        );
    }

    /// `component_max_last_updated` returns None for empty, Some after insert.
    pub async fn test_max_last_updated(store: &impl MetadataStore) {
        assert!(
            store
                .component_max_last_updated("ts_plugin")
                .await
                .unwrap()
                .is_none()
        );

        store
            .upsert_plugin_components("ts_plugin", &[make_component("y")])
            .await
            .unwrap();

        let ts = store.component_max_last_updated("ts_plugin").await.unwrap();
        assert!(ts.is_some(), "should have a timestamp after insert");
    }

    /// Filtering by plugin only returns that plugin's registrations.
    pub async fn test_filter_by_plugin(store: &impl MetadataStore) {
        store
            .upsert_plugin_components("plugin_a", &[make_component("comp1")])
            .await
            .unwrap();
        store
            .upsert_plugin_components("plugin_b", &[make_component("comp2")])
            .await
            .unwrap();

        let all = store.get_registered_components(None).await.unwrap();
        assert_eq!(all.len(), 2);

        let a_only = store
            .get_registered_components(Some("plugin_a"))
            .await
            .unwrap();
        assert_eq!(a_only.len(), 1);
        assert_eq!(a_only[0].plugin, "plugin_a");

        let b_only = store
            .get_registered_components(Some("plugin_b"))
            .await
            .unwrap();
        assert_eq!(b_only.len(), 1);
        assert_eq!(b_only[0].plugin, "plugin_b");
    }

    /// Upserting with an empty list removes all components for that plugin.
    pub async fn test_empty_components_clears_all(store: &impl MetadataStore) {
        store
            .upsert_plugin_components("clear_plugin", &[make_component("z")])
            .await
            .unwrap();
        assert!(
            store
                .has_component_registrations("clear_plugin")
                .await
                .unwrap()
        );

        store
            .upsert_plugin_components("clear_plugin", &[])
            .await
            .unwrap();
        assert!(
            !store
                .has_component_registrations("clear_plugin")
                .await
                .unwrap()
        );
    }
}
