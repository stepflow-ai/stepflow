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

//! HTTP client implementation for Stepflow protocol.
//!
//! This module implements the Streamable HTTP transport where:
//! - Send JSON-RPC requests to the main endpoint
//! - Receive either direct JSON responses or SSE streams
//! - Handle bidirectional communication through SSE when needed

use std::sync::{Arc, Weak};

use error_stack::ResultExt as _;
use futures::stream::StreamExt as _;
use stepflow_plugin::{Context, RunContext};
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
    pub async fn try_new(url: String, context: Weak<dyn Context>) -> Result<Self> {
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
    context: Weak<dyn Context>,
    request_headers: reqwest::header::HeaderMap,
}

impl HttpClientHandle {
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Send a typed method request and return the typed response
    /// This follows the same pattern as StdioClientHandle
    ///
    /// # Arguments
    /// * `params` - The method parameters
    /// * `run_context` - Optional run context for bidirectional communication.
    ///   Required if the call may result in SSE streaming (bidirectional).
    pub async fn method<I>(
        &self,
        params: &I,
        run_context: Option<Arc<RunContext>>,
    ) -> Result<I::Response>
    where
        I: ProtocolMethod + serde::Serialize + Send + Sync + std::fmt::Debug,
        I::Response: DeserializeOwned + Send + Sync + 'static,
    {
        let response = self
            .method_dyn(I::METHOD_NAME, LazyValue::write_ref(params), run_context)
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
        run_context: Option<Arc<RunContext>>,
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
            .send_method_request(id.clone(), request_str, run_context)
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
    ///
    /// # Arguments
    /// * `id` - The request ID
    /// * `message` - The serialized JSON-RPC request
    /// * `run_context` - Run context for bidirectional communication. Required if
    ///   the response may be an SSE stream (bidirectional mode).
    async fn send_method_request(
        &self,
        id: RequestId,
        message: String,
        run_context: Option<Arc<RunContext>>,
    ) -> Result<OwnedJson> {
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
            log::debug!(
                "request_id={}: Starting SSE stream processing for bidirectional communication",
                id
            );

            // SSE mode requires run_context for bidirectional handlers
            let run_context = run_context.ok_or_else(|| {
                error_stack::report!(TransportError::Recv)
                    .attach_printable("SSE response received but no RunContext provided")
            })?;

            // Extract instance ID from response headers for routing bidirectional requests
            let instance_id = response
                .headers()
                .get("Stepflow-Instance-Id")
                .and_then(|v| v.to_str().ok())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());

            if let Some(ref id) = instance_id {
                log::debug!(
                    "instance_id={}: Extracted instance ID for bidirectional routing",
                    id
                );
            } else if response.headers().get("Stepflow-Instance-Id").is_some() {
                log::warn!("Invalid or empty Stepflow-Instance-Id header value");
            }

            // Create a bidirectional driver to handle the SSE stream
            let driver = BidirectionalDriver::new(self.clone(), instance_id, run_context);
            let messages = self.sse_messages(response);
            driver.drive_to_completion(id, messages).await
        } else {
            // Handle direct JSON response
            let response_text = response
                .text()
                .await
                .change_context(TransportError::Recv)
                .attach_printable("Failed to read response body")?;

            log::debug!(
                "request_id={}, response_size={}: Received direct JSON response",
                id,
                response_text.len()
            );

            let owned_json = OwnedJson::try_new(response_text)
                .change_context(TransportError::Recv)
                .attach_printable("Failed to parse response as JSON")?;

            log::debug!("Received JSON response: {:?}", owned_json);

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
                        log::warn!("error={}", e);
                        continue;
                    }
                };

                // Convert to string
                let chunk_str = match std::str::from_utf8(&chunk) {
                    Ok(s) => s,
                    Err(e) => {
                        log::warn!("error={}", e);
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
                                log::warn!("error={:?}", e);
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
        instance_id: Option<&str>,
        run_context: &Arc<RunContext>,
    ) -> Result<()> {
        log::debug!(
            "method={}, request_id={}: Processing bidirectional request from server with concurrent message handling",
            incoming_method,
            incoming_id
        );

        let Some(handler) = MessageHandlerRegistry::instance().get_method_handler(incoming_method)
        else {
            log::warn!(
                "method={}, request_id={}: No handler registered for bidirectional method",
                incoming_method,
                incoming_id
            );
            self.send_error_response(
                &incoming_id,
                -32601,
                &format!("Method not found: {incoming_method}"),
                instance_id,
            )
            .await;
            return Ok(());
        };

        // Handle the request asynchronously with concurrent outgoing message processing
        let context = match self.context.upgrade() {
            Some(ctx) => ctx,
            None => {
                log::warn!("Context no longer available for bidirectional request");
                return Ok(());
            }
        };
        let client_handle = self.clone();
        let instance_id = instance_id.map(|s| s.to_string());
        let run_context = run_context.clone();

        let future = async move {
            let (outgoing_tx, mut outgoing_rx) = mpsc::channel(100);

            // Extract the request from the owned message
            let request = match owned_message.message() {
                Message::Request(req) => req,
                _ => {
                    log::error!(
                        "expected={}, actual={:?}: Invalid message type in bidirectional request handler",
                        "request",
                        owned_message.message()
                    );
                    return;
                }
            };

            // Start the handler and the message sender concurrently
            let handler_future =
                handler.handle_message(request, outgoing_tx, context, Some(&run_context));
            let sender_future = async {
                while let Some(outgoing_message) = outgoing_rx.recv().await {
                    if let Err(e) = client_handle
                        .send_response_to_server(&outgoing_message, instance_id.as_deref())
                        .await
                    {
                        log::error!(
                            "request_id={}, error={:?}: Failed to send outgoing message",
                            request.id,
                            e
                        );
                    }
                }
            };

            // Run handler and sender concurrently
            let (handler_result, _) = tokio::join!(handler_future, sender_future);

            // Handle the result
            match handler_result {
                Ok(()) => {
                    log::debug!("request_id={}: Handler completed successfully", request.id);
                }
                Err(e) => {
                    log::error!(
                        "method={}, request_id={}, error={:?}: Handler execution failed for bidirectional request",
                        request.method,
                        request.id,
                        e
                    );
                    client_handle
                        .send_error_response(
                            &request.id,
                            -32601,
                            &format!("Failed to handle method {}: {:?}", request.method, e),
                            instance_id.as_deref(),
                        )
                        .await;
                }
            }
        };
        tokio::spawn(future);
        Ok(())
    }

    /// Helper method to send JSON-RPC error responses back to server
    async fn send_error_response(
        &self,
        request_id: &RequestId,
        code: i32,
        message: &str,
        instance_id: Option<&str>,
    ) {
        let error_response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "error": {
                "code": code,
                "message": message
            }
        });

        if let Err(e) = self
            .send_response_to_server(&error_response.to_string(), instance_id)
            .await
        {
            log::error!(
                "error={:?}, request_id={}: Failed to send error response for bidirectional request",
                e,
                request_id
            );
        }
    }

    /// Send a response back to the server (used for bidirectional communication and notifications)
    pub async fn send_response_to_server(
        &self,
        message: &str,
        instance_id: Option<&str>,
    ) -> Result<()> {
        log::debug!("message_size={}: Sending response to server", message.len());

        // Clone headers and add instance ID if available for load balancer routing
        let mut headers = self.request_headers.clone();
        if let Some(id) = instance_id {
            headers.insert("Stepflow-Instance-Id", id.parse().unwrap());
            log::debug!(
                "instance_id={}: Including instance ID in bidirectional response",
                id
            );
        }

        let response = self
            .client
            .post(&self.url)
            .headers(headers)
            .body(message.to_string())
            .send()
            .await
            .change_context(TransportError::Send)
            .attach_printable("Failed to send response to server")?;

        // Server should return 202 Accepted for responses
        let status = response.status();
        if status != reqwest::StatusCode::ACCEPTED {
            let response_text = response.text().await;
            log::warn!(
                "status={}, response={:?}: Server returned unexpected status for response",
                status,
                response_text
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono;
    use futures::future::{BoxFuture, FutureExt as _};
    use std::path::Path;
    use std::sync::Arc;
    use stepflow_core::{FlowResult, GetRunOptions, SubmitRunParams, workflow::ValueRef};
    use stepflow_plugin::{Context, Result as PluginResult};
    use stepflow_state::{InMemoryStateStore, ItemResult, ItemStatistics, RunStatus, StateStore};
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
        fn submit_run(&self, params: SubmitRunParams) -> BoxFuture<'_, PluginResult<RunStatus>> {
            let input_count = params.inputs.len();
            async move {
                let flow_id = stepflow_core::BlobId::new(
                    "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
                )
                .expect("mock blob id");
                let now = chrono::Utc::now();

                Ok(RunStatus {
                    run_id: Uuid::now_v7(),
                    flow_id,
                    flow_name: None,
                    status: stepflow_core::status::ExecutionStatus::Running,
                    items: ItemStatistics {
                        total: input_count,
                        completed: 0,
                        running: input_count,
                        failed: 0,
                        cancelled: 0,
                    },
                    created_at: now,
                    completed_at: None,
                    results: None,
                })
            }
            .boxed()
        }

        fn get_run(
            &self,
            run_id: Uuid,
            options: GetRunOptions,
        ) -> BoxFuture<'_, PluginResult<RunStatus>> {
            async move {
                let flow_id = stepflow_core::BlobId::new(
                    "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
                )
                .expect("mock blob id");
                let now = chrono::Utc::now();

                let results = if options.include_results {
                    Some(vec![ItemResult {
                        item_index: 0,
                        status: stepflow_core::status::ExecutionStatus::Completed,
                        result: Some(FlowResult::Success(ValueRef::new(
                            serde_json::json!({"message": "Hello from mock"}),
                        ))),
                        completed_at: Some(now),
                    }])
                } else {
                    None
                };

                Ok(RunStatus {
                    run_id,
                    flow_id,
                    flow_name: None,
                    status: stepflow_core::status::ExecutionStatus::Completed,
                    items: ItemStatistics {
                        total: 1,
                        completed: 1,
                        running: 0,
                        failed: 0,
                        cancelled: 0,
                    },
                    created_at: now,
                    completed_at: Some(now),
                    results,
                })
            }
            .boxed()
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
        let result = HttpClient::try_new(url, Arc::downgrade(&context)).await;
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

        if let Some(id_value) = request_with_string_id.get("id")
            && let Some(s) = id_value.as_str()
        {
            let request_id = RequestId::from(s.to_string());
            assert_eq!(format!("{request_id}"), "test-string-id");
        }

        // Test integer ID
        let request_with_int_id = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "test",
            "id": 42
        });

        if let Some(id_value) = request_with_int_id.get("id")
            && let Some(i) = id_value.as_i64()
        {
            let request_id = RequestId::from(i);
            assert_eq!(format!("{request_id}"), "42");
        }
    }
}
