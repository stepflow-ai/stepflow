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
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse as _, Json, Response},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_core::{BlobId, BlobMetadata, BlobType};
use stepflow_plugin::StepflowEnvironment;
use stepflow_state::BlobStoreExt as _;
use utoipa::ToSchema;

use crate::error::ErrorResponse;

/// Request to store a blob (JSON mode)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StoreBlobRequest {
    /// The JSON data to store
    pub data: serde_json::Value,
    /// The type of blob (data or flow). Defaults to "data".
    #[serde(default)]
    pub blob_type: BlobType,
    /// Optional filename to associate with the blob.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

/// Response when a blob is stored
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StoreBlobResponse {
    /// The content-based blob ID (SHA-256 hash)
    pub blob_id: BlobId,
    /// The filename if one was provided
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

/// Response when retrieving a blob (JSON mode)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetBlobResponse {
    /// The blob data
    pub data: serde_json::Value,
    /// The blob type
    pub blob_type: BlobType,
    /// The blob ID (for confirmation)
    pub blob_id: BlobId,
    /// The filename if one was set
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

/// Store a blob and return its content-based ID.
///
/// Supports two content types:
/// - `application/json`: JSON body with `data`, `blobType`, and optional `filename` fields.
/// - `application/octet-stream`: Raw binary body. Use `X-Blob-Filename` header for filename.
#[utoipa::path(
    post,
    path = "/blobs",
    request_body(content(
        (StoreBlobRequest = "application/json"),
        (Vec<u8> = "application/octet-stream"),
    )),
    params(
        ("X-Blob-Filename" = Option<String>, Header, description = "Filename to associate with a binary blob (octet-stream uploads only)")
    ),
    responses(
        (status = 200, description = "Blob stored successfully", body = StoreBlobResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = crate::api::BLOB_TAG,
)]
pub async fn store_blob(
    State(env): State<Arc<StepflowEnvironment>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ErrorResponse> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json");

    let blob_store = env.blob_store();

    if content_type.starts_with("application/octet-stream") {
        // Binary upload: raw bytes in body
        let filename = headers
            .get("x-blob-filename")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let metadata = BlobMetadata {
            filename: filename.clone(),
        };

        let blob_id = blob_store
            .put_blob(&body, BlobType::Binary, metadata)
            .await
            .map_err(|_| ErrorResponse {
                code: StatusCode::INTERNAL_SERVER_ERROR,
                message: "Failed to store binary blob".to_string(),
                stack: vec![],
            })?;

        Ok(Json(StoreBlobResponse { blob_id, filename }).into_response())
    } else {
        // JSON upload
        let req: StoreBlobRequest = serde_json::from_slice(&body).map_err(|e| ErrorResponse {
            code: StatusCode::BAD_REQUEST,
            message: format!("Invalid JSON request: {e}"),
            stack: vec![],
        })?;

        let filename = req.filename.clone();

        let metadata = BlobMetadata {
            filename: filename.clone(),
        };

        let content = serde_json::to_vec(&req.data).map_err(|_| ErrorResponse {
            code: StatusCode::INTERNAL_SERVER_ERROR,
            message: "Failed to serialize blob data".to_string(),
            stack: vec![],
        })?;

        let blob_id = blob_store
            .put_blob(&content, req.blob_type, metadata)
            .await
            .map_err(|_| ErrorResponse {
                code: StatusCode::INTERNAL_SERVER_ERROR,
                message: "Failed to store blob".to_string(),
                stack: vec![],
            })?;

        Ok(Json(StoreBlobResponse { blob_id, filename }).into_response())
    }
}

/// Get a blob by its ID.
///
/// Content negotiation via `Accept` header:
/// - `application/json` (default): Returns JSON with `data`, `blobType`, `blobId`, `filename`.
/// - `application/octet-stream`: Returns raw bytes. For binary blobs, returns decoded bytes.
///   For data/flow blobs, returns UTF-8 JSON bytes. Sets `Content-Disposition` if filename exists.
#[utoipa::path(
    get,
    path = "/blobs/{blob_id}",
    params(
        ("blob_id" = String, Path, description = "Blob ID to retrieve")
    ),
    responses(
        (status = 200, description = "Blob retrieved successfully", content(
            (GetBlobResponse = "application/json"),
            (Vec<u8> = "application/octet-stream"),
        )),
        (status = 404, description = "Blob not found"),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = crate::api::BLOB_TAG,
)]
pub async fn get_blob(
    State(env): State<Arc<StepflowEnvironment>>,
    Path(blob_id): Path<BlobId>,
    headers: HeaderMap,
) -> Result<Response, ErrorResponse> {
    let blob_store = env.blob_store();

    let raw = blob_store
        .get_blob(&blob_id)
        .await
        .map_err(|_| ErrorResponse {
            code: StatusCode::INTERNAL_SERVER_ERROR,
            message: "Failed to get blob".to_string(),
            stack: vec![],
        })?
        .ok_or_else(|| ErrorResponse {
            code: StatusCode::NOT_FOUND,
            message: format!("Blob '{}' not found", blob_id),
            stack: vec![],
        })?;

    let accept = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json");

    if accept.contains("application/octet-stream") {
        // Return raw bytes directly
        let mut response_headers = HeaderMap::new();
        response_headers.insert(
            header::CONTENT_TYPE,
            "application/octet-stream".parse().unwrap(),
        );
        response_headers.insert(
            "x-blob-type",
            format!("{:?}", raw.blob_type)
                .to_lowercase()
                .parse()
                .unwrap(),
        );

        if let Some(ref filename) = raw.metadata.filename {
            // Sanitize filename for Content-Disposition header:
            // - strip CR/LF to prevent header injection
            // - escape double quotes
            let safe_filename = filename
                .replace(&['\r', '\n'][..], "_")
                .replace('"', "\\\"");
            if let Ok(value) = format!("attachment; filename=\"{safe_filename}\"").parse() {
                response_headers.insert(header::CONTENT_DISPOSITION, value);
            }
        }

        Ok((StatusCode::OK, response_headers, raw.content).into_response())
    } else {
        // Return JSON — deserialize content bytes back to Value
        let data: serde_json::Value =
            serde_json::from_slice(&raw.content).map_err(|_| ErrorResponse {
                code: StatusCode::INTERNAL_SERVER_ERROR,
                message: "Failed to deserialize blob data".to_string(),
                stack: vec![],
            })?;

        Ok(Json(GetBlobResponse {
            data,
            blob_type: raw.blob_type,
            blob_id,
            filename: raw.metadata.filename,
        })
        .into_response())
    }
}
