use serde::{Deserialize, Serialize};
use stepflow_core::{blob::BlobId, workflow::ValueRef};

use crate::schema::Method;

/// Request to retrieve JSON data by blob ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    /// The blob ID to retrieve
    pub blob_id: BlobId,
}

/// Response from retrieving a blob.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    /// The JSON data associated with the blob ID
    pub data: ValueRef,
}

impl Method for Request {
    const METHOD_NAME: &'static str = "get_blob";
    type Response = Response;
}
