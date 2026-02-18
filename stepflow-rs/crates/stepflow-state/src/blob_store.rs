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
//! - [`put_blob_bytes`](BlobStore::put_blob_bytes) — store raw content bytes
//! - [`get_blob_bytes`](BlobStore::get_blob_bytes) — retrieve raw content bytes
//!
//! All typed convenience methods (`put_blob`, `get_blob_opt`, `put_blob_binary`,
//! `store_flow`, etc.) are provided as default implementations that handle
//! serialization/deserialization and delegate to the core methods.

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
/// This is the low-level representation used by [`BlobStore`] implementations.
/// Typed access (JSON, Flow, Binary) is provided by [`BlobData`] via the
/// convenience methods on the trait.
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
/// Implementors only need to provide [`put_blob_bytes`](Self::put_blob_bytes) and
/// [`get_blob_bytes`](Self::get_blob_bytes). All other methods are provided as defaults.
pub trait BlobStore: Send + Sync {
    /// Store raw content bytes as a blob and return its content-based ID.
    ///
    /// The blob ID is generated as a SHA-256 hash of `content`,
    /// providing deterministic IDs and automatic deduplication.
    ///
    /// # Arguments
    /// * `content` - The raw bytes to store (JSON bytes for Data/Flow, raw bytes for Binary)
    /// * `blob_type` - The type of blob being stored
    /// * `metadata` - Non-content metadata (filename, etc.)
    fn put_blob_bytes(
        &self,
        content: &[u8],
        blob_type: BlobType,
        metadata: BlobMetadata,
    ) -> BoxFuture<'_, error_stack::Result<BlobId, StateError>>;

    /// Retrieve raw blob content by blob ID.
    ///
    /// Implementors should return `Ok(None)` when the blob is not found,
    /// reserving `Err` for actual failures (connection issues, corruption, etc.).
    fn get_blob_bytes(
        &self,
        blob_id: &BlobId,
    ) -> BoxFuture<'_, error_stack::Result<Option<RawBlob>, StateError>>;

    // =========================================================================
    // Convenience methods — all provided as defaults
    // =========================================================================

    /// Store JSON data as a blob and return its content-based ID.
    ///
    /// Serializes the `ValueRef` to compact JSON bytes and delegates to
    /// [`put_blob_bytes`](Self::put_blob_bytes).
    ///
    /// For `BlobType::Binary`, the `ValueRef` must contain a base64-encoded string
    /// which will be decoded to raw bytes before storage.
    fn put_blob(
        &self,
        data: ValueRef,
        blob_type: BlobType,
        metadata: BlobMetadata,
    ) -> BoxFuture<'_, error_stack::Result<BlobId, StateError>> {
        async move {
            let content = serialize_for_storage(&data, &blob_type)
                .change_context(StateError::Serialization)?;
            self.put_blob_bytes(&content, blob_type, metadata).await
        }
        .boxed()
    }

    /// Retrieve blob data with type information by blob ID.
    ///
    /// Calls [`get_blob_bytes`](Self::get_blob_bytes) and reconstructs
    /// a typed [`BlobData`] from the raw bytes.
    fn get_blob_opt(
        &self,
        blob_id: &BlobId,
    ) -> BoxFuture<'_, error_stack::Result<Option<BlobData>, StateError>> {
        let blob_id = blob_id.clone();
        async move {
            match self.get_blob_bytes(&blob_id).await? {
                Some(raw) => {
                    let blob_data = raw_blob_to_blob_data(raw, blob_id)?;
                    Ok(Some(blob_data))
                }
                None => Ok(None),
            }
        }
        .boxed()
    }

    /// Retrieve blob data, returning an error if not found.
    fn get_blob(
        &self,
        blob_id: &BlobId,
    ) -> BoxFuture<'_, error_stack::Result<BlobData, StateError>> {
        let blob_id = blob_id.clone();
        async move {
            self.get_blob_opt(&blob_id).await?.ok_or_else(|| {
                error_stack::report!(StateError::BlobNotFound {
                    blob_id: blob_id.to_string(),
                })
            })
        }
        .boxed()
    }

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
            self.put_blob_bytes(&content, BlobType::Flow, BlobMetadata::default())
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
            match self.get_blob_bytes(&flow_id).await? {
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
            match self.get_blob_opt(&blob_id).await? {
                Some(blob_data) if blob_data.blob_type() == expected_type => Ok(Some(blob_data)),
                _ => Ok(None),
            }
        }
        .boxed()
    }

    /// Store raw binary data as a blob and return its content-based ID.
    ///
    /// Delegates directly to [`put_blob_bytes`](Self::put_blob_bytes) with
    /// `BlobType::Binary`. No base64 encoding is performed.
    fn put_blob_binary(
        &self,
        data: &[u8],
        metadata: BlobMetadata,
    ) -> BoxFuture<'_, error_stack::Result<BlobId, StateError>> {
        self.put_blob_bytes(data, BlobType::Binary, metadata)
    }

    /// Retrieve raw binary data by blob ID.
    ///
    /// Calls [`get_blob_bytes`](Self::get_blob_bytes) and returns the raw
    /// content bytes directly. Returns an error if the blob is not binary type.
    fn get_blob_binary(
        &self,
        blob_id: &BlobId,
    ) -> BoxFuture<'_, error_stack::Result<Vec<u8>, StateError>> {
        let blob_id = blob_id.clone();
        async move {
            let raw = self.get_blob_bytes(&blob_id).await?.ok_or_else(|| {
                error_stack::report!(StateError::BlobNotFound {
                    blob_id: blob_id.to_string(),
                })
            })?;
            if raw.blob_type != BlobType::Binary {
                return Err(error_stack::report!(StateError::Internal)).attach_printable(format!(
                    "Blob '{}' is not a binary blob (type: {:?})",
                    blob_id, raw.blob_type
                ));
            }
            Ok(raw.content)
        }
        .boxed()
    }
}

/// Serialize a `ValueRef` to raw content bytes for storage.
///
/// For `BlobType::Binary`, decodes the base64-encoded string in the ValueRef.
/// For `BlobType::Data` and `BlobType::Flow`, serializes to compact JSON.
fn serialize_for_storage(
    data: &ValueRef,
    blob_type: &BlobType,
) -> error_stack::Result<Vec<u8>, StateError> {
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;

    match blob_type {
        BlobType::Binary => {
            let base64_str = data
                .as_ref()
                .as_str()
                .ok_or_else(|| error_stack::report!(StateError::Serialization))
                .attach_printable("Binary blob data is not a base64 string")?;
            BASE64_STANDARD
                .decode(base64_str)
                .change_context(StateError::Serialization)
                .attach_printable("Failed to decode base64 binary blob data")
        }
        BlobType::Data | BlobType::Flow => serde_json::to_vec(data.as_ref())
            .change_context(StateError::Serialization)
            .attach_printable("Failed to serialize blob data as JSON"),
    }
}

/// Reconstruct a typed `BlobData` from a `RawBlob`.
fn raw_blob_to_blob_data(
    raw: RawBlob,
    blob_id: BlobId,
) -> error_stack::Result<BlobData, StateError> {
    let value = match raw.blob_type {
        BlobType::Binary => BlobValue::Binary(raw.content),
        BlobType::Data | BlobType::Flow => {
            let json_value: serde_json::Value = serde_json::from_slice(&raw.content)
                .change_context(StateError::Serialization)
                .attach_printable("Failed to deserialize blob content as JSON")?;
            BlobValue::from_value_ref(ValueRef::new(json_value), raw.blob_type)
                .change_context(StateError::Serialization)?
        }
    };
    Ok(BlobData::with_metadata(value, blob_id, raw.metadata))
}
