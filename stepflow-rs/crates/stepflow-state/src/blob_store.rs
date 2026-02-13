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
//! This trait provides operations for storing and retrieving blobs (JSON data and
//! workflow definitions) using content-based addressing (SHA-256 hashes).

use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use error_stack::ResultExt as _;
use futures::future::{BoxFuture, FutureExt as _};
use stepflow_core::{
    BlobData, BlobId, BlobType,
    workflow::{Flow, ValueRef},
};

use crate::StateError;

/// Trait for content-addressed blob storage.
///
/// Blobs are immutable JSON data identified by SHA-256 hashes of their content.
/// This provides automatic deduplication and deterministic IDs.
pub trait BlobStore: Send + Sync {
    /// Store JSON data as a blob and return its content-based ID.
    ///
    /// The blob ID is generated as a SHA-256 hash of the JSON content,
    /// providing deterministic IDs and automatic deduplication.
    ///
    /// # Arguments
    /// * `data` - The JSON data to store as a blob
    /// * `blob_type` - The type of blob being stored
    ///
    /// # Returns
    /// The blob ID for the stored data
    fn put_blob(
        &self,
        data: ValueRef,
        blob_type: BlobType,
    ) -> BoxFuture<'_, error_stack::Result<BlobId, StateError>>;

    /// Retrieve blob data with type information by blob ID.
    ///
    /// Implementors should return `Ok(None)` when the blob is not found,
    /// reserving `Err` for actual failures (connection issues, etc.).
    ///
    /// # Arguments
    /// * `blob_id` - The blob ID to retrieve
    ///
    /// # Returns
    /// * `Ok(Some(data))` - The blob was found
    /// * `Ok(None)` - The blob was not found
    /// * `Err(e)` - An actual error occurred
    fn get_blob_opt(
        &self,
        blob_id: &BlobId,
    ) -> BoxFuture<'_, error_stack::Result<Option<BlobData>, StateError>>;

    /// Retrieve blob data, returning an error if not found.
    ///
    /// This is a convenience method that calls `get_blob_opt` and converts
    /// `None` to a `BlobNotFound` error.
    ///
    /// # Arguments
    /// * `blob_id` - The blob ID to retrieve
    ///
    /// # Returns
    /// The blob data with type information, or an error if not found
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
    /// This is a convenience method that serializes the workflow to JSON
    /// and stores it using `put_blob` with `BlobType::Flow`.
    ///
    /// # Arguments
    /// * `workflow` - The workflow to store
    ///
    /// # Returns
    /// The blob ID of the stored workflow
    fn store_flow(
        &self,
        workflow: Arc<Flow>,
    ) -> BoxFuture<'_, error_stack::Result<BlobId, StateError>> {
        async move {
            let flow_data = ValueRef::new(
                serde_json::to_value(workflow.as_ref())
                    .change_context(StateError::Serialization)?,
            );
            self.put_blob(flow_data, BlobType::Flow).await
        }
        .boxed()
    }

    /// Retrieve a workflow by its blob ID.
    ///
    /// This is a convenience method that retrieves the blob using `get_blob_of_type`
    /// and deserializes it as a Flow.
    ///
    /// # Arguments
    /// * `flow_id` - The blob ID of the workflow
    ///
    /// # Returns
    /// The workflow if found, or None if not found or not a Flow type
    fn get_flow(
        &self,
        flow_id: &BlobId,
    ) -> BoxFuture<'_, error_stack::Result<Option<Arc<Flow>>, StateError>> {
        let flow_id = flow_id.clone();
        async move {
            match self.get_blob_of_type(&flow_id, BlobType::Flow).await? {
                Some(blob_data) => {
                    let flow: Flow = serde_json::from_value(blob_data.data().as_ref().clone())
                        .change_context(StateError::Serialization)?;
                    Ok(Some(Arc::new(flow)))
                }
                None => Ok(None),
            }
        }
        .boxed()
    }

    /// Retrieve blob data only if it matches the expected type.
    ///
    /// # Arguments
    /// * `blob_id` - The blob ID to retrieve
    /// * `expected_type` - The expected blob type
    ///
    /// # Returns
    /// The blob data if found and type matches, None if not found or type mismatch, or error
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

    /// Set the filename metadata for an existing blob.
    ///
    /// The filename is not part of the content hash — it is purely metadata
    /// for download convenience (e.g., `Content-Disposition` headers).
    ///
    /// Default implementation is a no-op (returns Ok).
    fn set_blob_filename(
        &self,
        _blob_id: &BlobId,
        _filename: String,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        async { Ok(()) }.boxed()
    }

    /// Store raw binary data as a blob and return its content-based ID.
    ///
    /// The data is base64-encoded for storage via `put_blob` with `BlobType::Binary`.
    /// The blob ID is the SHA-256 hash of the raw bytes (not the base64 encoding).
    fn put_blob_binary(
        &self,
        data: &[u8],
    ) -> BoxFuture<'_, error_stack::Result<BlobId, StateError>> {
        let base64_str = BASE64_STANDARD.encode(data);
        let value_ref = ValueRef::new(serde_json::Value::String(base64_str));
        self.put_blob(value_ref, BlobType::Binary)
    }

    /// Retrieve raw binary data by blob ID.
    ///
    /// Returns the decoded binary data if the blob exists and is of type Binary.
    fn get_blob_binary(
        &self,
        blob_id: &BlobId,
    ) -> BoxFuture<'_, error_stack::Result<Vec<u8>, StateError>> {
        let blob_id = blob_id.clone();
        async move {
            let blob_data = self.get_blob(&blob_id).await?;
            match blob_data.as_binary() {
                Some(bytes) => Ok(bytes.to_vec()),
                None => {
                    // Blob exists but is not binary type
                    Err(error_stack::report!(StateError::Internal)).attach_printable(format!(
                        "Blob '{}' is not a binary blob (type: {:?})",
                        blob_id,
                        blob_data.blob_type()
                    ))
                }
            }
        }
        .boxed()
    }
}
