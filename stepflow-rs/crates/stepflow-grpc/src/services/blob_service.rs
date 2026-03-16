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

use std::sync::Arc;

use stepflow_core::{BlobMetadata, BlobType};
use stepflow_plugin::StepflowEnvironment;
use stepflow_state::BlobStoreExt as _;
use tonic::{Request, Response, Status};

use crate::error as grpc_err;
use crate::proto::stepflow::v1::blob_service_server::BlobService;
use crate::proto::stepflow::v1::{
    GetBlobRequest, GetBlobResponse, PutBlobRequest, PutBlobResponse,
};

/// gRPC implementation of `BlobService`.
#[derive(Debug)]
pub struct BlobServiceImpl {
    env: Arc<StepflowEnvironment>,
}

impl BlobServiceImpl {
    pub fn new(env: Arc<StepflowEnvironment>) -> Self {
        Self { env }
    }
}

#[tonic::async_trait]
impl BlobService for BlobServiceImpl {
    async fn put_blob(
        &self,
        request: Request<PutBlobRequest>,
    ) -> Result<Response<PutBlobResponse>, Status> {
        let req = request.into_inner();

        let blob_type = match req.blob_type.as_str() {
            "flow" => BlobType::Flow,
            "data" | "" => BlobType::Data,
            "binary" => BlobType::Binary,
            other => {
                return Err(grpc_err::invalid_field(
                    "blob_type",
                    format!("invalid blob_type: '{other}'"),
                ));
            }
        };

        let content = match req.content {
            Some(crate::proto::stepflow::v1::put_blob_request::Content::JsonData(data)) => {
                let json = crate::conversions::proto_value_to_json(&data);
                serde_json::to_vec(&json).map_err(|e| {
                    grpc_err::internal(format!("failed to serialize blob data: {e}"))
                })?
            }
            Some(crate::proto::stepflow::v1::put_blob_request::Content::RawData(bytes)) => bytes,
            None => return Err(grpc_err::invalid_field("content", "content is required")),
        };

        let metadata = BlobMetadata {
            filename: req.filename.clone(),
        };

        let blob_id = self
            .env
            .blob_store()
            .put_blob(&content, blob_type, metadata)
            .await
            .map_err(|e| grpc_err::internal(format!("failed to store blob: {e}")))?;

        Ok(Response::new(PutBlobResponse {
            blob_id: blob_id.to_string(),
            filename: req.filename,
        }))
    }

    async fn get_blob(
        &self,
        request: Request<GetBlobRequest>,
    ) -> Result<Response<GetBlobResponse>, Status> {
        let req = request.into_inner();

        let blob_id = stepflow_core::BlobId::new(req.blob_id.clone())
            .map_err(|_| grpc_err::invalid_field("blob_id", "invalid blob_id format"))?;

        let raw = self
            .env
            .blob_store()
            .get_blob(&blob_id)
            .await
            .map_err(|e| grpc_err::internal(format!("failed to get blob: {e}")))?
            .ok_or_else(|| grpc_err::not_found("blob", &req.blob_id))?;

        let (content, content_type) = if raw.blob_type == BlobType::Binary {
            (
                Some(crate::proto::stepflow::v1::get_blob_response::Content::RawData(raw.content)),
                raw.metadata
                    .filename
                    .as_deref()
                    .map(|_| "application/octet-stream".to_string()),
            )
        } else {
            let data: prost_wkt_types::Value = serde_json::from_slice(&raw.content)
                .map_err(|e| grpc_err::internal(format!("failed to deserialize blob data: {e}")))?;
            (
                Some(crate::proto::stepflow::v1::get_blob_response::Content::JsonData(data)),
                None,
            )
        };

        Ok(Response::new(GetBlobResponse {
            content,
            blob_type: format!("{:?}", raw.blob_type).to_lowercase(),
            blob_id: req.blob_id,
            filename: raw.metadata.filename,
            content_type,
        }))
    }
}
