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

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use stepflow_core::workflow::ValueRef;
use stepflow_core::{BlobId, BlobType};

use crate::protocol::Method;

use super::ProtocolMethod;

/// Sent from the component server to the Stepflow to retrieve the content of a specific blob.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetBlobParams {
    /// The ID of the blob to retrieve.
    pub blob_id: BlobId,
}

/// Sent from the Stepflow back to the component server with the blob data and metadata.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetBlobResult {
    pub data: ValueRef,
    pub blob_type: BlobType,
}

impl ProtocolMethod for GetBlobParams {
    const METHOD_NAME: Method = Method::BlobsGet;
    type Response = GetBlobResult;
}

/// Sent from the component server to the Stepflow to store a blob with the provided content.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PutBlobParams {
    pub data: ValueRef,
    pub blob_type: BlobType,
}

/// Sent from the Stepflow back to the component server with the ID of the stored blob.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PutBlobResult {
    pub blob_id: BlobId,
}

impl ProtocolMethod for PutBlobParams {
    const METHOD_NAME: Method = Method::BlobsPut;
    type Response = PutBlobResult;
}
