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

use futures::future::BoxFuture;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use stepflow_plugin::Context;
use tokio::sync::mpsc;

use super::blob_handlers::{GetBlobHandler, PutBlobHandler};
use crate::protocol::{Method, MethodRequest};
use crate::error::TransportError;

/// Trait for handling incoming method calls and notifications from component servers.
///
/// This enables bidirectional communication where component servers can
/// call methods on the stepflow runtime.
pub trait MethodHandler: Send + Sync {
    /// Handle an incoming method call or notification.
    ///
    /// # Arguments
    /// * `request` - The method request to handle
    /// * `outgoing_tx` - Channel to send messages back to the component server
    ///
    /// # Returns
    /// Result indicating if the handling was successful
    fn handle_message<'a>(
        &self,
        request: &'a MethodRequest<'a>,
        outgoing_tx: mpsc::Sender<String>,
        context: Arc<dyn Context>,
    ) -> BoxFuture<'a, error_stack::Result<(), TransportError>>;
}

/// Registry for incoming message handlers.
///
/// Maps method names to their corresponding handlers using a trait object.
pub struct MessageHandlerRegistry {
    method_handlers: HashMap<Method, Box<dyn MethodHandler>>,
}

static INCOMING_HANDLERS: LazyLock<MessageHandlerRegistry> = LazyLock::new(|| {
    let mut registry = MessageHandlerRegistry::new();
    registry.register_method(Method::BlobsGet, Box::new(GetBlobHandler));
    registry.register_method(Method::BlobsPut, Box::new(PutBlobHandler));
    registry
});

impl MessageHandlerRegistry {
    /// Create a new empty registry.
    fn new() -> Self {
        Self {
            method_handlers: HashMap::new(),
        }
    }

    /// Register a handler for a specific method.
    fn register_method(&mut self, method_name: Method, handler: Box<dyn MethodHandler>) {
        self.method_handlers.insert(method_name, handler);
    }

    pub fn instance() -> &'static Self {
        &INCOMING_HANDLERS
    }

    pub fn get_method_handler(&self, method_name: Method) -> Option<&dyn MethodHandler> {
        self.method_handlers.get(&method_name).map(|h| h.as_ref())
    }
}

impl Default for MessageHandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}
