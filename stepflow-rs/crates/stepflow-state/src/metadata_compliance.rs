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

//! Compliance test suite for `MetadataStore` implementations.
//!
//! This module provides a comprehensive set of tests that any `MetadataStore` implementation
//! must pass. Use these tests to verify your implementation conforms to the trait contract.
//!
//! # Usage
//!
//! In your implementation's test module:
//!
//! ```ignore
//! #[cfg(test)]
//! mod tests {
//!     use stepflow_state::metadata_compliance::MetadataComplianceTests;
//!
//!     #[tokio::test]
//!     async fn compliance_blob_round_trip() {
//!         let store = MyStateStore::new().await.unwrap();
//!         MetadataComplianceTests::test_blob_round_trip(&store).await;
//!     }
//!
//!     // ... or run all tests at once:
//!     #[tokio::test]
//!     async fn compliance_all() {
//!         let store = MyStateStore::new().await.unwrap();
//!         MetadataComplianceTests::run_all(&store).await;
//!     }
//! }
//! ```

use std::future::Future;
use std::sync::Arc;

use serde_json::json;
use stepflow_core::blob::BlobType;
use stepflow_core::status::ExecutionStatus;
use stepflow_core::workflow::{FlowBuilder, StepBuilder, ValueRef};
use stepflow_core::{BlobId, FlowResult, ValueExpr};
use stepflow_dtos::ResultOrder;
use uuid::Uuid;

use crate::{BlobStore, CreateRunParams, MetadataStore};

/// Compliance test suite for MetadataStore implementations.
///
/// Each test method validates a specific aspect of the MetadataStore contract.
/// Implementations should pass all tests to ensure correct behavior.
pub struct MetadataComplianceTests;

impl MetadataComplianceTests {
    /// Run all compliance tests against the given store implementation.
    ///
    /// This is a convenience method that runs every test in the suite.
    /// Tests are run sequentially and will panic on the first failure.
    pub async fn run_all<M: MetadataStore + BlobStore>(store: &M) {
        // Blob tests
        Self::test_blob_round_trip(store).await;
        Self::test_blob_deduplication(store).await;
        Self::test_blob_types(store).await;
        Self::test_flow_round_trip(store).await;
        Self::test_get_nonexistent_blob(store).await;

        // Run management tests
        Self::test_create_run(store).await;
        Self::test_create_run_idempotent(store).await;
        Self::test_get_nonexistent_run(store).await;
        Self::test_run_status_transitions(store).await;
        Self::test_list_runs_empty(store).await;
        Self::test_list_runs_with_status_filter(store).await;
        Self::test_list_runs_with_limit(store).await;
        Self::test_subflow_hierarchy(store).await;
        Self::test_list_runs_by_root(store).await;
        Self::test_list_runs_by_parent(store).await;
        Self::test_multilevel_hierarchy(store).await;
        Self::test_list_runs_roots_only(store).await;

        // Item result tests
        Self::test_item_results_single_item(store).await;
        Self::test_item_results_multiple_items(store).await;
        Self::test_item_results_ordering_by_index(store).await;
    }

    /// Run all compliance tests with a fresh store for each test.
    ///
    /// This version creates a new store instance for each test, ensuring complete
    /// isolation between tests. Use this when tests may interfere with each other
    /// due to shared state.
    ///
    /// # Example
    ///
    /// ```ignore
    /// MetadataComplianceTests::run_all_isolated(|| async {
    ///     SqliteStateStore::in_memory().await.unwrap()
    /// }).await;
    /// ```
    pub async fn run_all_isolated<M, F, Fut>(factory: F)
    where
        M: MetadataStore + BlobStore,
        F: Fn() -> Fut,
        Fut: Future<Output = M>,
    {
        // Blob tests
        Self::test_blob_round_trip(&factory().await).await;
        Self::test_blob_deduplication(&factory().await).await;
        Self::test_blob_types(&factory().await).await;
        Self::test_flow_round_trip(&factory().await).await;
        Self::test_get_nonexistent_blob(&factory().await).await;

        // Run management tests
        Self::test_create_run(&factory().await).await;
        Self::test_create_run_idempotent(&factory().await).await;
        Self::test_get_nonexistent_run(&factory().await).await;
        Self::test_run_status_transitions(&factory().await).await;
        Self::test_list_runs_empty(&factory().await).await;
        Self::test_list_runs_with_status_filter(&factory().await).await;
        Self::test_list_runs_with_limit(&factory().await).await;
        Self::test_subflow_hierarchy(&factory().await).await;
        Self::test_list_runs_by_root(&factory().await).await;
        Self::test_list_runs_by_parent(&factory().await).await;
        Self::test_multilevel_hierarchy(&factory().await).await;
        Self::test_list_runs_roots_only(&factory().await).await;

        // Item result tests
        Self::test_item_results_single_item(&factory().await).await;
        Self::test_item_results_multiple_items(&factory().await).await;
        Self::test_item_results_ordering_by_index(&factory().await).await;
    }

    // =========================================================================
    // Blob Storage Tests
    // =========================================================================

    /// Test that blob data can be stored and retrieved correctly.
    ///
    /// Contract: put_blob followed by get_blob returns the same data.
    pub async fn test_blob_round_trip<M: MetadataStore + BlobStore>(store: &M) {
        let data = ValueRef::new(json!({"key": "value", "number": 42}));

        let blob_id = store
            .put_blob(data.clone(), BlobType::Data)
            .await
            .expect("put_blob should succeed");

        let retrieved = store
            .get_blob(&blob_id)
            .await
            .expect("get_blob should succeed");

        assert_eq!(
            retrieved.data().as_ref(),
            data.as_ref(),
            "Retrieved data should match stored data"
        );
        assert_eq!(
            retrieved.blob_type(),
            BlobType::Data,
            "Retrieved blob type should match"
        );
    }

    /// Test that storing the same data twice returns the same blob ID.
    ///
    /// Contract: Blob IDs are content-addressed, so identical content produces identical IDs.
    pub async fn test_blob_deduplication<M: MetadataStore + BlobStore>(store: &M) {
        let data = ValueRef::new(json!({"dedup": "test", "value": 123}));

        let blob_id1 = store
            .put_blob(data.clone(), BlobType::Data)
            .await
            .expect("first put_blob should succeed");

        let blob_id2 = store
            .put_blob(data.clone(), BlobType::Data)
            .await
            .expect("second put_blob should succeed");

        assert_eq!(
            blob_id1, blob_id2,
            "Same content should produce same blob ID (deduplication)"
        );
    }

    /// Test that blob types are preserved correctly.
    ///
    /// Contract: The blob type specified during put_blob is returned by get_blob.
    pub async fn test_blob_types<M: MetadataStore + BlobStore>(store: &M) {
        let data = ValueRef::new(json!({"test": "blob_types"}));

        // Store as Data type
        let data_blob_id = store
            .put_blob(data.clone(), BlobType::Data)
            .await
            .expect("put_blob Data should succeed");

        let data_blob = store
            .get_blob(&data_blob_id)
            .await
            .expect("get_blob should succeed");

        assert_eq!(data_blob.blob_type(), BlobType::Data);

        // Store different content as Flow type
        let flow_data = ValueRef::new(json!({"flow": "definition"}));
        let flow_blob_id = store
            .put_blob(flow_data.clone(), BlobType::Flow)
            .await
            .expect("put_blob Flow should succeed");

        let flow_blob = store
            .get_blob(&flow_blob_id)
            .await
            .expect("get_blob should succeed");

        assert_eq!(flow_blob.blob_type(), BlobType::Flow);
    }

    /// Test that workflows can be stored and retrieved via store_flow/get_flow.
    ///
    /// Contract: store_flow followed by get_flow returns the same workflow structure.
    pub async fn test_flow_round_trip<M: MetadataStore + BlobStore>(store: &M) {
        let flow = Arc::new(create_test_flow());

        let flow_id = store
            .store_flow(flow.clone())
            .await
            .expect("store_flow should succeed");

        let retrieved = store
            .get_flow(&flow_id)
            .await
            .expect("get_flow should succeed")
            .expect("Flow should exist");

        // Compare key properties
        assert_eq!(
            retrieved.steps().len(),
            flow.steps().len(),
            "Flow should have same number of steps"
        );
        assert_eq!(
            retrieved.steps()[0].id,
            flow.steps()[0].id,
            "Step IDs should match"
        );
    }

    /// Test that getting a non-existent blob returns an error.
    ///
    /// Contract: get_blob with an unknown ID returns an error (not found).
    pub async fn test_get_nonexistent_blob<M: MetadataStore + BlobStore>(store: &M) {
        let fake_id = BlobId::from_content(&ValueRef::new(json!({"nonexistent": true}))).unwrap();

        let result = store.get_blob(&fake_id).await;

        assert!(
            result.is_err(),
            "get_blob with unknown ID should return error"
        );
    }

    // =========================================================================
    // Run Management Tests
    // =========================================================================

    /// Test that runs can be created and retrieved.
    ///
    /// Contract: create_run followed by get_run returns the run details.
    pub async fn test_create_run<M: MetadataStore + BlobStore>(store: &M) {
        let flow = Arc::new(create_test_flow());
        let flow_id = store
            .store_flow(flow)
            .await
            .expect("store_flow should succeed");

        let run_id = Uuid::now_v7();
        let input = ValueRef::new(json!({"x": 1}));

        let params = CreateRunParams::new(run_id, flow_id.clone(), vec![input.clone()]);

        store
            .create_run(params)
            .await
            .expect("create_run should succeed");

        let run = store
            .get_run(run_id)
            .await
            .expect("get_run should succeed")
            .expect("Run should exist");

        assert_eq!(run.summary.run_id, run_id, "Run ID should match");
        assert_eq!(run.summary.flow_id, flow_id, "Flow ID should match");
        assert_eq!(
            run.summary.status,
            ExecutionStatus::Running,
            "Initial status should be Running"
        );
        assert_eq!(run.summary.items.total, 1, "Item count should be 1");
    }

    /// Test that create_run is idempotent.
    ///
    /// Contract: Calling create_run twice with the same run_id is a no-op.
    pub async fn test_create_run_idempotent<M: MetadataStore + BlobStore>(store: &M) {
        let flow = Arc::new(create_test_flow());
        let flow_id = store
            .store_flow(flow)
            .await
            .expect("store_flow should succeed");

        let run_id = Uuid::now_v7();
        let input = ValueRef::new(json!({"x": 1}));

        let params = CreateRunParams::new(run_id, flow_id.clone(), vec![input.clone()]);

        // First create
        store
            .create_run(params.clone())
            .await
            .expect("first create_run should succeed");

        // Second create with same ID - should be no-op
        store
            .create_run(params)
            .await
            .expect("second create_run should succeed (idempotent)");

        // Verify only one run exists
        let run = store
            .get_run(run_id)
            .await
            .expect("get_run should succeed")
            .expect("Run should exist");

        assert_eq!(run.summary.run_id, run_id);
    }

    /// Test that getting a non-existent run returns None.
    ///
    /// Contract: get_run with an unknown ID returns None (not an error).
    pub async fn test_get_nonexistent_run<M: MetadataStore + BlobStore>(store: &M) {
        let fake_run_id = Uuid::now_v7();

        let result = store
            .get_run(fake_run_id)
            .await
            .expect("get_run should succeed even for unknown ID");

        assert!(result.is_none(), "Non-existent run should return None");
    }

    /// Test that run status can be updated.
    ///
    /// Contract: update_run_status changes the status returned by get_run.
    pub async fn test_run_status_transitions<M: MetadataStore + BlobStore>(store: &M) {
        let flow = Arc::new(create_test_flow());
        let flow_id = store.store_flow(flow).await.unwrap();

        let run_id = Uuid::now_v7();
        let params = CreateRunParams::new(run_id, flow_id, vec![ValueRef::new(json!({}))]);
        store.create_run(params).await.unwrap();

        // Initial status should be Running
        let run = store.get_run(run_id).await.unwrap().unwrap();
        assert_eq!(run.summary.status, ExecutionStatus::Running);

        // Update to Completed
        store
            .update_run_status(run_id, ExecutionStatus::Completed)
            .await
            .expect("update_run_status should succeed");

        let run = store.get_run(run_id).await.unwrap().unwrap();
        assert_eq!(
            run.summary.status,
            ExecutionStatus::Completed,
            "Status should be updated to Completed"
        );

        // Update to Failed
        store
            .update_run_status(run_id, ExecutionStatus::Failed)
            .await
            .expect("update_run_status should succeed");

        let run = store.get_run(run_id).await.unwrap().unwrap();
        assert_eq!(
            run.summary.status,
            ExecutionStatus::Failed,
            "Status should be updated to Failed"
        );
    }

    /// Test that list_runs returns empty when no runs exist.
    ///
    /// Contract: list_runs with no matching runs returns an empty vec.
    pub async fn test_list_runs_empty<M: MetadataStore + BlobStore>(store: &M) {
        use stepflow_dtos::RunFilters;

        // Use a filter that won't match any runs (non-existent root_run_id)
        let fake_root_id = Uuid::now_v7();
        let filters = RunFilters {
            root_run_id: Some(fake_root_id),
            ..Default::default()
        };

        let runs = store
            .list_runs(&filters)
            .await
            .expect("list_runs should succeed");

        assert!(
            runs.is_empty(),
            "Should return empty list when no runs match"
        );
    }

    /// Test that list_runs filters by status correctly.
    ///
    /// Contract: list_runs with status filter only returns runs with matching status.
    pub async fn test_list_runs_with_status_filter<M: MetadataStore + BlobStore>(store: &M) {
        use stepflow_dtos::RunFilters;

        let flow = Arc::new(create_test_flow());
        let flow_id = store.store_flow(flow).await.unwrap();

        // Create runs with different statuses - all sharing a common root for filtering
        let run1_id = Uuid::now_v7();
        let run2_id = Uuid::now_v7();
        let run3_id = Uuid::now_v7();

        // Create run1 as root
        let params = CreateRunParams::new(run1_id, flow_id.clone(), vec![ValueRef::new(json!({}))]);
        store.create_run(params).await.unwrap();

        // Create run2 and run3 as subflows of run1 so we can filter by root_run_id
        let params = CreateRunParams::new_subflow(
            run2_id,
            flow_id.clone(),
            vec![ValueRef::new(json!({}))],
            run1_id,
            run1_id,
        );
        store.create_run(params).await.unwrap();
        let params = CreateRunParams::new_subflow(
            run3_id,
            flow_id.clone(),
            vec![ValueRef::new(json!({}))],
            run1_id,
            run1_id,
        );
        store.create_run(params).await.unwrap();

        // Update statuses
        store
            .update_run_status(run1_id, ExecutionStatus::Completed)
            .await
            .unwrap();
        store
            .update_run_status(run2_id, ExecutionStatus::Failed)
            .await
            .unwrap();
        // run3 stays Running

        // Filter by Completed within our root tree
        let filters = RunFilters {
            status: Some(ExecutionStatus::Completed),
            root_run_id: Some(run1_id),
            ..Default::default()
        };
        let completed_runs = store.list_runs(&filters).await.unwrap();

        assert_eq!(completed_runs.len(), 1, "Should have 1 completed run");
        assert_eq!(completed_runs[0].run_id, run1_id);

        // Filter by Running within our root tree
        let filters = RunFilters {
            status: Some(ExecutionStatus::Running),
            root_run_id: Some(run1_id),
            ..Default::default()
        };
        let running_runs = store.list_runs(&filters).await.unwrap();

        assert_eq!(running_runs.len(), 1, "Should have 1 running run");
        assert_eq!(running_runs[0].run_id, run3_id);
    }

    /// Test that list_runs respects limit.
    ///
    /// Contract: list_runs returns at most `limit` runs.
    pub async fn test_list_runs_with_limit<M: MetadataStore + BlobStore>(store: &M) {
        use stepflow_dtos::RunFilters;

        let flow = Arc::new(create_test_flow());
        let flow_id = store.store_flow(flow).await.unwrap();

        // Create a root run
        let root_id = Uuid::now_v7();
        let params = CreateRunParams::new(root_id, flow_id.clone(), vec![ValueRef::new(json!({}))]);
        store.create_run(params).await.unwrap();

        // Create 4 more subflows under the same root
        for _ in 0..4 {
            let run_id = Uuid::now_v7();
            let params = CreateRunParams::new_subflow(
                run_id,
                flow_id.clone(),
                vec![ValueRef::new(json!({}))],
                root_id,
                root_id,
            );
            store.create_run(params).await.unwrap();
        }

        // List with limit of 2, filtering by root_run_id
        let filters = RunFilters {
            root_run_id: Some(root_id),
            limit: Some(2),
            ..Default::default()
        };
        let runs = store.list_runs(&filters).await.unwrap();

        assert_eq!(runs.len(), 2, "Should return at most 2 runs");
    }

    /// Test that subflow hierarchy is correctly stored.
    ///
    /// Contract: Subflows have correct root_run_id and parent_run_id.
    pub async fn test_subflow_hierarchy<M: MetadataStore + BlobStore>(store: &M) {
        let flow = Arc::new(create_test_flow());
        let flow_id = store.store_flow(flow).await.unwrap();

        // Create root run
        let root_run_id = Uuid::now_v7();
        let root_params =
            CreateRunParams::new(root_run_id, flow_id.clone(), vec![ValueRef::new(json!({}))]);
        store.create_run(root_params).await.unwrap();

        // Create subflow
        let subflow_run_id = Uuid::now_v7();
        let subflow_params = CreateRunParams::new_subflow(
            subflow_run_id,
            flow_id.clone(),
            vec![ValueRef::new(json!({}))],
            root_run_id,
            root_run_id,
        );
        store.create_run(subflow_params).await.unwrap();

        // Verify root run
        let root_run = store.get_run(root_run_id).await.unwrap().unwrap();
        assert_eq!(root_run.summary.root_run_id, root_run_id);
        assert!(root_run.summary.parent_run_id.is_none());

        // Verify subflow
        let subflow_run = store.get_run(subflow_run_id).await.unwrap().unwrap();
        assert_eq!(
            subflow_run.summary.root_run_id, root_run_id,
            "Subflow should have correct root_run_id"
        );
        assert_eq!(
            subflow_run.summary.parent_run_id,
            Some(root_run_id),
            "Subflow should have correct parent_run_id"
        );
    }

    /// Test finding all runs under a root using root_run_id filter.
    ///
    /// Contract: list_runs with root_run_id returns all runs in that execution tree.
    pub async fn test_list_runs_by_root<M: MetadataStore + BlobStore>(store: &M) {
        use stepflow_dtos::RunFilters;

        let flow = Arc::new(create_test_flow());
        let flow_id = store.store_flow(flow).await.unwrap();

        // Create root run
        let root_id = Uuid::now_v7();
        let params = CreateRunParams::new(root_id, flow_id.clone(), vec![ValueRef::new(json!({}))]);
        store.create_run(params).await.unwrap();

        // Create subflows under the root
        let sub1_id = Uuid::now_v7();
        let sub2_id = Uuid::now_v7();

        let params = CreateRunParams::new_subflow(
            sub1_id,
            flow_id.clone(),
            vec![ValueRef::new(json!({}))],
            root_id,
            root_id,
        );
        store.create_run(params).await.unwrap();

        let params = CreateRunParams::new_subflow(
            sub2_id,
            flow_id.clone(),
            vec![ValueRef::new(json!({}))],
            root_id,
            root_id,
        );
        store.create_run(params).await.unwrap();

        // Create an unrelated root run (should not be returned)
        let other_root_id = Uuid::now_v7();
        let params = CreateRunParams::new(
            other_root_id,
            flow_id.clone(),
            vec![ValueRef::new(json!({}))],
        );
        store.create_run(params).await.unwrap();

        // List all runs under our root
        let filters = RunFilters {
            root_run_id: Some(root_id),
            ..Default::default()
        };
        let runs = store.list_runs(&filters).await.unwrap();

        // Should include root + 2 subflows = 3 runs
        assert_eq!(runs.len(), 3, "Should return root and all subflows");

        let run_ids: Vec<_> = runs.iter().map(|r| r.run_id).collect();
        assert!(run_ids.contains(&root_id), "Should include root run");
        assert!(run_ids.contains(&sub1_id), "Should include first subflow");
        assert!(run_ids.contains(&sub2_id), "Should include second subflow");
        assert!(
            !run_ids.contains(&other_root_id),
            "Should not include unrelated root"
        );
    }

    /// Test finding direct children using parent_run_id filter.
    ///
    /// Contract: list_runs with parent_run_id returns only direct children.
    pub async fn test_list_runs_by_parent<M: MetadataStore + BlobStore>(store: &M) {
        use stepflow_dtos::RunFilters;

        let flow = Arc::new(create_test_flow());
        let flow_id = store.store_flow(flow).await.unwrap();

        // Create root run
        let root_id = Uuid::now_v7();
        let params = CreateRunParams::new(root_id, flow_id.clone(), vec![ValueRef::new(json!({}))]);
        store.create_run(params).await.unwrap();

        // Create direct children of root
        let child1_id = Uuid::now_v7();
        let child2_id = Uuid::now_v7();

        let params = CreateRunParams::new_subflow(
            child1_id,
            flow_id.clone(),
            vec![ValueRef::new(json!({}))],
            root_id,
            root_id,
        );
        store.create_run(params).await.unwrap();

        let params = CreateRunParams::new_subflow(
            child2_id,
            flow_id.clone(),
            vec![ValueRef::new(json!({}))],
            root_id,
            root_id,
        );
        store.create_run(params).await.unwrap();

        // Create grandchild (child of child1)
        let grandchild_id = Uuid::now_v7();
        let params = CreateRunParams::new_subflow(
            grandchild_id,
            flow_id.clone(),
            vec![ValueRef::new(json!({}))],
            root_id,
            child1_id, // parent is child1, not root
        );
        store.create_run(params).await.unwrap();

        // List direct children of root
        let filters = RunFilters {
            parent_run_id: Some(root_id),
            ..Default::default()
        };
        let runs = store.list_runs(&filters).await.unwrap();

        // Should only include direct children (child1, child2), not root or grandchild
        assert_eq!(runs.len(), 2, "Should return only direct children");

        let run_ids: Vec<_> = runs.iter().map(|r| r.run_id).collect();
        assert!(run_ids.contains(&child1_id), "Should include first child");
        assert!(run_ids.contains(&child2_id), "Should include second child");
        assert!(!run_ids.contains(&root_id), "Should not include root");
        assert!(
            !run_ids.contains(&grandchild_id),
            "Should not include grandchild"
        );

        // List children of child1 (should only be grandchild)
        let filters = RunFilters {
            parent_run_id: Some(child1_id),
            ..Default::default()
        };
        let runs = store.list_runs(&filters).await.unwrap();

        assert_eq!(runs.len(), 1, "Should return only grandchild");
        assert_eq!(runs[0].run_id, grandchild_id);
    }

    /// Test multilevel hierarchy traversal.
    ///
    /// Contract: root_run_id filter includes all descendants at any depth.
    pub async fn test_multilevel_hierarchy<M: MetadataStore + BlobStore>(store: &M) {
        use stepflow_dtos::RunFilters;

        let flow = Arc::new(create_test_flow());
        let flow_id = store.store_flow(flow).await.unwrap();

        // Create 3-level hierarchy: root -> child -> grandchild
        let root_id = Uuid::now_v7();
        let child_id = Uuid::now_v7();
        let grandchild_id = Uuid::now_v7();

        // Root
        let params = CreateRunParams::new(root_id, flow_id.clone(), vec![ValueRef::new(json!({}))]);
        store.create_run(params).await.unwrap();

        // Child of root
        let params = CreateRunParams::new_subflow(
            child_id,
            flow_id.clone(),
            vec![ValueRef::new(json!({}))],
            root_id,
            root_id,
        );
        store.create_run(params).await.unwrap();

        // Grandchild (child of child)
        let params = CreateRunParams::new_subflow(
            grandchild_id,
            flow_id.clone(),
            vec![ValueRef::new(json!({}))],
            root_id,  // same root
            child_id, // parent is child
        );
        store.create_run(params).await.unwrap();

        // Query by root should return all 3
        let filters = RunFilters {
            root_run_id: Some(root_id),
            ..Default::default()
        };
        let runs = store.list_runs(&filters).await.unwrap();

        assert_eq!(runs.len(), 3, "Should return all levels of hierarchy");

        let run_ids: Vec<_> = runs.iter().map(|r| r.run_id).collect();
        assert!(run_ids.contains(&root_id));
        assert!(run_ids.contains(&child_id));
        assert!(run_ids.contains(&grandchild_id));

        // Verify hierarchy relationships
        let grandchild_run = store.get_run(grandchild_id).await.unwrap().unwrap();
        assert_eq!(
            grandchild_run.summary.root_run_id, root_id,
            "Grandchild should point to root"
        );
        assert_eq!(
            grandchild_run.summary.parent_run_id,
            Some(child_id),
            "Grandchild's parent should be child"
        );
    }

    /// Test filtering for root runs only.
    ///
    /// Contract: list_runs with roots_only=true returns only top-level runs (no parent).
    pub async fn test_list_runs_roots_only<M: MetadataStore + BlobStore>(store: &M) {
        use stepflow_dtos::RunFilters;

        let flow = Arc::new(create_test_flow());
        let flow_id = store.store_flow(flow).await.unwrap();

        // Create two root runs
        let root1_id = Uuid::now_v7();
        let root2_id = Uuid::now_v7();

        let params =
            CreateRunParams::new(root1_id, flow_id.clone(), vec![ValueRef::new(json!({}))]);
        store.create_run(params).await.unwrap();

        let params =
            CreateRunParams::new(root2_id, flow_id.clone(), vec![ValueRef::new(json!({}))]);
        store.create_run(params).await.unwrap();

        // Create subflows under root1
        let sub1_id = Uuid::now_v7();
        let sub2_id = Uuid::now_v7();

        let params = CreateRunParams::new_subflow(
            sub1_id,
            flow_id.clone(),
            vec![ValueRef::new(json!({}))],
            root1_id,
            root1_id,
        );
        store.create_run(params).await.unwrap();

        let params = CreateRunParams::new_subflow(
            sub2_id,
            flow_id.clone(),
            vec![ValueRef::new(json!({}))],
            root1_id,
            root1_id,
        );
        store.create_run(params).await.unwrap();

        // Create a subflow under root2
        let sub3_id = Uuid::now_v7();
        let params = CreateRunParams::new_subflow(
            sub3_id,
            flow_id.clone(),
            vec![ValueRef::new(json!({}))],
            root2_id,
            root2_id,
        );
        store.create_run(params).await.unwrap();

        // Total: 2 roots + 3 subflows = 5 runs

        // List only root runs
        let filters = RunFilters {
            roots_only: Some(true),
            ..Default::default()
        };
        let runs = store.list_runs(&filters).await.unwrap();

        // Should include our root runs (may include other roots from prior tests)
        let run_ids: Vec<_> = runs.iter().map(|r| r.run_id).collect();
        assert!(run_ids.contains(&root1_id), "Should include first root");
        assert!(run_ids.contains(&root2_id), "Should include second root");

        // Should NOT include any of our subflows
        assert!(!run_ids.contains(&sub1_id), "Should not include subflow 1");
        assert!(!run_ids.contains(&sub2_id), "Should not include subflow 2");
        assert!(!run_ids.contains(&sub3_id), "Should not include subflow 3");

        // Verify all returned runs have no parent (this is the key contract)
        for run in &runs {
            assert!(
                run.parent_run_id.is_none(),
                "Root runs should have no parent"
            );
        }
    }

    // =========================================================================
    // Item Result Tests
    // =========================================================================

    /// Test item results for single-item runs.
    ///
    /// Contract: record_item_result and get_item_results work for single items.
    pub async fn test_item_results_single_item<M: MetadataStore + BlobStore>(store: &M) {
        let flow = Arc::new(create_test_flow());
        let flow_id = store.store_flow(flow).await.unwrap();

        let run_id = Uuid::now_v7();
        let params = CreateRunParams::new(run_id, flow_id, vec![ValueRef::new(json!({"x": 1}))]);
        store.create_run(params).await.unwrap();

        // Record item result
        let result = FlowResult::Success(ValueRef::new(json!({"output": "done"})));
        store
            .record_item_result(run_id, 0, result.clone(), Vec::new())
            .await
            .expect("record_item_result should succeed");

        // Get item results
        let results = store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .expect("get_item_results should succeed");

        assert_eq!(results.len(), 1, "Should have 1 item result");
        assert_eq!(results[0].item_index, 0);

        match &results[0].result {
            Some(FlowResult::Success(v)) => {
                assert_eq!(v.as_ref(), &json!({"output": "done"}));
            }
            _ => panic!("Expected Some(Success) result"),
        }
    }

    /// Test item results for multi-item runs.
    ///
    /// Contract: Multiple items can have their results recorded independently.
    pub async fn test_item_results_multiple_items<M: MetadataStore + BlobStore>(store: &M) {
        let flow = Arc::new(create_test_flow());
        let flow_id = store.store_flow(flow).await.unwrap();

        let run_id = Uuid::now_v7();
        let params = CreateRunParams::new(
            run_id,
            flow_id,
            vec![
                ValueRef::new(json!({"x": 1})),
                ValueRef::new(json!({"x": 2})),
                ValueRef::new(json!({"x": 3})),
            ],
        );
        store.create_run(params).await.unwrap();

        // Record results out of order
        store
            .record_item_result(
                run_id,
                2,
                FlowResult::Success(ValueRef::new(json!({"item": 2}))),
                Vec::new(),
            )
            .await
            .unwrap();
        store
            .record_item_result(
                run_id,
                0,
                FlowResult::Success(ValueRef::new(json!({"item": 0}))),
                Vec::new(),
            )
            .await
            .unwrap();
        store
            .record_item_result(
                run_id,
                1,
                FlowResult::Success(ValueRef::new(json!({"item": 1}))),
                Vec::new(),
            )
            .await
            .unwrap();

        // Get results
        let results = store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        assert_eq!(results.len(), 3, "Should have 3 item results");
    }

    /// Test that item results are ordered correctly by index.
    ///
    /// Contract: get_item_results with ByIndex order returns results sorted by item_index.
    pub async fn test_item_results_ordering_by_index<M: MetadataStore + BlobStore>(store: &M) {
        let flow = Arc::new(create_test_flow());
        let flow_id = store.store_flow(flow).await.unwrap();

        let run_id = Uuid::now_v7();
        let params = CreateRunParams::new(
            run_id,
            flow_id,
            vec![
                ValueRef::new(json!({"x": 0})),
                ValueRef::new(json!({"x": 1})),
                ValueRef::new(json!({"x": 2})),
            ],
        );
        store.create_run(params).await.unwrap();

        // Record results out of order: 2, 0, 1
        store
            .record_item_result(
                run_id,
                2,
                FlowResult::Success(ValueRef::new(json!({"idx": 2}))),
                Vec::new(),
            )
            .await
            .unwrap();
        store
            .record_item_result(
                run_id,
                0,
                FlowResult::Success(ValueRef::new(json!({"idx": 0}))),
                Vec::new(),
            )
            .await
            .unwrap();
        store
            .record_item_result(
                run_id,
                1,
                FlowResult::Success(ValueRef::new(json!({"idx": 1}))),
                Vec::new(),
            )
            .await
            .unwrap();

        // Get results ordered by index
        let results = store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        // Verify order: 0, 1, 2
        assert_eq!(results[0].item_index, 0);
        assert_eq!(results[1].item_index, 1);
        assert_eq!(results[2].item_index, 2);
    }
}

/// Create a simple test flow with one step.
fn create_test_flow() -> stepflow_core::workflow::Flow {
    FlowBuilder::test_flow()
        .steps(vec![
            StepBuilder::new("step1")
                .component("/mock/test")
                .input(ValueExpr::Input {
                    input: Default::default(),
                })
                .build(),
        ])
        .output(ValueExpr::Step {
            step: "step1".to_string(),
            path: Default::default(),
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryStateStore;

    #[tokio::test]
    async fn in_memory_metadata_compliance() {
        MetadataComplianceTests::run_all_isolated(|| async { InMemoryStateStore::new() }).await;
    }
}
