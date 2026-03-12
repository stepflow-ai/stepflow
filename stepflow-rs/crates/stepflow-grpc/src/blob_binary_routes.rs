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

//! Hand-written binary blob REST routes.
//!
//! tonic-rest does not support `google.api.HttpBody` or raw byte passthrough,
//! so binary blob upload/download is handled by these dedicated Axum routes.
//!
//! These routes handle:
//! - `POST /blobs` with `Content-Type: application/octet-stream`
//! - `GET /blobs/{blob_id}` with `Accept: application/octet-stream`
//!
//! For JSON requests, the tonic-rest generated routes handle the request.
//! Mount these routes *before* the tonic-rest router so they take priority
//! for binary content negotiation.

use std::sync::Arc;

use axum::{
    Router,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse as _, Json, Response},
    routing::{get, post},
};
use serde::Serialize;
use stepflow_core::{BlobId, BlobMetadata, BlobType};
use stepflow_plugin::StepflowEnvironment;
use stepflow_state::BlobStoreExt as _;

/// Response when a blob is stored (JSON envelope).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StoreBlobResponse {
    blob_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
}

/// Response when retrieving a blob as JSON.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GetBlobJsonResponse {
    data: serde_json::Value,
    blob_type: String,
    blob_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_type: Option<String>,
}

/// Create an Axum router with binary-aware blob routes.
///
/// Mount this *before* the tonic-rest generated routes so binary content
/// negotiation takes priority. These handlers support both JSON and binary
/// content types, falling through to the appropriate behavior based on headers.
pub fn blob_binary_routes(env: Arc<StepflowEnvironment>) -> Router {
    Router::new()
        .route("/blobs", post(put_blob_handler))
        .route("/blobs/{blob_id}", get(get_blob_handler))
        .with_state(env)
}

/// Store a blob with content negotiation.
///
/// - `Content-Type: application/octet-stream`: Raw binary upload.
///   Use `X-Blob-Filename` header for filename.
/// - `Content-Type: application/json` (default): JSON body with `json_data`,
///   `blob_type`, and optional `filename` fields (matches proto schema).
async fn put_blob_handler(
    State(env): State<Arc<StepflowEnvironment>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json");

    let blob_store = env.blob_store();

    if content_type.starts_with("application/octet-stream") {
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
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        Ok(Json(StoreBlobResponse {
            blob_id: blob_id.to_string(),
            filename,
        })
        .into_response())
    } else {
        // Parse as the proto-compatible JSON format
        let req: serde_json::Value =
            serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;

        let blob_type = match req.get("blob_type").or_else(|| req.get("blobType")) {
            Some(serde_json::Value::String(s)) => match s.as_str() {
                "flow" => BlobType::Flow,
                "binary" => BlobType::Binary,
                "data" | "" => BlobType::Data,
                _ => return Err(StatusCode::BAD_REQUEST),
            },
            None => BlobType::Data,
            _ => return Err(StatusCode::BAD_REQUEST),
        };

        let json_data = req
            .get("json_data")
            .or_else(|| req.get("jsonData"))
            .or_else(|| req.get("data"))
            .ok_or(StatusCode::BAD_REQUEST)?;

        let filename = req
            .get("filename")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let metadata = BlobMetadata {
            filename: filename.clone(),
        };

        let content =
            serde_json::to_vec(json_data).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let blob_id = blob_store
            .put_blob(&content, blob_type, metadata)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        Ok(Json(StoreBlobResponse {
            blob_id: blob_id.to_string(),
            filename,
        })
        .into_response())
    }
}

/// Get a blob with content negotiation.
///
/// - `Accept: application/octet-stream`: Returns raw bytes with appropriate headers.
/// - `Accept: application/json` (default): Returns JSON envelope with data, type, etc.
async fn get_blob_handler(
    State(env): State<Arc<StepflowEnvironment>>,
    Path(blob_id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let blob_id = BlobId::new(blob_id).map_err(|_| StatusCode::BAD_REQUEST)?;

    let raw = env
        .blob_store()
        .get_blob(&blob_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let accept = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json");

    if accept.contains("application/octet-stream") {
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
            let safe_filename = filename
                .replace(&['\r', '\n'][..], "_")
                .replace('"', "\\\"");
            if let Ok(value) = format!("attachment; filename=\"{safe_filename}\"").parse() {
                response_headers.insert(header::CONTENT_DISPOSITION, value);
            }
        }

        Ok((StatusCode::OK, response_headers, raw.content).into_response())
    } else {
        let blob_type_str = format!("{:?}", raw.blob_type).to_lowercase();
        let content_type = if raw.blob_type == BlobType::Binary {
            raw.metadata
                .filename
                .as_deref()
                .map(|_| "application/octet-stream".to_string())
        } else {
            None
        };

        if raw.blob_type == BlobType::Binary {
            // Binary blobs: return base64 data as JSON isn't useful, return metadata
            Ok(Json(GetBlobJsonResponse {
                data: serde_json::Value::Null,
                blob_type: blob_type_str,
                blob_id: blob_id.to_string(),
                filename: raw.metadata.filename,
                content_type,
            })
            .into_response())
        } else {
            let data: serde_json::Value = serde_json::from_slice(&raw.content)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            Ok(Json(GetBlobJsonResponse {
                data,
                blob_type: blob_type_str,
                blob_id: blob_id.to_string(),
                filename: raw.metadata.filename,
                content_type,
            })
            .into_response())
        }
    }
}
