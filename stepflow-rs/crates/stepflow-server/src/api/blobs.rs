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

use axum::{
    extract::{Path, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_core::{BlobId, BlobType, workflow::ValueRef};
use stepflow_plugin::StepflowEnvironment;
use stepflow_state::BlobStoreExt as _;
use utoipa::ToSchema;

use crate::error::ErrorResponse;

/// Request to store a blob
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StoreBlobRequest {
    /// The JSON data to store
    pub data: ValueRef,
    /// The type of blob (data or flow). Defaults to "data".
    #[serde(default)]
    pub blob_type: BlobType,
}

/// Response when a blob is stored
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StoreBlobResponse {
    /// The content-based blob ID (SHA-256 hash)
    pub blob_id: BlobId,
}

/// Response when retrieving a blob
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetBlobResponse {
    /// The blob data
    pub data: ValueRef,
    /// The blob type
    pub blob_type: BlobType,
    /// The blob ID (for confirmation)
    pub blob_id: BlobId,
}

/// Store a blob and return its content-based ID
#[utoipa::path(
    post,
    path = "/blobs",
    request_body = StoreBlobRequest,
    responses(
        (status = 200, description = "Blob stored successfully", body = StoreBlobResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = crate::api::BLOB_TAG,
)]
pub async fn store_blob(
    State(env): State<Arc<StepflowEnvironment>>,
    Json(req): Json<StoreBlobRequest>,
) -> Result<Json<StoreBlobResponse>, ErrorResponse> {
    let blob_store = env.blob_store();
    let blob_id = blob_store
        .put_blob(req.data, req.blob_type)
        .await
        .map_err(|_| ErrorResponse {
            code: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            message: "Failed to store blob".to_string(),
            stack: vec![],
        })?;

    Ok(Json(StoreBlobResponse { blob_id }))
}

/// Get a blob by its ID
#[utoipa::path(
    get,
    path = "/blobs/{blob_id}",
    params(
        ("blob_id" = String, Path, description = "Blob ID to retrieve")
    ),
    responses(
        (status = 200, description = "Blob retrieved successfully", body = GetBlobResponse),
        (status = 404, description = "Blob not found"),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = crate::api::BLOB_TAG,
)]
pub async fn get_blob(
    State(env): State<Arc<StepflowEnvironment>>,
    Path(blob_id): Path<BlobId>,
) -> Result<Json<GetBlobResponse>, ErrorResponse> {
    let blob_store = env.blob_store();

    let blob_data = blob_store
        .get_blob_opt(&blob_id)
        .await
        .map_err(|_| ErrorResponse {
            code: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            message: "Failed to get blob".to_string(),
            stack: vec![],
        })?
        .ok_or_else(|| ErrorResponse {
            code: axum::http::StatusCode::NOT_FOUND,
            message: format!("Blob '{}' not found", blob_id),
            stack: vec![],
        })?;

    Ok(Json(GetBlobResponse {
        data: blob_data.data(),
        blob_type: blob_data.blob_type(),
        blob_id,
    }))
}
