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

//! Compliance test suite for `BlobStore` implementations.
//!
//! This module provides a comprehensive set of tests that any `BlobStore` implementation
//! must pass. Use these tests to verify your implementation conforms to the trait contract.
//!
//! # Usage
//!
//! In your implementation's test module:
//!
//! ```ignore
//! #[cfg(test)]
//! mod tests {
//!     use stepflow_state::blob_compliance::BlobStoreComplianceTests;
//!
//!     #[tokio::test]
//!     async fn compliance_blob_round_trip() {
//!         let store = MyBlobStore::new().await.unwrap();
//!         BlobStoreComplianceTests::test_blob_round_trip(&store).await;
//!     }
//!
//!     // ... or run all tests at once:
//!     #[tokio::test]
//!     async fn compliance_all() {
//!         let store = MyBlobStore::new().await.unwrap();
//!         BlobStoreComplianceTests::run_all(&store).await;
//!     }
//! }
//! ```

use std::future::Future;
use std::sync::Arc;

use serde_json::json;
use stepflow_core::BlobMetadata;
use stepflow_core::blob::BlobType;
use stepflow_core::workflow::{FlowBuilder, StepBuilder};
use stepflow_core::{BlobId, ValueExpr};

use crate::BlobStore;

/// Compliance test suite for BlobStore implementations.
///
/// Each test method validates a specific aspect of the BlobStore contract.
/// Implementations should pass all tests to ensure correct behavior.
pub struct BlobStoreComplianceTests;

impl BlobStoreComplianceTests {
    /// Run all compliance tests against the given store implementation.
    ///
    /// This is a convenience method that runs every test in the suite.
    /// Tests are run sequentially and will panic on the first failure.
    pub async fn run_all<B: BlobStore>(store: &B) {
        Self::test_blob_round_trip(store).await;
        Self::test_blob_deduplication(store).await;
        Self::test_blob_types(store).await;
        Self::test_flow_round_trip(store).await;
        Self::test_get_blob_not_found(store).await;
        Self::test_get_blob_of_type_with_wrong_type(store).await;
        Self::test_get_blob_of_type_not_found(store).await;
        Self::test_binary_blob_round_trip(store).await;
        Self::test_binary_blob_deduplication(store).await;
        Self::test_blob_metadata_filename(store).await;
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
    /// BlobStoreComplianceTests::run_all_isolated(|| async {
    ///     SqliteStateStore::in_memory().await.unwrap()
    /// }).await;
    /// ```
    pub async fn run_all_isolated<B, F, Fut>(factory: F)
    where
        B: BlobStore,
        F: Fn() -> Fut,
        Fut: Future<Output = B>,
    {
        Self::test_blob_round_trip(&factory().await).await;
        Self::test_blob_deduplication(&factory().await).await;
        Self::test_blob_types(&factory().await).await;
        Self::test_flow_round_trip(&factory().await).await;
        Self::test_get_blob_not_found(&factory().await).await;
        Self::test_get_blob_of_type_with_wrong_type(&factory().await).await;
        Self::test_get_blob_of_type_not_found(&factory().await).await;
        Self::test_binary_blob_round_trip(&factory().await).await;
        Self::test_binary_blob_deduplication(&factory().await).await;
        Self::test_blob_metadata_filename(&factory().await).await;
    }

    // =========================================================================
    // Core Blob Storage Tests
    // =========================================================================

    /// Test that blob data can be stored and retrieved correctly.
    ///
    /// Contract: put_blob followed by get_blob returns the same data.
    pub async fn test_blob_round_trip<B: BlobStore>(store: &B) {
        let data = json!({"key": "value", "number": 42});
        let content = serde_json::to_vec(&data).unwrap();

        let blob_id = store
            .put_blob(&content, BlobType::Data, Default::default())
            .await
            .expect("put_blob should succeed");

        let raw = store
            .get_blob(&blob_id)
            .await
            .expect("get_blob should succeed")
            .expect("Blob should exist");

        let retrieved: serde_json::Value = serde_json::from_slice(&raw.content).unwrap();
        assert_eq!(retrieved, data, "Retrieved data should match stored data");
        assert_eq!(
            raw.blob_type,
            BlobType::Data,
            "Retrieved blob type should match"
        );
    }

    /// Test that storing the same data twice returns the same blob ID.
    ///
    /// Contract: Blob IDs are content-addressed, so identical content produces identical IDs.
    pub async fn test_blob_deduplication<B: BlobStore>(store: &B) {
        let data = json!({"dedup": "test", "value": 123});
        let content = serde_json::to_vec(&data).unwrap();

        let blob_id1 = store
            .put_blob(&content, BlobType::Data, Default::default())
            .await
            .expect("first put_blob should succeed");

        let blob_id2 = store
            .put_blob(&content, BlobType::Data, Default::default())
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
    pub async fn test_blob_types<B: BlobStore>(store: &B) {
        let data = json!({"test": "blob_types"});
        let content = serde_json::to_vec(&data).unwrap();

        // Store as Data type
        let data_blob_id = store
            .put_blob(&content, BlobType::Data, Default::default())
            .await
            .expect("put_blob Data should succeed");

        let data_blob = store
            .get_blob(&data_blob_id)
            .await
            .expect("get_blob should succeed")
            .expect("Blob should exist");

        assert_eq!(data_blob.blob_type, BlobType::Data);

        // Store different content as Flow type
        let flow_data = json!({"flow": "definition"});
        let flow_content = serde_json::to_vec(&flow_data).unwrap();
        let flow_blob_id = store
            .put_blob(&flow_content, BlobType::Flow, Default::default())
            .await
            .expect("put_blob Flow should succeed");

        let flow_blob = store
            .get_blob(&flow_blob_id)
            .await
            .expect("get_blob should succeed")
            .expect("Blob should exist");

        assert_eq!(flow_blob.blob_type, BlobType::Flow);
    }

    /// Test that get_blob returns None for non-existent blob.
    ///
    /// Contract: get_blob returns Ok(None) for unknown blob ID.
    pub async fn test_get_blob_not_found<B: BlobStore>(store: &B) {
        let fake_content = serde_json::to_vec(&json!({"nonexistent": true})).unwrap();
        let fake_id = BlobId::from_binary(&fake_content).unwrap();

        let result = store
            .get_blob(&fake_id)
            .await
            .expect("get_blob should not error for not found");

        assert!(
            result.is_none(),
            "get_blob with unknown ID should return None"
        );
    }

    // =========================================================================
    // Flow Storage Tests (convenience methods)
    // =========================================================================

    /// Test that workflows can be stored and retrieved via store_flow/get_flow.
    ///
    /// Contract: store_flow followed by get_flow returns the same workflow structure.
    pub async fn test_flow_round_trip<B: BlobStore>(store: &B) {
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

    // =========================================================================
    // get_blob_of_type Tests
    // =========================================================================

    /// Test that get_blob_of_type returns None for wrong type.
    ///
    /// Contract: get_blob_of_type returns None if blob exists but type doesn't match.
    pub async fn test_get_blob_of_type_with_wrong_type<B: BlobStore>(store: &B) {
        let data = json!({"test": "wrong_type"});
        let content = serde_json::to_vec(&data).unwrap();

        // Store as Data type
        let blob_id = store
            .put_blob(&content, BlobType::Data, Default::default())
            .await
            .expect("put_blob should succeed");

        // Try to get as Flow type
        let result = store
            .get_blob_of_type(&blob_id, BlobType::Flow)
            .await
            .expect("get_blob_of_type should not error");

        assert!(
            result.is_none(),
            "get_blob_of_type with wrong type should return None"
        );

        // Verify it works with correct type
        let result = store
            .get_blob_of_type(&blob_id, BlobType::Data)
            .await
            .expect("get_blob_of_type should not error");

        assert!(
            result.is_some(),
            "get_blob_of_type with correct type should return Some"
        );
    }

    // =========================================================================
    // Binary Blob Tests
    // =========================================================================

    /// Test that binary data can be stored and retrieved correctly.
    ///
    /// Contract: put_blob with Binary type followed by get_blob
    /// returns the same bytes.
    pub async fn test_binary_blob_round_trip<B: BlobStore>(store: &B) {
        let data = b"Hello, binary world! \x00\x01\x02\xff";

        let blob_id = store
            .put_blob(data, BlobType::Binary, Default::default())
            .await
            .expect("put_blob should succeed");

        let raw = store
            .get_blob(&blob_id)
            .await
            .expect("get_blob should succeed")
            .expect("Blob should exist");

        assert_eq!(
            raw.content.as_slice(),
            data,
            "Retrieved binary data should match stored data"
        );
        assert_eq!(raw.blob_type, BlobType::Binary);
    }

    /// Test that storing the same binary data twice returns the same blob ID.
    ///
    /// Contract: Binary blob IDs are content-addressed (SHA-256 of raw bytes).
    pub async fn test_binary_blob_deduplication<B: BlobStore>(store: &B) {
        let data = b"dedup binary test data";

        let blob_id1 = store
            .put_blob(data, BlobType::Binary, Default::default())
            .await
            .expect("first put_blob should succeed");

        let blob_id2 = store
            .put_blob(data, BlobType::Binary, Default::default())
            .await
            .expect("second put_blob should succeed");

        assert_eq!(
            blob_id1, blob_id2,
            "Same binary content should produce same blob ID"
        );
    }

    // =========================================================================
    // Filename Metadata Tests
    // =========================================================================

    /// Test that filenames can be stored and retrieved via BlobMetadata.
    ///
    /// Contract: put_blob with metadata containing a filename, then get_blob returns
    /// the filename. Blobs without metadata have no filename.
    pub async fn test_blob_metadata_filename<B: BlobStore>(store: &B) {
        let data = json!({"filename_test": true});
        let content = serde_json::to_vec(&data).unwrap();

        // Store without filename
        let blob_id_no_name = store
            .put_blob(&content, BlobType::Data, Default::default())
            .await
            .expect("put_blob should succeed");

        let raw = store
            .get_blob(&blob_id_no_name)
            .await
            .expect("get_blob should succeed")
            .expect("Blob should exist");
        assert_eq!(
            raw.metadata.filename, None,
            "Blob without metadata filename should have no filename"
        );

        // Store different data WITH a filename
        let data2 = json!({"filename_test": "with_name"});
        let content2 = serde_json::to_vec(&data2).unwrap();
        let metadata = BlobMetadata {
            filename: Some("test-file.json".to_string()),
        };

        let blob_id = store
            .put_blob(&content2, BlobType::Data, metadata)
            .await
            .expect("put_blob with metadata should succeed");

        let raw = store
            .get_blob(&blob_id)
            .await
            .expect("get_blob should succeed")
            .expect("Blob should exist");
        assert_eq!(
            raw.metadata.filename.as_deref(),
            Some("test-file.json"),
            "Filename should be returned from metadata"
        );

        // Data should be correct
        let retrieved: serde_json::Value = serde_json::from_slice(&raw.content).unwrap();
        assert_eq!(retrieved, data2, "Blob data should match what was stored");
    }

    /// Test that get_blob_of_type returns None for non-existent blob.
    ///
    /// Contract: get_blob_of_type returns None (not error) for unknown blob ID.
    pub async fn test_get_blob_of_type_not_found<B: BlobStore>(store: &B) {
        let fake_content = serde_json::to_vec(&json!({"not_found": true})).unwrap();
        let fake_id = BlobId::from_binary(&fake_content).unwrap();

        let result = store
            .get_blob_of_type(&fake_id, BlobType::Data)
            .await
            .expect("get_blob_of_type should not error for not found");

        assert!(
            result.is_none(),
            "get_blob_of_type with unknown ID should return None"
        );
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
    async fn in_memory_blob_compliance() {
        BlobStoreComplianceTests::run_all_isolated(|| async { InMemoryStateStore::new() }).await;
    }
}
