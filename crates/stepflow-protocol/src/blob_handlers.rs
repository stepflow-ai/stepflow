use error_stack::ResultExt as _;
use futures::future::{BoxFuture, FutureExt as _};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::sync::Arc;
use stepflow_plugin::Context;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::{IncomingHandler, schema, stdio::StdioError};

/// Helper function to handle a method call with request/response types.
async fn handle_method_call<Req, Resp, F, Fut>(
    id: Uuid,
    params: &RawValue,
    response_tx: &mpsc::Sender<String>,
    handler: F,
) -> error_stack::Result<(), StdioError>
where
    Req: for<'de> Deserialize<'de>,
    Resp: Serialize,
    F: FnOnce(Req) -> Fut,
    Fut: Future<Output = Result<Resp, String>>,
{
    let response_json = match serde_json::from_str::<Req>(params.get()) {
        Ok(request) => match handler(request).await {
            Ok(response_data) => match serde_json::to_value(response_data) {
                Ok(result_value) => {
                    let result_raw = serde_json::value::to_raw_value(&result_value)
                        .change_context(StdioError::Send)?;
                    let response = schema::ResponseMessage {
                        jsonrpc: "2.0",
                        id,
                        result: Some(result_raw.as_ref()),
                        error: None,
                    };
                    serde_json::to_string(&response).change_context(StdioError::Send)?
                }
                Err(e) => {
                    let error_msg = format!("Serialization error: {}", e);
                    create_error_response_json(id, -32000, &error_msg)?
                }
            },
            Err(e) => create_error_response_json(id, -32000, &e)?,
        },
        Err(e) => {
            let error_msg = format!("Invalid request: {}", e);
            create_error_response_json(id, -32602, &error_msg)?
        }
    };

    response_tx
        .send(response_json)
        .await
        .change_context(StdioError::Send)?;

    Ok(())
}

/// Helper to create error response JSON
fn create_error_response_json(
    id: Uuid,
    code: i64,
    message: &str,
) -> error_stack::Result<String, StdioError> {
    let response = schema::ResponseMessage {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(schema::RemoteError {
            code,
            message,
            data: None,
        }),
    };
    serde_json::to_string(&response).change_context(StdioError::Send)
}

/// Handler for put_blob method calls from component servers.
pub struct PutBlobHandler;

impl IncomingHandler for PutBlobHandler {
    fn handle_incoming(
        &self,
        _method: String,
        params: Box<RawValue>,
        id: Option<Uuid>,
        response_tx: mpsc::Sender<String>,
        context: Arc<dyn Context>,
    ) -> BoxFuture<'static, error_stack::Result<(), StdioError>> {
        let state_store = context.state_store().clone();

        async move {
            // Only handle method calls (with ID)
            let id = match id {
                Some(id) => id,
                None => return Ok(()), // Ignore notifications
            };

            handle_method_call(
                id,
                &params,
                &response_tx,
                |request: schema::put_blob::Request| {
                    let state_store = state_store.clone();
                    async move {
                        let blob_id = state_store
                            .put_blob(request.data)
                            .await
                            .map_err(|e| format!("Blob store error: {}", e))?;
                        Ok(schema::put_blob::Response { blob_id })
                    }
                },
            )
            .await
        }
        .boxed()
    }
}

/// Handler for get_blob method calls from component servers.
pub struct GetBlobHandler;

impl IncomingHandler for GetBlobHandler {
    fn handle_incoming(
        &self,
        _method: String,
        params: Box<RawValue>,
        id: Option<Uuid>,
        response_tx: mpsc::Sender<String>,
        context: Arc<dyn Context>,
    ) -> BoxFuture<'static, error_stack::Result<(), StdioError>> {
        let state_store = context.state_store().clone();

        async move {
            // Only handle method calls (with ID)
            let id = match id {
                Some(id) => id,
                None => return Ok(()), // Ignore notifications
            };

            handle_method_call(
                id,
                &params,
                &response_tx,
                |request: schema::get_blob::Request| {
                    let state_store = state_store.clone();
                    async move {
                        let data = state_store
                            .get_blob(&request.blob_id)
                            .await
                            .map_err(|e| format!("Blob retrieve error: {}", e))?;
                        Ok(schema::get_blob::Response { data })
                    }
                },
            )
            .await
        }
        .boxed()
    }
}

/// Handler for streaming_chunk notifications from component servers.
pub struct StreamingChunkHandler;

impl IncomingHandler for StreamingChunkHandler {
    fn handle_incoming(
        &self,
        _method: String,
        params: Box<RawValue>,
        id: Option<Uuid>,
        response_tx: mpsc::Sender<String>,
        context: Arc<dyn Context>,
    ) -> BoxFuture<'static, error_stack::Result<(), StdioError>> {
        async move {
            // This is a notification (no ID), so we don't send a response
            // Instead, we need to handle the streaming chunk
            match serde_json::from_str::<StreamingChunkNotification>(params.get()) {
                Ok(notification) => {
                    tracing::info!("Received streaming chunk for request {}: {:?}", 
                                  notification.request_id, notification.chunk);
                    
                    // Route this chunk to the appropriate workflow executor
                    if let Some(executor) = context.executor() {
                        if let Ok(execution_id) = Uuid::parse_str(&notification.request_id) {
                            // Try to find the workflow executor for this execution
                            if let Ok(Some(mut workflow_executor)) = executor.get_workflow_executor(execution_id).await {
                                // For now, just log that we received the chunk
                                // TODO: Implement proper chunk routing when the streaming pipeline is ready
                                tracing::info!("Received streaming chunk for execution {}: {:?}", 
                                              execution_id, notification.chunk);
                                tracing::warn!("Streaming chunk routing not yet implemented for trait objects");
                            } else {
                                tracing::warn!("No workflow executor found for execution ID: {}", execution_id);
                            }
                        } else {
                            tracing::warn!("Invalid execution ID in streaming chunk: {}", notification.request_id);
                        }
                    } else {
                        tracing::warn!("No executor available in context for streaming chunk routing");
                    }
                    
                    Ok(())
                }
                Err(e) => {
                    tracing::error!("Failed to parse streaming chunk notification: {}", e);
                    Ok(())
                }
            }
        }
        .boxed()
    }
}

#[derive(Debug, Deserialize)]
struct StreamingChunkNotification {
    request_id: String,
    chunk: serde_json::Value,
}
