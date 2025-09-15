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

use crate::{Result, error::MainError, validation_display::display_diagnostics};
use error_stack::ResultExt as _;
use serde::Deserialize;
use std::sync::Arc;
use stepflow_core::FlowResult;
use stepflow_core::workflow::{Flow, ValueRef};
use stepflow_server::{CreateRunRequest, CreateRunResponse, StoreFlowRequest, StoreFlowResponse};
use url::Url;

/// Error response structure for client-side deserialization
#[derive(Debug, Deserialize)]
struct ErrorResponse {
    pub message: String,
    #[serde(default)]
    pub stack: Vec<ErrorStackEntry>,
}

/// A single entry in the error stack
#[derive(Debug, Deserialize)]
struct ErrorStackEntry {
    pub error: String,
    #[serde(default)]
    pub attachments: Vec<String>,
    pub backtrace: Option<String>,
}

/// Display a server error response with enhanced stack information
fn display_server_error(status: reqwest::StatusCode, response_text: &str, context: &str) {
    // Try to parse as structured error response first
    if let Ok(error_response) = serde_json::from_str::<ErrorResponse>(response_text) {
        tracing::error!("Server returned error {}: {}", context, error_response.message);

        if !error_response.stack.is_empty() {
            tracing::error!("Error stack trace:");
            for (i, entry) in error_response.stack.iter().enumerate() {
                tracing::error!("  {}. {}", i + 1, entry.error);

                // Display attachments if present
                if !entry.attachments.is_empty() {
                    for attachment in &entry.attachments {
                        tracing::error!("     • {}", attachment);
                    }
                }

                // Display backtrace if present (truncated for readability)
                if let Some(backtrace) = &entry.backtrace {
                    let lines: Vec<_> = backtrace.lines().collect();
                    if !lines.is_empty() {
                        tracing::error!("     Backtrace ({} frames):", lines.len());
                        // Show first few frames for context
                        lines.iter().take(5).for_each(|line| {
                            tracing::error!("       {}", line);
                        });
                        if lines.len() > 5 {
                            tracing::error!("       ... ({} more frames)", lines.len() - 5);
                            tracing::error!("       Set RUST_LOG=debug for full backtrace");
                        }
                    }
                }
            }
        }
    } else {
        // Fallback to simple error display if parsing fails
        tracing::error!("Server returned error {} {}: {}", context, status, response_text);
    }
}

/// Submit a workflow to a Stepflow service for execution
pub async fn submit(service_url: Url, flow: Flow, input: ValueRef) -> Result<FlowResult> {
    let client = reqwest::Client::new();

    // Step 1: Store the flow to get its hash
    let store_request = StoreFlowRequest {
        flow: Arc::new(flow),
    };

    let store_url = service_url
        .join("/api/v1/flows")
        .map_err(|_| MainError::Configuration)?;

    let store_response = client
        .post(store_url)
        .json(&store_request)
        .send()
        .await
        .map_err(|e| {
            tracing::error!("Failed to store workflow: {}", e);
            MainError::Configuration
        })?;

    if !store_response.status().is_success() {
        let status = store_response.status();
        let error_text = store_response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        display_server_error(status, &error_text, "storing workflow");
        return Err(MainError::Configuration.into());
    };

    let store_result: StoreFlowResponse = store_response
        .json()
        .await
        .change_context(MainError::ServerError)?;

    // Display validation results
    let failure_count = display_diagnostics(&store_result.analysis_result.diagnostics);

    // Check if the workflow was stored successfully
    let flow_id = store_result.flow_id.ok_or_else(|| {
        // If validation failed, the error details were already shown by display_diagnostics
        if failure_count > 0 {
            tracing::error!("Workflow validation failed - see diagnostics above");
        } else {
            tracing::error!("Workflow was not stored for unknown reasons");
        }
        MainError::ValidationError("Workflow validation failed".to_string())
    })?;

    // Step 2: Execute the workflow by hash
    let execute_request = CreateRunRequest {
        flow_id,
        input,
        debug: false, // TODO: Add debug option to CLI
    };

    let execute_url = service_url
        .join("/api/v1/runs")
        .map_err(|_| MainError::Configuration)?;

    let execute_response = client
        .post(execute_url)
        .json(&execute_request)
        .send()
        .await
        .map_err(|e| {
            tracing::error!("Failed to execute workflow: {}", e);
            MainError::Configuration
        })?;

    if !execute_response.status().is_success() {
        let status = execute_response.status();
        let error_text = execute_response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        display_server_error(status, &error_text, "executing workflow");
        return Err(MainError::Configuration.into());
    }

    let execute_result: CreateRunResponse = execute_response.json().await.map_err(|e| {
        tracing::error!("Failed to parse execute response: {}", e);
        MainError::Configuration
    })?;

    // Return the result if available
    match execute_result.result {
        Some(result) => Ok(result),
        None => {
            tracing::error!("No result in response");
            Err(MainError::Configuration.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_server_error_structured() {
        // Test parsing and display of structured error response
        let error_json = r#"{
            "code": 404,
            "message": "Execution not found",
            "stack": [
                {
                    "error": "Execution '12345' not found",
                    "attachments": ["Test execution not found", "Looking for execution: 12345"]
                },
                {
                    "error": "State store unavailable",
                    "attachments": ["Database connection failed"]
                }
            ]
        }"#;

        // This should not panic and should display structured error
        display_server_error(reqwest::StatusCode::NOT_FOUND, error_json, "test operation");
    }

    #[test]
    fn test_display_server_error_with_backtrace() {
        // Test parsing and display of error response with backtrace
        let error_json = r#"{
            "code": 500,
            "message": "Internal server error",
            "stack": [
                {
                    "error": "Database connection failed",
                    "attachments": [
                        "Connection timeout after 30s",
                        "Retry attempts: 3",
                        "Last error: Connection refused"
                    ],
                    "backtrace": "   0: std::backtrace_rs::backtrace::libunwind::trace\n             at /rustc/.../backtrace/src/backtrace/libunwind.rs:117:9\n   1: std::backtrace::Backtrace::create\n             at /rustc/.../library/std/src/backtrace.rs:331:13\n   2: stepflow_server::api::health::health_check\n             at ./crates/stepflow-server/src/api/health.rs:77:33\n   3: <F as axum::handler::Handler<(M,T1),S>>::call\n             at /Users/.../axum-0.8.4/src/handler/mod.rs:239:43\n   4: tower::util::oneshot::Oneshot<S,Req>\n             at /Users/.../tower-0.5.2/src/util/oneshot.rs:97:42\n   5: hyper::proto::h1::dispatch::Dispatcher<D,Bs,I,T>::poll_write\n             at /Users/.../hyper-1.7.0/src/proto/h1/dispatch.rs:336:72"
                }
            ]
        }"#;

        // Test that this doesn't panic - actual output verification would require
        // tracing subscriber setup which is complex for unit tests
        display_server_error(reqwest::StatusCode::INTERNAL_SERVER_ERROR, error_json, "executing workflow");
    }

    #[test]
    fn test_display_server_error_fallback() {
        // Test fallback for non-JSON error response
        let error_text = "Internal Server Error";

        // This should fallback to simple error display
        display_server_error(reqwest::StatusCode::INTERNAL_SERVER_ERROR, error_text, "test operation");
    }

    #[test]
    fn test_error_response_parsing() {
        let error_json = r#"{
            "code": 500,
            "message": "Internal error",
            "stack": [
                {
                    "error": "Database error",
                    "attachments": ["Connection timeout"],
                    "backtrace": "   0: test::function\n   1: another::function\n"
                }
            ]
        }"#;

        let parsed: ErrorResponse = serde_json::from_str(error_json).expect("Should parse");
        assert_eq!(parsed.message, "Internal error");
        assert_eq!(parsed.stack.len(), 1);
        assert_eq!(parsed.stack[0].error, "Database error");
        assert_eq!(parsed.stack[0].attachments.len(), 1);
        assert!(parsed.stack[0].backtrace.is_some());
    }
}
