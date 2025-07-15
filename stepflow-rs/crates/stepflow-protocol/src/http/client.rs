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

use std::sync::Arc;

use error_stack::ResultExt as _;
use futures::stream::StreamExt as _;
use reqwest_eventsource::{Event, EventSource};
use stepflow_plugin::Context;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::Instrument as _;

use crate::OwnedJson;
use crate::error::{Result, TransportError};
use crate::{Message, MessageHandlerRegistry, RequestId};

/// HTTP client that communicates with a remote component server
pub struct HttpClient {
    url: String,
    client: reqwest::Client,
    outgoing_tx: mpsc::Sender<String>,
    pending_tx: mpsc::Sender<(RequestId, oneshot::Sender<OwnedJson>)>,
    // TODO: Use the handle for error checking
    #[allow(dead_code)]
    loop_handle: JoinHandle<Result<()>>,
}

impl HttpClient {
    pub async fn try_new(url: String, context: Arc<dyn Context>) -> Result<Self> {
        let client = reqwest::Client::new();
        let (outgoing_tx, outgoing_rx) = mpsc::channel(100);
        let (pending_tx, pending_rx) = mpsc::channel(100);

        let http_span = tracing::info_span!("http_message_loop", url = %url);
        let loop_handle = tokio::spawn(
            http_message_loop(
                client.clone(),
                url.clone(),
                outgoing_tx.clone(),
                outgoing_rx,
                pending_rx,
                context,
            )
            .instrument(http_span),
        );

        Ok(Self {
            url,
            client,
            outgoing_tx,
            pending_tx,
            loop_handle,
        })
    }

    pub fn handle(&self) -> HttpClientHandle {
        HttpClientHandle {
            client: self.client.clone(),
            url: self.url.clone(),
            outgoing_tx: self.outgoing_tx.clone(),
            pending_tx: self.pending_tx.clone(),
        }
    }
}

#[derive(Clone)]
pub struct HttpClientHandle {
    client: reqwest::Client,
    url: String,
    outgoing_tx: mpsc::Sender<String>,
    pending_tx: mpsc::Sender<(RequestId, oneshot::Sender<OwnedJson>)>,
}

impl HttpClientHandle {
    pub async fn send_message(&self, message: String) -> Result<()> {
        self.outgoing_tx
            .send(message)
            .await
            .map_err(|_| TransportError::Send)?;
        Ok(())
    }

    pub async fn register_pending(
        &self,
        id: RequestId,
        sender: oneshot::Sender<OwnedJson>,
    ) -> Result<()> {
        self.pending_tx
            .send((id, sender))
            .await
            .map_err(|_| TransportError::Send)?;
        Ok(())
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    pub fn url(&self) -> &str {
        &self.url
    }
}

async fn http_message_loop(
    client: reqwest::Client,
    base_url: String,
    outgoing_tx: mpsc::Sender<String>,
    mut outgoing_rx: mpsc::Receiver<String>,
    mut pending_rx: mpsc::Receiver<(RequestId, oneshot::Sender<OwnedJson>)>,
    context: Arc<dyn Context>,
) -> Result<()> {
    // Set up SSE connection for incoming messages
    let events_url = format!("{base_url}/runtime/events");
    let mut event_source = EventSource::get(&events_url);

    // Track pending requests
    let mut pending_requests = std::collections::HashMap::new();

    // Wait for endpoint negotiation (MCP-style session negotiation)
    let mut endpoint_url = None;
    let mut session_negotiated = false;

    tracing::info!("Waiting for endpoint negotiation...");

    // Set up fallback timeout for non-MCP servers
    let negotiation_timeout = tokio::time::sleep(tokio::time::Duration::from_secs(5));
    tokio::pin!(negotiation_timeout);

    loop {
        tokio::select! {
            // Handle outgoing messages (only after session negotiation)
            Some(message) = outgoing_rx.recv(), if session_negotiated => {
                tracing::debug!("Sending HTTP message: {}", message);

                // Use the negotiated endpoint URL if available
                let target_url = endpoint_url.as_ref().unwrap_or(&base_url);

                // Send JSON-RPC message via HTTP POST
                let response = client
                    .post(target_url)
                    .header("Content-Type", "application/json")
                    .body(message)
                    .send()
                    .await
                    .change_context(TransportError::Send)?;

                if !response.status().is_success() {
                    tracing::error!("HTTP request failed with status: {}", response.status());
                    return Err(TransportError::Send.into());
                }

                // For HTTP, we expect the response to be processed via SSE
                // unless it's a synchronous response
                let response_text = response.text().await.change_context(TransportError::Recv)?;
                if !response_text.is_empty() {
                    tracing::debug!("Received HTTP response: {}", response_text);

                    // Parse and handle the response
                    let owned_json = OwnedJson::try_new(response_text).change_context(TransportError::Recv)?;
                    handle_message(owned_json, &mut pending_requests, &context, &outgoing_tx).await?;
                }
            }

            // Handle SSE events
            Some(event) = event_source.next() => {
                match event {
                    Ok(Event::Open) => {
                        tracing::info!("SSE connection opened");
                    }
                    Ok(Event::Message(msg)) => {
                        tracing::debug!("Received SSE event: {} - {}", msg.event, msg.data);

                        // Handle different event types
                        match msg.event.as_str() {
                            "endpoint" => {
                                // Handle MCP-style endpoint negotiation
                                if let Ok(endpoint_data) = serde_json::from_str::<serde_json::Value>(&msg.data) {
                                    if let Some(endpoint) = endpoint_data.get("endpoint").and_then(|v| v.as_str()) {
                                        // Construct full URL from relative endpoint
                                        let full_endpoint = if endpoint.starts_with("/") {
                                            format!("{base_url}{endpoint}")
                                        } else {
                                            endpoint.to_string()
                                        };

                                        tracing::info!("Received endpoint: {}", full_endpoint);
                                        endpoint_url = Some(full_endpoint);
                                        session_negotiated = true;
                                    } else {
                                        tracing::error!("Invalid endpoint event: missing endpoint field");
                                        return Err(TransportError::Recv.into());
                                    }
                                } else {
                                    tracing::error!("Failed to parse endpoint event data");
                                    return Err(TransportError::Recv.into());
                                }
                            }
                            "message" => {
                                // Handle JSON-RPC message
                                let owned_json = OwnedJson::try_new(msg.data).change_context(TransportError::Recv)?;
                                handle_message(owned_json, &mut pending_requests, &context, &outgoing_tx).await?;
                            }
                            "keepalive" => {
                                // Ignore keepalive events
                                tracing::debug!("Received keepalive event");
                            }
                            _ => {
                                tracing::warn!("Unknown SSE event type: {}", msg.event);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("SSE error: {}", e);
                        return Err(TransportError::Recv.into());
                    }
                }
            }

            // Handle new pending requests
            Some((id, sender)) = pending_rx.recv() => {
                pending_requests.insert(id, sender);
            }

            // Handle session negotiation timeout (fallback for non-MCP servers)
            _ = &mut negotiation_timeout, if !session_negotiated => {
                tracing::warn!("Session negotiation timeout - falling back to direct HTTP communication");
                session_negotiated = true;
                // endpoint_url remains None, so we'll use base_url directly
            }

            else => {
                tracing::info!("HTTP message loop exiting");
                break;
            }
        }
    }

    Ok(())
}

async fn handle_message(
    owned_json: OwnedJson,
    pending_requests: &mut std::collections::HashMap<RequestId, oneshot::Sender<OwnedJson>>,
    context: &Arc<dyn Context>,
    outgoing_tx: &mpsc::Sender<String>,
) -> Result<()> {
    match owned_json.message() {
        Message::Request(request) => {
            tracing::info!("Received request for method '{}'", request.method);

            let Some(handler) =
                MessageHandlerRegistry::instance().get_method_handler(request.method)
            else {
                tracing::warn!("No handler found for method '{}'", request.method);

                // Send an error response
                let response = Message::Response(crate::MethodResponse::error(
                    request.id.clone(),
                    crate::Error::method_not_found(request.method),
                ));
                let response_json =
                    serde_json::to_string(&response).change_context(TransportError::Send)?;
                outgoing_tx
                    .send(response_json)
                    .await
                    .map_err(|_| TransportError::Send)?;
                return Ok(());
            };

            // Handle the request asynchronously
            let outgoing_tx = outgoing_tx.clone();
            let context = context.clone();

            let future = async move {
                let Message::Request(request) = owned_json.message() else {
                    panic!("Expected a request message");
                };

                if let Err(err) = handler
                    .handle_message(request, outgoing_tx, context.clone())
                    .await
                {
                    tracing::error!(
                        "Error handling request for method '{}': {:?}",
                        request.method,
                        err
                    );
                }
            };
            tokio::spawn(future);
        }
        Message::Notification(notification) => {
            tracing::error!(
                "Received unsupported notification for method '{}'",
                notification.method
            );
        }
        Message::Response(response) => {
            tracing::info!("Received response with id '{}'", response.id());

            if let Some(pending) = pending_requests.remove(response.id()) {
                tracing::info!(
                    "Sending response to pending request with id '{}'",
                    response.id()
                );
                pending.send(owned_json).map_err(|_| TransportError::Send)?;
            } else {
                tracing::warn!("Unexpected response with id '{}'", response.id());
            }
        }
    }

    Ok(())
}
