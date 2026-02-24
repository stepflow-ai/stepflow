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

//! Compliance test suite for `CheckpointStore` implementations.
//!
//! This module provides a comprehensive set of tests that any `CheckpointStore` implementation
//! must pass. Use these tests to verify your implementation conforms to the trait contract.
//!
//! # Usage
//!
//! In your implementation's test module:
//!
//! ```ignore
//! #[cfg(test)]
//! mod tests {
//!     use stepflow_state::checkpoint_compliance::CheckpointComplianceTests;
//!
//!     #[tokio::test]
//!     async fn compliance_put_and_get() {
//!         let store = MyCheckpointStore::new().await.unwrap();
//!         CheckpointComplianceTests::test_put_and_get(&store).await;
//!     }
//!
//!     // ... or run all tests at once:
//!     #[tokio::test]
//!     async fn compliance_all() {
//!         CheckpointComplianceTests::run_all_isolated(|| async {
//!             MyCheckpointStore::new().await.unwrap()
//!         }).await;
//!     }
//! }
//! ```

use std::future::Future;

use bytes::Bytes;
use uuid::Uuid;

use crate::{CheckpointStore, SequenceNumber};

/// Compliance test suite for CheckpointStore implementations.
///
/// Each test method validates a specific aspect of the CheckpointStore contract.
/// Implementations should pass all tests to ensure correct behavior.
pub struct CheckpointComplianceTests;

impl CheckpointComplianceTests {
    /// Run all compliance tests against the given store implementation.
    ///
    /// This is a convenience method that runs every test in the suite.
    /// Tests are run sequentially and will panic on the first failure.
    pub async fn run_all<C: CheckpointStore>(store: &C) {
        Self::test_put_and_get(store).await;
        Self::test_get_returns_none_when_empty(store).await;
        Self::test_put_replaces_previous(store).await;
        Self::test_different_runs_independent(store).await;
        Self::test_delete(store).await;
        Self::test_delete_nonexistent_is_noop(store).await;
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
    /// CheckpointComplianceTests::run_all_isolated(|| async {
    ///     SqliteStateStore::in_memory().await.unwrap()
    /// }).await;
    /// ```
    pub async fn run_all_isolated<C, F, Fut>(factory: F)
    where
        C: CheckpointStore,
        F: Fn() -> Fut,
        Fut: Future<Output = C>,
    {
        Self::test_put_and_get(&factory().await).await;
        Self::test_get_returns_none_when_empty(&factory().await).await;
        Self::test_put_replaces_previous(&factory().await).await;
        Self::test_different_runs_independent(&factory().await).await;
        Self::test_delete(&factory().await).await;
        Self::test_delete_nonexistent_is_noop(&factory().await).await;
    }

    /// Verify that a checkpoint can be stored and retrieved.
    pub async fn test_put_and_get<C: CheckpointStore>(store: &C) {
        let run_id = Uuid::now_v7();
        let sequence = SequenceNumber::new(5);
        let data = Bytes::from_static(b"checkpoint-data");

        store
            .put_checkpoint(run_id, sequence, data.clone())
            .await
            .unwrap();

        let stored = store
            .get_latest_checkpoint(run_id)
            .await
            .unwrap()
            .expect("checkpoint should exist");
        assert_eq!(stored.sequence, sequence);
        assert_eq!(stored.data, data);
    }

    /// Verify that get returns None when no checkpoint exists.
    pub async fn test_get_returns_none_when_empty<C: CheckpointStore>(store: &C) {
        let run_id = Uuid::now_v7();
        let result = store.get_latest_checkpoint(run_id).await.unwrap();
        assert!(result.is_none());
    }

    /// Verify that putting a new checkpoint replaces the previous one.
    pub async fn test_put_replaces_previous<C: CheckpointStore>(store: &C) {
        let run_id = Uuid::now_v7();

        store
            .put_checkpoint(
                run_id,
                SequenceNumber::new(5),
                Bytes::from_static(b"first"),
            )
            .await
            .unwrap();

        store
            .put_checkpoint(
                run_id,
                SequenceNumber::new(10),
                Bytes::from_static(b"second"),
            )
            .await
            .unwrap();

        let stored = store
            .get_latest_checkpoint(run_id)
            .await
            .unwrap()
            .expect("checkpoint should exist");
        assert_eq!(stored.sequence, SequenceNumber::new(10));
        assert_eq!(stored.data, Bytes::from_static(b"second"));
    }

    /// Verify that checkpoints for different runs are independent.
    pub async fn test_different_runs_independent<C: CheckpointStore>(store: &C) {
        let run1 = Uuid::now_v7();
        let run2 = Uuid::now_v7();

        store
            .put_checkpoint(
                run1,
                SequenceNumber::new(3),
                Bytes::from_static(b"run1-data"),
            )
            .await
            .unwrap();
        store
            .put_checkpoint(
                run2,
                SequenceNumber::new(7),
                Bytes::from_static(b"run2-data"),
            )
            .await
            .unwrap();

        let cp1 = store
            .get_latest_checkpoint(run1)
            .await
            .unwrap()
            .expect("run1 checkpoint");
        let cp2 = store
            .get_latest_checkpoint(run2)
            .await
            .unwrap()
            .expect("run2 checkpoint");

        assert_eq!(cp1.sequence, SequenceNumber::new(3));
        assert_eq!(cp1.data, Bytes::from_static(b"run1-data"));
        assert_eq!(cp2.sequence, SequenceNumber::new(7));
        assert_eq!(cp2.data, Bytes::from_static(b"run2-data"));
    }

    /// Verify that deleting checkpoints removes them.
    pub async fn test_delete<C: CheckpointStore>(store: &C) {
        let run_id = Uuid::now_v7();

        store
            .put_checkpoint(
                run_id,
                SequenceNumber::new(5),
                Bytes::from_static(b"data"),
            )
            .await
            .unwrap();
        assert!(store.get_latest_checkpoint(run_id).await.unwrap().is_some());

        store.delete_checkpoints(run_id).await.unwrap();
        assert!(store.get_latest_checkpoint(run_id).await.unwrap().is_none());
    }

    /// Verify that deleting when no checkpoint exists is a no-op.
    pub async fn test_delete_nonexistent_is_noop<C: CheckpointStore>(store: &C) {
        let run_id = Uuid::now_v7();
        // Should not error
        store.delete_checkpoints(run_id).await.unwrap();
    }
}
