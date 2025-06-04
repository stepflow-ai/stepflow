use serde::{Deserialize, Serialize};
use stepflow_core::{blob::BlobId, workflow::ValueRef};

use crate::schema::Method;

/// Request to store JSON data as a blob.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    /// The JSON data to store as a blob
    pub data: ValueRef,
}

/// Response from storing a blob.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    /// The blob ID for the stored data
    pub blob_id: BlobId,
}

impl Method for Request {
    const METHOD_NAME: &'static str = "put_blob";
    type Response = Response;
}
