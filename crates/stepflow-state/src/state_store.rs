use crate::Result;
use std::{future::Future, pin::Pin};
use stepflow_core::{blob::BlobId, workflow::ValueRef};

/// Trait for storing and retrieving state data including blobs.
///
/// This trait provides the foundation for both blob storage and future
/// execution state persistence (journaling, checkpointing, etc.).
pub trait StateStore: Send + Sync {
    /// Store JSON data as a blob and return its content-based ID.
    ///
    /// The blob ID is generated as a SHA-256 hash of the JSON content,
    /// providing deterministic IDs and automatic deduplication.
    ///
    /// # Arguments
    /// * `data` - The JSON data to store as a blob
    ///
    /// # Returns
    /// The blob ID for the stored data
    fn put_blob(&self, data: ValueRef)
    -> Pin<Box<dyn Future<Output = Result<BlobId>> + Send + '_>>;

    /// Retrieve JSON data by blob ID.
    ///
    /// # Arguments
    /// * `blob_id` - The blob ID to retrieve
    ///
    /// # Returns
    /// The JSON data associated with the blob ID, or an error if not found
    fn get_blob(
        &self,
        blob_id: &BlobId,
    ) -> Pin<Box<dyn Future<Output = Result<ValueRef>> + Send + '_>>;
}
