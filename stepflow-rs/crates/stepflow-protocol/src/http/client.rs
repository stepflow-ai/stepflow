// Licensed to the Apache Software Foundation (ASF) under one or more contributor license agreements.
// See the NOTICE file distributed with this work for additional information regarding copyright
// ownership.  The ASF licenses this file to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance with the License.  You may obtain a
// copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied.  See the License for the specific language governing permissions and limitations under
// the License.

//! HTTP client implementation for StepFlow protocol.
//!
//! This module implements the Streamable HTTP transport where:
//! - Send JSON-RPC requests to the main endpoint
//! - Receive either direct JSON responses or SSE streams
//! - Handle bidirectional communication through SSE when needed

use std::sync::Arc;

use error_stack::ResultExt as _;
use futures::stream::StreamExt as _;
use serde_json::Value as JsonValue;
use stepflow_plugin::Context;
use tokio::sync::mpsc;

use crate::error::{Result, TransportError};
use crate::{Message, MessageHandlerRegistry, OwnedJson, RequestId};

/// HTTP client that communicates with a remote component server using streamable HTTP
pub struct HttpClient {
    handle: HttpClientHandle,
}

impl HttpClient {
    /// Create a new HTTP client for streamable HTTP transport
    pub async fn try_new(url: String, context: Arc<dyn Context>) -> Result<Self> {
        let client = reqwest::Client::new();

        // Pre-build static headers to avoid recreating them for each request
        let mut request_headers = reqwest::header::HeaderMap::new();
        request_headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        request_headers.insert(
            reqwest::header::ACCEPT,
            "application/json, text/event-stream".parse().unwrap(),
        );

        let handle = HttpClientHandle {
            client,
            url,
            context,
            request_headers,
        };

        Ok(Self { handle })
    }

    pub fn handle(&self) -> HttpClientHandle {
        self.handle.clone()
    }
}

#[derive(Clone)]
pub struct HttpClientHandle {
    client: reqwest::Client,
    url: String,
    context: Arc<dyn Context>,
    request_headers: reqwest::header::HeaderMap,
}

impl HttpClientHandle {
    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// Send a JSON-RPC request and wait for a response
    /// Handles both direct JSON responses and SSE streams automatically
    pub async fn send_request(&self, message: &str) -> Result<OwnedJson> {
        // Parse the request to extract the ID for response tracking
        let request_value: JsonValue = serde_json::from_str(message)
            .change_context(TransportError::Send)
            .attach_printable("Failed to parse JSON-RPC request")?;

        let request_id = if let Some(id_value) = request_value.get("id") {
            if let Some(s) = id_value.as_str() {
                RequestId::from(s.to_string())
            } else if let Some(i) = id_value.as_i64() {
                RequestId::from(i)
            } else {
                return Err(TransportError::Send)
                    .attach_printable("JSON-RPC request 'id' field must be string or integer");
            }
        } else {
            return Err(TransportError::Send)
                .attach_printable("JSON-RPC request missing 'id' field");
        };

        // Send the HTTP POST request using pre-built headers
        let response = self
            .client
            .post(&self.url)
            .headers(self.request_headers.clone())
            .body(message.to_string())
            .send()
            .await
            .change_context(TransportError::Send)
            .attach_printable("Failed to send HTTP request")?;

        let status = response.status();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|ct| ct.to_str().ok())
            .unwrap_or("");

        if status.is_client_error() || status.is_server_error() {
            // Handle error response
            let error_text = response
                .text()
                .await
                .change_context(TransportError::Recv)
                .attach_printable("Failed to read error response")?;

            let owned_json = OwnedJson::try_new(error_text)
                .change_context(TransportError::Recv)
                .attach_printable("Failed to parse error response as JSON")?;

            return Ok(owned_json);
        }

        if content_type.contains("text/event-stream") {
            // Handle SSE streaming response - this means we need bidirectional communication
            tracing::debug!("Received SSE streaming response for request {}", request_id);

            // Process the SSE stream and handle bidirectional communication
            self.handle_sse_stream(response, request_id).await
        } else {
            // Handle direct JSON response
            let response_text = response
                .text()
                .await
                .change_context(TransportError::Recv)
                .attach_printable("Failed to read response body")?;

            tracing::debug!("Received direct JSON response: {}", response_text);

            let owned_json = OwnedJson::try_new(response_text)
                .change_context(TransportError::Recv)
                .attach_printable("Failed to parse response as JSON")?;

            Ok(owned_json)
        }
    }

    /// Handle SSE stream and bidirectional communication
    async fn handle_sse_stream(
        &self,
        response: reqwest::Response,
        expected_response_id: RequestId,
    ) -> Result<OwnedJson> {
        // Get the response text as a stream
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut final_response: Option<OwnedJson> = None;

        // Process SSE stream chunks
        while let Some(chunk) = stream.next().await {
            let chunk = chunk
                .change_context(TransportError::Recv)
                .attach_printable("Failed to read chunk from SSE stream")?;

            let chunk_str = std::str::from_utf8(&chunk)
                .change_context(TransportError::Recv)
                .attach_printable("Invalid UTF-8 in SSE stream")?;

            buffer.push_str(chunk_str);

            // Process complete SSE messages
            while let Some(message_end) = buffer.find("\n\n") {
                let message = buffer[..message_end].to_string();
                buffer = buffer[message_end + 2..].to_string();

                // Parse SSE message
                if let Some(data) = self.parse_sse_message(&message) {
                    // Parse the JSON-RPC message
                    let owned_json = match OwnedJson::try_new(data) {
                        Ok(json) => json,
                        Err(e) => {
                            tracing::error!("Failed to parse SSE message as JSON: {:?}", e);
                            continue;
                        }
                    };

                    match owned_json.message() {
                        Message::Request(request) => {
                            // Handle bidirectional request from server
                            tracing::info!(
                                "Received bidirectional request for method '{}' with id {}",
                                request.method,
                                request.id
                            );

                            if let Err(e) = self.handle_bidirectional_request(owned_json).await {
                                tracing::error!("Failed to handle bidirectional request: {:?}", e);
                            }
                        }
                        Message::Response(response) => {
                            // Check if this is the final response we're waiting for
                            if response.id() == &expected_response_id {
                                tracing::info!(
                                    "Received final response for request {}",
                                    expected_response_id
                                );
                                final_response = Some(owned_json);
                                break; // We got our response, exit the loop
                            } else {
                                tracing::debug!(
                                    "Received response for different request: {}",
                                    response.id()
                                );
                            }
                        }
                        Message::Notification(notification) => {
                            tracing::debug!(
                                "Received notification for method '{}'",
                                notification.method
                            );
                            // Notifications are fire-and-forget, no response needed
                        }
                    }
                }
            }

            // If we got our final response, break out of the stream processing
            if final_response.is_some() {
                break;
            }
        }

        // Return the final response
        final_response.ok_or_else(|| {
            error_stack::Report::new(TransportError::Recv)
                .attach_printable("SSE stream ended without receiving final response")
        })
    }

    /// Parse an SSE message to extract the data field
    fn parse_sse_message(&self, message: &str) -> Option<String> {
        for line in message.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                return Some(data.to_string());
            }
        }
        None
    }

    /// Handle a bidirectional request from the server during SSE streaming
    async fn handle_bidirectional_request(&self, request_json: OwnedJson) -> Result<()> {
        // Extract the request data before moving into the async block
        let (request_method, request_id) = match request_json.message() {
            Message::Request(request) => (request.method, request.id.clone()),
            _ => {
                return Err(
                    error_stack::Report::new(TransportError::Recv).attach_printable(
                        "Expected a request message for bidirectional communication",
                    ),
                );
            }
        };

        let Some(handler) = MessageHandlerRegistry::instance().get_method_handler(request_method)
        else {
            tracing::warn!(
                "No handler found for bidirectional method '{}'",
                request_method
            );

            // Send error response back to server
            let error_response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "error": {
                    "code": -32601,
                    "message": format!("Method not found: {}", request_method)
                }
            });

            if let Err(e) = self
                .send_response_to_server(&error_response.to_string())
                .await
            {
                tracing::error!(
                    "Failed to send error response for bidirectional request: {:?}",
                    e
                );
            }
            return Ok(());
        };

        // Handle the request asynchronously and send response back to server
        let context = self.context.clone();
        let client_handle = self.clone();

        let future = async move {
            let (outgoing_tx, mut outgoing_rx) = mpsc::channel(100);

            // Extract the request again in the async block
            let request = match request_json.message() {
                Message::Request(req) => req,
                _ => {
                    tracing::error!("Invalid message type in bidirectional request handler");
                    return;
                }
            };

            // Handle the request
            if let Err(e) = handler
                .handle_message(request, outgoing_tx, context.clone())
                .await
            {
                tracing::error!(
                    "Failed executing handler for method '{}': {:?}",
                    request.method,
                    e
                );

                // Send error response back to server
                let error_response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": request.id,
                    "error": {
                        "code": -32601,
                        "message": format!("Failed to handle method {}: {:?}", request.method, e)
                    }
                });

                if let Err(e) = client_handle
                    .send_response_to_server(&error_response.to_string())
                    .await
                {
                    tracing::error!(
                        "Failed to send error response for bidirectional request: {:?}",
                        e
                    );
                }
                return;
            };

            // Process any outgoing messages that were generated
            // These are additional requests from the handler (like blob operations)
            while let Ok(outgoing_message) = outgoing_rx.try_recv() {
                if let Err(e) = client_handle
                    .send_response_to_server(&outgoing_message)
                    .await
                {
                    tracing::error!("Failed to send outgoing message: {:?}", e);
                }
            }
        };

        tokio::spawn(future);
        Ok(())
    }

    /// Send a response back to the server (used for bidirectional communication and notifications)
    pub async fn send_response_to_server(&self, message: &str) -> Result<()> {
        tracing::info!("Sending response to server: {}", message);
        let response = self
            .client
            .post(&self.url)
            .headers(self.request_headers.clone())
            .body(message.to_string())
            .send()
            .await
            .change_context(TransportError::Send)
            .attach_printable("Failed to send response to server")?;

        // Server should return 202 Accepted for responses
        let status = response.status();
        if status != reqwest::StatusCode::ACCEPTED {
            let response_text = response.text().await;
            tracing::warn!("Unexpected status for response: {status} {response_text:?}",);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::{BoxFuture, FutureExt as _};
    use std::path::Path;
    use std::sync::Arc;
    use stepflow_core::{
        FlowResult,
        workflow::{Flow, FlowHash, ValueRef},
    };
    use stepflow_plugin::{Context, Result as PluginResult};
    use stepflow_state::{InMemoryStateStore, StateStore};
    use uuid::Uuid;

    // Mock context for testing
    struct MockContext {
        state_store: Arc<dyn StateStore>,
    }

    impl MockContext {
        fn new() -> Self {
            Self {
                state_store: Arc::new(InMemoryStateStore::new()),
            }
        }
    }

    impl Context for MockContext {
        fn submit_flow(
            &self,
            _flow: Arc<Flow>,
            _workflow_hash: FlowHash,
            _input: ValueRef,
        ) -> BoxFuture<'_, PluginResult<Uuid>> {
            async { Ok(Uuid::new_v4()) }.boxed()
        }

        fn flow_result(&self, _run_id: Uuid) -> BoxFuture<'_, PluginResult<FlowResult>> {
            async { Ok(FlowResult::Success(ValueRef::default())) }.boxed()
        }

        fn state_store(&self) -> &Arc<dyn StateStore> {
            &self.state_store
        }

        fn working_directory(&self) -> &Path {
            Path::new("/tmp")
        }
    }

    #[tokio::test]
    async fn test_http_client_creation() {
        let context = Arc::new(MockContext::new()) as Arc<dyn Context>;
        let url = "http://localhost:8080".to_string();

        // Test that we can create a client without errors
        let result = HttpClient::try_new(url, context).await;
        assert!(result.is_ok());

        let client = result.unwrap();
        let handle = client.handle();

        // Test that handle provides access to URL
        assert_eq!(handle.url(), "http://localhost:8080");
    }

    #[test]
    fn test_request_id_parsing() {
        // Test string ID
        let request_with_string_id = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "test",
            "id": "test-string-id"
        });

        if let Some(id_value) = request_with_string_id.get("id") {
            if let Some(s) = id_value.as_str() {
                let request_id = RequestId::from(s.to_string());
                assert_eq!(format!("{request_id}"), "test-string-id");
            }
        }

        // Test integer ID
        let request_with_int_id = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "test",
            "id": 42
        });

        if let Some(id_value) = request_with_int_id.get("id") {
            if let Some(i) = id_value.as_i64() {
                let request_id = RequestId::from(i);
                assert_eq!(format!("{request_id}"), "42");
            }
        }
    }
}
