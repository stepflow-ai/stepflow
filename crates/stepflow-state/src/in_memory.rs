use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};

use crate::{Result, StateError};
use stepflow_core::{blob::BlobId, workflow::ValueRef};
use tokio::sync::RwLock;

use crate::StateStore;

/// In-memory implementation of StateStore.
///
/// This provides a simple, fast storage implementation suitable for
/// single-process execution. In the future, this can be extended with
/// persistent storage backends for distributed or long-running workflows.
pub struct InMemoryStateStore {
    /// Map from blob ID (SHA-256 hash) to stored JSON data
    blobs: Arc<RwLock<HashMap<String, ValueRef>>>,
}

impl InMemoryStateStore {
    /// Create a new in-memory state store.
    pub fn new() -> Self {
        Self {
            blobs: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemoryStateStore {
    fn default() -> Self {
        Self::new()
    }
}

impl StateStore for InMemoryStateStore {
    fn put_blob(
        &self,
        data: ValueRef,
    ) -> Pin<Box<dyn Future<Output = Result<BlobId>> + Send + '_>> {
        let blobs = self.blobs.clone();

        Box::pin(async move {
            let blob_id = BlobId::from_content(&data).map_err(|e| {
                error_stack::report!(StateError::Internal)
                    .attach_printable(format!("Failed to create blob ID: {}", e))
            })?;

            // Store the data (overwrites are fine since content is identical)
            {
                let mut blobs = blobs.write().await;
                blobs.insert(blob_id.as_str().to_string(), data);
            }

            Ok(blob_id)
        })
    }

    fn get_blob(
        &self,
        blob_id: &BlobId,
    ) -> Pin<Box<dyn Future<Output = Result<ValueRef>> + Send + '_>> {
        let blobs = self.blobs.clone();
        let blob_id_str = blob_id.as_str().to_string();

        Box::pin(async move {
            let blobs = blobs.read().await;
            blobs.get(&blob_id_str).cloned().ok_or_else(|| {
                error_stack::report!(StateError::BlobNotFound {
                    blob_id: blob_id_str.clone()
                })
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_blob_storage() {
        let store = InMemoryStateStore::new();

        // Test data
        let test_data = json!({"message": "Hello, world!", "count": 42});
        let value_ref = ValueRef::new(test_data.clone());

        // Create blob
        let blob_id = store.put_blob(value_ref.clone()).await.unwrap();

        // Blob ID should be deterministic (SHA-256 hash)
        assert_eq!(blob_id.as_str().len(), 64); // SHA-256 produces 64 hex characters

        // Retrieve blob
        let retrieved = store.get_blob(&blob_id).await.unwrap();
        assert_eq!(retrieved.as_ref(), &test_data);

        // Same content should produce same blob ID
        let value_ref2 = ValueRef::new(test_data.clone());
        let blob_id2 = store.put_blob(value_ref2).await.unwrap();
        assert_eq!(blob_id, blob_id2);

        // Non-existent blob should return error
        let fake_blob_id = BlobId::new("a".repeat(64)).unwrap();
        let result = store.get_blob(&fake_blob_id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_blob_id_validation() {
        // Valid blob ID
        let valid_id = BlobId::new("a".repeat(64)).unwrap();
        assert_eq!(valid_id.as_str().len(), 64);

        // Invalid length
        let invalid_length = BlobId::new("abc".to_string());
        assert!(invalid_length.is_err());

        // Invalid characters
        let invalid_chars = BlobId::new("g".repeat(64));
        assert!(invalid_chars.is_err());
    }

    #[tokio::test]
    async fn test_blob_id_from_content() {
        let data1 = ValueRef::new(json!({"key": "value"}));
        let data2 = ValueRef::new(json!({"key": "value"}));
        let data3 = ValueRef::new(json!({"key": "different"}));

        let id1 = BlobId::from_content(&data1).unwrap();
        let id2 = BlobId::from_content(&data2).unwrap();
        let id3 = BlobId::from_content(&data3).unwrap();

        // Same content produces same ID
        assert_eq!(id1, id2);

        // Different content produces different ID
        assert_ne!(id1, id3);

        // All IDs are valid SHA-256 hashes
        assert_eq!(id1.as_str().len(), 64);
        assert_eq!(id3.as_str().len(), 64);
    }
}
