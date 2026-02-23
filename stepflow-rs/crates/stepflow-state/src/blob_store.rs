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

//! BlobStore trait for content-addressed blob storage.
//!
//! This trait provides operations for storing and retrieving blobs (JSON data,
//! workflow definitions, and raw binary) using content-based addressing (SHA-256 hashes).
//!
//! ## Design
//!
//! Implementors provide two core methods that deal in raw bytes:
//! - [`put_blob`](BlobStore::put_blob) — store raw content bytes
//! - [`get_blob`](BlobStore::get_blob) — retrieve raw content bytes
//!
//! Convenience defaults (`store_flow`, `get_flow`, `get_blob_of_type`) are provided
//! for common typed access patterns. Callers that need typed serialization/deserialization
//! use `serde_json` directly.

use std::sync::Arc;

use error_stack::ResultExt as _;
use futures::future::{BoxFuture, FutureExt as _};
use stepflow_core::{
    BlobData, BlobId, BlobMetadata, BlobType,
    blob::BlobValue,
    workflow::{Flow, ValueRef},
};

use crate::StateError;

/// Raw blob content with type and metadata, as stored on disk/database.
///
/// This is the representation used by [`BlobStore`] implementations and returned
/// by [`get_blob`](BlobStore::get_blob). Callers deserialize `content` as needed
/// (e.g. `serde_json::from_slice` for JSON blobs).
#[derive(Debug, Clone)]
pub struct RawBlob {
    /// The raw content bytes.
    ///
    /// For `BlobType::Data` and `BlobType::Flow`, this is compact JSON.
    /// For `BlobType::Binary`, this is the raw binary data.
    pub content: Vec<u8>,
    /// The type of blob.
    pub blob_type: BlobType,
    /// Non-content metadata (filename, etc.).
    pub metadata: BlobMetadata,
}

/// Trait for content-addressed blob storage.
///
/// Blobs are immutable data identified by SHA-256 hashes of their content.
/// This provides automatic deduplication and deterministic IDs.
///
/// Implementors provide [`put_blob`](Self::put_blob) and [`get_blob`](Self::get_blob).
/// Convenience defaults are provided for flow storage and typed retrieval.
pub trait BlobStore: Send + Sync {
    /// Initialize the blob store backend (e.g., create tables, set up schema).
    ///
    /// Called by the configuration layer after the store is created and before
    /// it is used. The default implementation is a no-op, suitable for backends
    /// that require no initialization (e.g., in-memory stores).
    fn initialize_blob_store(&self) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        async { Ok(()) }.boxed()
    }

    /// Store raw content bytes as a blob and return its content-based ID.
    ///
    /// The blob ID is generated as a SHA-256 hash of `content`,
    /// providing deterministic IDs and automatic deduplication.
    ///
    /// # Arguments
    /// * `content` - The raw bytes to store (JSON bytes for Data/Flow, raw bytes for Binary)
    /// * `blob_type` - The type of blob being stored
    /// * `metadata` - Non-content metadata (filename, etc.)
    fn put_blob(
        &self,
        content: &[u8],
        blob_type: BlobType,
        metadata: BlobMetadata,
    ) -> BoxFuture<'_, error_stack::Result<BlobId, StateError>>;

    /// Retrieve raw blob content by blob ID.
    ///
    /// Returns `Ok(None)` when the blob is not found,
    /// reserving `Err` for actual failures (connection issues, corruption, etc.).
    fn get_blob(
        &self,
        blob_id: &BlobId,
    ) -> BoxFuture<'_, error_stack::Result<Option<RawBlob>, StateError>>;

    // =========================================================================
    // Convenience methods — all provided as defaults
    // =========================================================================

    /// Store a workflow as a blob and return its blob ID.
    ///
    /// Serializes the workflow to compact JSON bytes and stores with `BlobType::Flow`.
    fn store_flow(
        &self,
        workflow: Arc<Flow>,
    ) -> BoxFuture<'_, error_stack::Result<BlobId, StateError>> {
        async move {
            let content =
                serde_json::to_vec(workflow.as_ref()).change_context(StateError::Serialization)?;
            self.put_blob(&content, BlobType::Flow, BlobMetadata::default())
                .await
        }
        .boxed()
    }

    /// Retrieve a workflow by its blob ID.
    fn get_flow(
        &self,
        flow_id: &BlobId,
    ) -> BoxFuture<'_, error_stack::Result<Option<Arc<Flow>>, StateError>> {
        let flow_id = flow_id.clone();
        async move {
            match self.get_blob(&flow_id).await? {
                Some(raw) if raw.blob_type == BlobType::Flow => {
                    let flow: Flow = serde_json::from_slice(&raw.content)
                        .change_context(StateError::Serialization)?;
                    Ok(Some(Arc::new(flow)))
                }
                _ => Ok(None),
            }
        }
        .boxed()
    }

    /// Retrieve blob data only if it matches the expected type.
    fn get_blob_of_type<'a>(
        &'a self,
        blob_id: &BlobId,
        expected_type: BlobType,
    ) -> BoxFuture<'a, error_stack::Result<Option<BlobData>, StateError>> {
        let blob_id = blob_id.clone();
        async move {
            match self.get_blob(&blob_id).await? {
                Some(raw) if raw.blob_type == expected_type => {
                    let value = match raw.blob_type {
                        BlobType::Binary => BlobValue::Binary(raw.content),
                        BlobType::Data | BlobType::Flow => {
                            let json_value: serde_json::Value =
                                serde_json::from_slice(&raw.content)
                                    .change_context(StateError::Serialization)
                                    .attach_printable(
                                        "Failed to deserialize blob content as JSON",
                                    )?;
                            BlobValue::from_value_ref(ValueRef::new(json_value), raw.blob_type)
                                .change_context(StateError::Serialization)?
                        }
                    };
                    Ok(Some(BlobData::with_metadata(value, blob_id, raw.metadata)))
                }
                _ => Ok(None),
            }
        }
        .boxed()
    }
}
