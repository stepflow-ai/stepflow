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

//! HTTP client implementation for Stepflow protocol.
//!
//! This module implements the Streamable HTTP transport where:
//! - Send JSON-RPC requests to the main endpoint
//! - Receive either direct JSON responses or SSE streams
//! - Handle bidirectional communication through SSE when needed

use std::sync::Arc;

use error_stack::ResultExt as _;
use futures::stream::StreamExt as _;
use stepflow_plugin::Context;
use tokio::sync::mpsc;

use super::bidirectional_driver::BidirectionalDriver;
use crate::error::{Result, TransportError};
use crate::protocol::{Method, MethodRequest, Notification, ProtocolMethod, ProtocolNotification};
use crate::{LazyValue, Message, MessageHandlerRegistry, OwnedJson, RequestId};
use serde::de::DeserializeOwned;

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
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Send a typed method request and return the typed response
    /// This follows the same pattern as StdioClientHandle
    pub async fn method<I>(&self, params: &I) -> Result<I::Response>
    where
        I: ProtocolMethod + serde::Serialize + Send + Sync + std::fmt::Debug,
        I::Response: DeserializeOwned + Send + Sync + 'static,
    {
        let response = self
            .method_dyn(I::METHOD_NAME, LazyValue::write_ref(params))
            .await?;

        response
            .value()
            .deserialize_to()
            .change_context(TransportError::InvalidResponse(I::METHOD_NAME))
    }

    async fn method_dyn(
        &self,
        method: Method,
        params: LazyValue<'_>,
    ) -> Result<OwnedJson<LazyValue<'static>>> {
        let id = RequestId::new_uuid();
        let request = MethodRequest {
            jsonrpc: Default::default(),
            id: id.clone(),
            method,
            params: Some(params),
        };

        let request_str = serde_json::to_string(&request)
            .change_context(TransportError::SerializeRequest(method))?;

        let response = self
            .send_method_request(id.clone(), request_str)
            .await?
            .owned_response()?;

        // This is an assertion since the routing should only send the response for the
        // registered ID to the pending one-shot channel.
        debug_assert_eq!(response.response().id(), &id);
        response.into_success_value()
    }

    /// Send a typed notification (fire-and-forget)
    pub async fn notify<I>(&self, params: &I) -> Result<()>
    where
        I: ProtocolNotification + serde::Serialize + Send + Sync + std::fmt::Debug,
    {
        let notification = Notification::new(I::METHOD_NAME, Some(LazyValue::write_ref(params)));
        self.notify_dyn(notification).await
    }

    /// Send a notification.
    async fn notify_dyn(&self, notification: Notification<'_>) -> Result<()> {
        let notification_str = serde_json::to_string(&notification)
            .change_context(TransportError::Send)
            .attach_printable_lazy(|| {
                format!("Failed to serialize {} notification", notification.method)
            })?;

        // For notifications, we send the request but don't wait for a response
        let response = self
            .client
            .post(&self.url)
            .headers(self.request_headers.clone())
            .body(notification_str)
            .send()
            .await
            .change_context(TransportError::Send)?
            .error_for_status()
            .change_context(TransportError::Send)?;

        error_stack::ensure!(
            response.status() == reqwest::StatusCode::ACCEPTED,
            TransportError::Send
        );
        Ok(())
    }

    /// Send a JSON-RPC request and wait for a response
    /// Handles both direct JSON responses and SSE streams automatically
    async fn send_method_request(&self, id: RequestId, message: String) -> Result<OwnedJson> {
        // Send the HTTP POST request using pre-built headers
        let response = self
            .client
            .post(&self.url)
            .headers(self.request_headers.clone())
            .body(message)
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
            tracing::debug!(request_id = %id, "Starting SSE stream processing for bidirectional communication");

            // Create a bidirectional driver to handle the SSE stream
            let driver = BidirectionalDriver::new(self.clone());
            let messages = self.sse_messages(response);
            driver.drive_to_completion(id, messages).await
        } else {
            // Handle direct JSON response
            let response_text = response
                .text()
                .await
                .change_context(TransportError::Recv)
                .attach_printable("Failed to read response body")?;

            tracing::debug!(request_id = %id, response_size = response_text.len(), "Received direct JSON response");

            let owned_json = OwnedJson::try_new(response_text)
                .change_context(TransportError::Recv)
                .attach_printable("Failed to parse response as JSON")?;

            tracing::debug!(?owned_json, "Received JSON response");

            Ok(owned_json)
        }
    }

    /// Create a clean async stream of parsed SSE messages using async_stream
    fn sse_messages(
        &self,
        response: reqwest::Response,
    ) -> impl futures::Stream<Item = OwnedJson> + '_ {
        let mut byte_stream = response.bytes_stream();
        let mut buffer = String::new();

        async_stream::stream! {
            while let Some(chunk_result) = byte_stream.next().await {
                // Handle chunk errors
                let chunk = match chunk_result {
                    Ok(chunk) => chunk,
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to read chunk from SSE stream, skipping");
                        continue;
                    }
                };

                // Convert to string
                let chunk_str = match std::str::from_utf8(&chunk) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!(error = %e, "Invalid UTF-8 in SSE stream, skipping chunk");
                        continue;
                    }
                };

                buffer.push_str(chunk_str);

                // Extract complete SSE messages (delimited by \n\n)
                while let Some(message_end) = buffer.find("\n\n") {
                    let raw_message = buffer[..message_end].to_string();
                    buffer = buffer[message_end + 2..].to_string();

                    // Parse SSE message format and extract JSON data
                    if let Some(json_data) = self.parse_sse_message(&raw_message) {
                        match OwnedJson::try_new(json_data) {
                            Ok(message) => yield message,
                            Err(e) => {
                                tracing::warn!(error = ?e, "Failed to parse SSE message as JSON, skipping");
                                continue;
                            }
                        }
                    }
                }
            }
        }
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

    /// Handle a bidirectional request from the server during SSE streaming with concurrent message processing
    pub(super) async fn handle_incoming_request(
        &self,
        incoming_method: Method,
        incoming_id: RequestId,
        owned_message: OwnedJson<Message<'static>>,
    ) -> Result<()> {
        tracing::debug!(
            method = %incoming_method,
            request_id = %incoming_id,
            "Processing bidirectional request from server with concurrent message handling"
        );

        let Some(handler) = MessageHandlerRegistry::instance().get_method_handler(incoming_method)
        else {
            tracing::warn!(
                method = %incoming_method,
                request_id = %incoming_id,
                "No handler registered for bidirectional method"
            );
            self.send_error_response(
                &incoming_id,
                -32601,
                &format!("Method not found: {incoming_method}"),
            )
            .await;
            return Ok(());
        };

        // Handle the request asynchronously with concurrent outgoing message processing
        let context = self.context.clone();
        let client_handle = self.clone();

        let future = async move {
            let (outgoing_tx, mut outgoing_rx) = mpsc::channel(100);

            // Extract the request from the owned message
            let request = match owned_message.message() {
                Message::Request(req) => req,
                _ => {
                    tracing::error!(
                        expected = "request",
                        actual = ?owned_message.message(),
                        "Invalid message type in bidirectional request handler"
                    );
                    return;
                }
            };

            // Start the handler and the message sender concurrently
            let handler_future = handler.handle_message(request, outgoing_tx, context.clone());
            let sender_future = async {
                while let Some(outgoing_message) = outgoing_rx.recv().await {
                    if let Err(e) = client_handle
                        .send_response_to_server(&outgoing_message)
                        .await
                    {
                        tracing::error!(
                            request_id = %request.id,
                            error = ?e,
                            "Failed to send outgoing message"
                        );
                    }
                }
            };

            // Run handler and sender concurrently
            let (handler_result, _) = tokio::join!(handler_future, sender_future);

            // Handle the result
            match handler_result {
                Ok(()) => {
                    tracing::debug!(
                        request_id = %request.id,
                        "Handler completed successfully"
                    );
                }
                Err(e) => {
                    tracing::error!(
                        method = %request.method,
                        request_id = %request.id,
                        error = ?e,
                        "Handler execution failed for bidirectional request"
                    );
                    client_handle
                        .send_error_response(
                            &request.id,
                            -32601,
                            &format!("Failed to handle method {}: {:?}", request.method, e),
                        )
                        .await;
                }
            }
        };
        tokio::spawn(future);
        Ok(())
    }

    /// Helper method to send JSON-RPC error responses back to server
    async fn send_error_response(&self, request_id: &RequestId, code: i32, message: &str) {
        let error_response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "error": {
                "code": code,
                "message": message
            }
        });

        if let Err(e) = self
            .send_response_to_server(&error_response.to_string())
            .await
        {
            tracing::error!(
                error = ?e,
                request_id = %request_id,
                "Failed to send error response for bidirectional request"
            );
        }
    }

    /// Send a response back to the server (used for bidirectional communication and notifications)
    pub async fn send_response_to_server(&self, message: &str) -> Result<()> {
        tracing::debug!(message_size = message.len(), "Sending response to server");
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
            tracing::warn!(
                status = %status,
                response = ?response_text,
                "Server returned unexpected status for response"
            );
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
        BlobId, FlowResult,
        workflow::{Flow, ValueRef},
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
            _flow_id: BlobId,
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

        fn get_execution_metadata(
            &self,
            _step_id: Option<&str>,
        ) -> BoxFuture<'_, PluginResult<stepflow_plugin::ExecutionMetadata>> {
            async move { Ok(stepflow_plugin::ExecutionMetadata::empty()) }.boxed()
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
