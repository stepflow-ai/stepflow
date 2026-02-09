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
    /// # Arguments
    /// * `blob_id` - The blob ID to retrieve
    ///
    /// # Returns
    /// The blob data with type information, or an error if not found
    fn get_blob(
        &self,
        blob_id: &BlobId,
    ) -> BoxFuture<'_, error_stack::Result<BlobData, StateError>>;

    /// Store a workflow as a blob and return its blob ID.
    ///
    /// # Arguments
    /// * `workflow` - The workflow to store
    ///
    /// # Returns
    /// The blob ID of the stored workflow
    fn store_flow(
        &self,
        workflow: Arc<Flow>,
    ) -> BoxFuture<'_, error_stack::Result<BlobId, StateError>>;

    /// Retrieve a workflow by its blob ID.
    ///
    /// # Arguments
    /// * `flow_id` - The blob ID of the workflow
    ///
    /// # Returns
    /// The workflow if found, or None if not found
    fn get_flow(
        &self,
        flow_id: &BlobId,
    ) -> BoxFuture<'_, error_stack::Result<Option<Arc<Flow>>, StateError>>;

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
            match self.get_blob(&blob_id).await {
                Ok(blob_data) => {
                    if blob_data.blob_type() == expected_type {
                        Ok(Some(blob_data))
                    } else {
                        Ok(None)
                    }
                }
                Err(e) => {
                    // Check if it's a "not found" error - if so, return None instead of error
                    // For now, we'll just propagate all errors
                    Err(e)
                }
            }
        }
        .boxed()
    }
}
