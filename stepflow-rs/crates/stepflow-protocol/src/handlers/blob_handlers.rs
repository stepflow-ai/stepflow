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

use futures::future::{BoxFuture, FutureExt as _};
use std::sync::Arc;
use stepflow_plugin::{Context, RunContext};
use tokio::sync::mpsc;

use super::handle_method_call;
use crate::MethodRequest;
use crate::error::TransportError;
use crate::{Error, MethodHandler};

/// Handler for put_blob method calls from component servers.
pub struct PutBlobHandler;

impl MethodHandler for PutBlobHandler {
    fn handle_message<'a>(
        &self,
        request: &'a MethodRequest<'a>,
        response_tx: mpsc::Sender<String>,
        context: Arc<dyn Context>,
        _run_context: Option<&'a Arc<RunContext>>,
    ) -> BoxFuture<'a, error_stack::Result<(), TransportError>> {
        handle_method_call(
            request,
            response_tx,
            async move |request: crate::protocol::PutBlobParams| {
                let blob_id = context
                    .state_store()
                    .put_blob(request.data, request.blob_type)
                    .await
                    .map_err(|e| {
                        log::error!("Failed to put blob: {e}");
                        Error::internal("Failed to put blob")
                    })?;
                Ok(crate::protocol::PutBlobResult { blob_id })
            },
        )
        .boxed()
    }
}

/// Handler for get_blob method calls from component servers.
pub struct GetBlobHandler;

impl MethodHandler for GetBlobHandler {
    fn handle_message<'a>(
        &self,
        request: &'a MethodRequest<'a>,
        response_tx: mpsc::Sender<String>,
        context: Arc<dyn Context>,
        _run_context: Option<&'a Arc<RunContext>>,
    ) -> BoxFuture<'a, error_stack::Result<(), TransportError>> {
        handle_method_call(
            request,
            response_tx,
            async move |request: crate::protocol::GetBlobParams| {
                let blob_data = context
                    .state_store()
                    .get_blob(&request.blob_id)
                    .await
                    .map_err(|e| {
                        log::error!("Failed to get blob: {e}");
                        Error::internal("Failed to get blob")
                    })?;
                Ok(crate::protocol::GetBlobResult {
                    data: blob_data.data(),
                    blob_type: blob_data.blob_type(),
                })
            },
        )
        .boxed()
    }
}
