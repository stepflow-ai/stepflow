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

use axum::{extract::Query, response::Json};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::error::{ErrorResponse, ServerError};

/// Health check query parameters
#[derive(Debug, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct HealthQuery {
    /// Trigger an error for testing error response format
    pub error: Option<String>,
}

/// Health check response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    /// Service status
    pub status: String,
    /// Timestamp when health was checked (RFC3339 format)
    pub timestamp: String,
    /// Service version
    pub version: String,
}

/// Check service health
#[utoipa::path(
    get,
    path = "/health",
    params(HealthQuery),
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse),
        (status = 400, description = "Bad request - triggered by ?error=bad_request"),
        (status = 404, description = "Not found - triggered by ?error=not_found"),
        (status = 500, description = "Internal server error - triggered by ?error=internal or ?error=stack")
    )
)]
pub async fn health_check(Query(params): Query<HealthQuery>) -> Result<Json<HealthResponse>, ErrorResponse> {
    // Test error scenarios
    if let Some(error_type) = params.error.as_ref() {
        return match error_type.as_str() {
            "bad_request" => {
                use error_stack::{report, ResultExt};
                let backtrace = std::backtrace::Backtrace::capture();
                Err(report!(ServerError::ExecutionNotFound(uuid::Uuid::new_v4()))
                    .attach(backtrace)
                    .change_context(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Bad request triggered for testing"))
                    .attach_printable("This is a test error with multiple layers")
                    .attach_printable("Layer 1: User requested error via ?error=bad_request")
                    .attach_printable("Layer 2: Simulating validation failure")
                    .into())
            },
            "not_found" => {
                use error_stack::report;
                let test_id = uuid::Uuid::new_v4();
                Err(report!(ServerError::ExecutionNotFound(test_id))
                    .attach_printable("Test execution not found")
                    .attach_printable(format!("Looking for execution: {}", test_id))
                    .into())
            },
            "stack" => {
                use error_stack::{report, ResultExt};
                // Create a more complex error stack with backtrace
                let backtrace = std::backtrace::Backtrace::capture();
                Err(report!(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Database connection failed"))
                    .attach(backtrace)
                    .change_context(std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "State store unavailable"))
                    .change_context(ServerError::ExecutionNotFound(uuid::Uuid::new_v4()))
                    .attach_printable("Complex error stack for testing")
                    .attach_printable("Bottom layer: Database permission denied")
                    .attach_printable("Middle layer: Connection refused")
                    .attach_printable("Top layer: Execution not found")
                    .into())
            },
            _ => {
                use error_stack::report;
                Err(report!(std::io::Error::new(std::io::ErrorKind::Other, "Generic test error"))
                    .attach_printable("Unknown error type requested")
                    .attach_printable(format!("Requested error type: {}", error_type))
                    .into())
            }
        };
    }

    Ok(Json(HealthResponse {
        status: "healthy".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }))
}
