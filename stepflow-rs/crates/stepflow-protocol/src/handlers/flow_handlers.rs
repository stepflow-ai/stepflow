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

use futures::future::{BoxFuture, FutureExt as _};
use std::sync::Arc;
use stepflow_plugin::Context;
use tokio::sync::mpsc;

use crate::error::TransportError;
use crate::{Error, MethodHandler, MethodRequest};

use super::blob_handlers::handle_method_call;

/// Handler for flow evaluation method calls from component servers.
pub struct EvaluateFlowHandler;

impl MethodHandler for EvaluateFlowHandler {
    fn handle_message<'a>(
        &self,
        request: &'a MethodRequest<'a>,
        response_tx: mpsc::Sender<String>,
        context: Arc<dyn Context>,
    ) -> BoxFuture<'a, error_stack::Result<(), TransportError>> {
        handle_method_call(
            request,
            response_tx,
            |request: crate::protocol::EvaluateFlowParams| async move {
                // Execute the flow using the shared utility
                let result = context
                    .execute_flow_by_id(&request.flow_id, request.input)
                    .await
                    .map_err(|e| {
                        tracing::error!("Failed to evaluate flow: {e}");
                        Error::internal("Failed to evaluate flow")
                    })?;

                Ok(crate::protocol::EvaluateFlowResult { result })
            },
        )
        .boxed()
    }
}
