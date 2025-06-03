use serde_json::value::RawValue;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, LazyLock};
use stepflow_plugin::Context;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::stdio::StdioError;
use crate::{GetBlobHandler, PutBlobHandler};

/// Trait for handling incoming method calls and notifications from component servers.
///
/// This enables bidirectional communication where component servers can
/// call methods on the stepflow runtime.
pub trait IncomingHandler: Send + Sync {
    /// Handle an incoming method call or notification.
    ///
    /// # Arguments
    /// * `method` - The method name being called
    /// * `params` - The JSON parameters for the method
    /// * `id` - The request ID (None for notifications)
    /// * `response_tx` - Channel to send the response back to the component server
    ///
    /// # Returns
    /// Result indicating if the handling was successful
    fn handle_incoming(
        &self,
        method: String,
        params: Box<RawValue>,
        id: Option<Uuid>,
        response_tx: mpsc::Sender<String>,
        context: Arc<dyn Context>,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<(), StdioError>> + Send>>;
}

/// Registry for incoming method handlers.
///
/// Maps method names to their corresponding handlers using a trait object approach.
pub struct IncomingHandlerRegistry {
    handlers: HashMap<String, Box<dyn IncomingHandler>>,
}

static INCOMING_HANDLERS: LazyLock<IncomingHandlerRegistry> = LazyLock::new(|| {
    let mut registry = IncomingHandlerRegistry::new();
    registry.register("put_blob", Box::new(PutBlobHandler));
    registry.register("get_blob", Box::new(GetBlobHandler));
    registry
});

impl IncomingHandlerRegistry {
    /// Create a new empty registry.
    fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    /// Register a handler for a specific method.
    fn register(&mut self, method_name: impl Into<String>, handler: Box<dyn IncomingHandler>) {
        self.handlers.insert(method_name.into(), handler);
    }

    pub fn instance() -> &'static Self {
        &INCOMING_HANDLERS
    }

    /// Handle an incoming method call or notification by spawning a task.
    ///
    /// This spawns a task to avoid blocking the receive loop.
    pub fn spawn_handle_incoming(
        &self,
        method: String,
        params: Box<RawValue>,
        id: Option<Uuid>,
        response_tx: mpsc::Sender<String>,
        context: Arc<dyn Context>,
    ) {
        if let Some(handler) = self.handlers.get(&method) {
            // Now we can spawn the handler with owned values
            let future =
                handler.handle_incoming(method.clone(), params, id, response_tx.clone(), context);
            tokio::spawn(async move {
                if let Err(e) = future.await {
                    tracing::error!("Handler error for method {}: {}", method, e);
                }
            });
        } else {
            // Send error response for unknown method if it's a method call (has ID)
            if let Some(id) = id {
                tracing::error!("Unknown method: {}", method);
                tokio::spawn(async move {
                    let error_message = format!("Unknown method: {}", method);
                    let error_response = crate::schema::ResponseMessage {
                        jsonrpc: "2.0",
                        id,
                        result: None,
                        error: Some(crate::schema::RemoteError {
                            code: -32601, // Method not found
                            message: &error_message,
                            data: None,
                        }),
                    };

                    if let Ok(response_json) = serde_json::to_string(&error_response) {
                        let _ = response_tx.send(response_json).await;
                    }
                });
            } else {
                // For notifications, we just ignore unknown methods
                tracing::error!("Unknown notification method: {}", method);
            }
        }
    }
}

impl Default for IncomingHandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}
