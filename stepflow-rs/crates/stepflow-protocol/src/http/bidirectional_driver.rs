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

//! Bidirectional communication driver for SSE streams
//!
//! This module handles the complex logic of managing SSE streams with concurrent
//! bidirectional request processing, keeping the main HTTP client clean and focused.

use futures::stream::{FuturesUnordered, StreamExt as _};
use tokio::task::JoinHandle;

use crate::error::{Result, TransportError};
use crate::protocol::Method;
use crate::{Message, OwnedJson, RequestId};

/// Drives bidirectional communication over an SSE stream
///
/// When a server implementing Streamable HTTP receives a request, it either
/// responds to the POST with the response, or creates an SSE stream to execute
/// the component if it needs bidirectional communication. This drives the
/// client side of the latter.
///
/// This encapsulates all the logic for:
/// - Processing incoming SSE messages containing requests from the server
///   before it is able to produce the final response
/// - Spawning concurrent request handlers
/// - Tracking task completions
/// - Waiting for the final response
pub struct BidirectionalDriver {
    client_handle: super::HttpClientHandle,
}

/// Result of processing an SSE message
#[derive(Debug)]
enum SseMessageEvent {
    FinalResponse(OwnedJson),
    StreamEnded,
    Continue,
}

impl BidirectionalDriver {
    /// Create a new bidirectional driver
    pub fn new(client_handle: super::HttpClientHandle) -> Self {
        Self { client_handle }
    }

    /// Drive the SSE stream until the expected response is received and all requests complete
    pub async fn drive_to_completion(
        &self,
        expected_id: RequestId,
        messages: impl futures::Stream<Item = OwnedJson>,
    ) -> Result<OwnedJson> {
        tokio::pin!(messages);
        let mut pending_requests: FuturesUnordered<JoinHandle<()>> = FuturesUnordered::new();
        let mut final_response: Option<OwnedJson> = None;

        loop {
            tokio::select! {
                // Process new messages from SSE stream
                message_result = messages.next() => {
                    match self.handle_sse_message_event(message_result, &expected_id, &mut pending_requests).await? {
                        SseMessageEvent::FinalResponse(response) => {
                            final_response = Some(response);
                            break;
                        }
                        SseMessageEvent::StreamEnded => {
                            tracing::debug!("SSE stream ended, waiting for pending requests to complete");
                            break;
                        }
                        SseMessageEvent::Continue => {
                            // Continue processing
                        }
                    }
                }

                // Handle completion of bidirectional requests
                task_result = pending_requests.next(), if !pending_requests.is_empty() => {
                    self.handle_task_completion_event(task_result);
                }
            }
        }

        // Wait for all remaining pending requests to complete
        self.wait_for_pending_requests(pending_requests).await;

        // Return the final response if we received it, otherwise error
        match final_response {
            Some(response) => {
                tracing::info!(request_id = %expected_id, ?response, "All bidirectional requests completed, returning final response");
                Ok(response)
            }
            None => Err(error_stack::Report::new(TransportError::Recv)
                .attach_printable("SSE stream ended without receiving final response")),
        }
    }

    /// Handle an SSE message event and return the appropriate action
    async fn handle_sse_message_event(
        &self,
        message_result: Option<OwnedJson>,
        expected_id: &RequestId,
        pending_requests: &mut FuturesUnordered<JoinHandle<()>>,
    ) -> Result<SseMessageEvent> {
        match message_result {
            Some(owned_json) => {
                match owned_json.message() {
                    Message::Request(request) => {
                        self.spawn_request_handler(
                            request.method,
                            request.id.clone(),
                            owned_json,
                            pending_requests,
                        )
                        .await;
                        Ok(SseMessageEvent::Continue)
                    }
                    Message::Response(response) => {
                        if response.id() == expected_id {
                            tracing::info!(request_id = %expected_id, "Received final response, will complete after pending requests");
                            Ok(SseMessageEvent::FinalResponse(owned_json))
                        } else {
                            tracing::warn!(
                                expected_id = %expected_id,
                                received_id = %response.id(),
                                "Received response for unexpected request ID"
                            );
                            Ok(SseMessageEvent::Continue)
                        }
                    }
                    Message::Notification(notification) => {
                        tracing::debug!(method = %notification.method, "Received notification from server");
                        // Notifications are fire-and-forget, no response needed
                        Ok(SseMessageEvent::Continue)
                    }
                }
            }
            None => Ok(SseMessageEvent::StreamEnded),
        }
    }

    /// Spawn a concurrent handler for a bidirectional request
    async fn spawn_request_handler(
        &self,
        method: Method,
        request_id: RequestId,
        owned_json: OwnedJson,
        pending_requests: &mut FuturesUnordered<JoinHandle<()>>,
    ) {
        let client_handle = self.client_handle.clone();

        tracing::debug!(
            method = %method,
            request_id = %request_id,
            "Spawning concurrent handler for bidirectional request"
        );

        // Spawn concurrent handler
        let handle = tokio::spawn(async move {
            if let Err(e) = client_handle
                .handle_incoming_request(method, request_id.clone(), owned_json)
                .await
            {
                tracing::error!(
                    method = %method,
                    request_id = %request_id,
                    error = ?e,
                    "Failed to handle bidirectional request"
                );
            }
        });

        pending_requests.push(handle);
    }

    /// Handle the completion of a task
    fn handle_task_completion_event(
        &self,
        task_result: Option<Result<(), tokio::task::JoinError>>,
    ) {
        if let Some(Err(join_err)) = task_result {
            tracing::error!(error = ?join_err, "Bidirectional request handler panicked");
        } else {
            tracing::debug!("Bidirectional request handler completed successfully");
        }
    }

    /// Wait for all remaining pending requests to complete
    async fn wait_for_pending_requests(
        &self,
        mut pending_requests: FuturesUnordered<JoinHandle<()>>,
    ) {
        tracing::debug!(
            pending_count = pending_requests.len(),
            "Waiting for remaining bidirectional requests to complete"
        );

        while let Some(task_result) = pending_requests.next().await {
            if let Err(join_err) = task_result {
                tracing::error!(error = ?join_err, "Bidirectional request handler panicked during cleanup");
            } else {
                tracing::debug!("Bidirectional request handler completed during cleanup");
            }
        }
    }
}
