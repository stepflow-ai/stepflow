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

use error_stack::ResultExt as _;
use fastrace::prelude::*;
use futures::future::{BoxFuture, FutureExt as _};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_plugin::Context;
use tokio::sync::mpsc;

use crate::error::{Result, TransportError};
use crate::lazy_value::LazyValue;
use crate::protocol::ObservabilityContext;
use crate::{Error, MethodHandler, MethodResponse};
use crate::{Message, MethodRequest};

/// Extract parent span context from observability context.
fn extract_parent_span(observability: &Option<ObservabilityContext>) -> Option<SpanContext> {
    observability.as_ref().and_then(|obs| {
        if let (Some(trace_id), Some(span_id)) = (&obs.trace_id, &obs.span_id) {
            // Parse hex strings to u128 and u64
            let trace_id_int = u128::from_str_radix(trace_id, 16).ok()?;
            let span_id_int = u64::from_str_radix(span_id, 16).ok()?;
            Some(SpanContext::new(TraceId(trace_id_int), SpanId(span_id_int)))
        } else {
            None
        }
    })
}

/// Helper function to handle a method call with request/response types.
pub(super) async fn handle_method_call<'a, Req, Resp, F, Fut>(
    request: &'a MethodRequest<'a>,
    response_tx: mpsc::Sender<String>,
    handler: F,
) -> Result<()>
where
    Req: for<'de> Deserialize<'de>,
    Resp: Serialize + std::fmt::Debug + Sync + Send,
    F: FnOnce(Req) -> Fut,
    Fut: Future<Output = Result<Resp, Error<'static>>> + 'a,
{
    let id = request.id.clone();
    let report_user_error = async |error| {
        let response = Message::Response(MethodResponse::error(id.clone(), error));
        let response = serde_json::to_string(&response).change_context(TransportError::Send)?;

        response_tx
            .send(response)
            .await
            .change_context(TransportError::Send)?;
        Ok(())
    };

    let request: Req = if let Some(params) = &request.params {
        match params.deserialize_to() {
            Ok(request) => request,
            Err(e) => {
                log::error!(
                    "Failed to deserialize request parameters for {}: {e:#}",
                    request.method
                );
                return report_user_error(Error::invalid_parameters(&request.id)).await;
            }
        }
    } else {
        match serde_json::from_value(serde_json::Value::Null) {
            Ok(request) => request,
            Err(e) => {
                log::error!("Failed to deserialize empty request parameters: {e}");
                return report_user_error(Error::invalid_parameters(&request.id)).await;
            }
        }
    };

    let response = match handler(request).await {
        Ok(result) => result,
        Err(e) => {
            log::error!("Method call failed: {e:?}");
            return report_user_error(e).await;
        }
    };
    let response = Message::Response(MethodResponse::success(id, LazyValue::write_ref(&response)));
    let response = serde_json::to_string(&response).change_context(TransportError::Send)?;

    response_tx
        .send(response)
        .await
        .change_context(TransportError::Send)?;

    Ok(())
}

/// Handler for put_blob method calls from component servers.
pub struct PutBlobHandler;

impl MethodHandler for PutBlobHandler {
    fn handle_message<'a>(
        &self,
        request: &'a MethodRequest<'a>,
        response_tx: mpsc::Sender<String>,
        context: Arc<dyn Context>,
    ) -> BoxFuture<'a, error_stack::Result<(), TransportError>> {
        handle_method_call(
            request,
            response_tx,
            |request: crate::protocol::PutBlobParams| {
                // Extract parent span context
                let parent_span_ctx = extract_parent_span(&request.observability);

                let fut = async move {
                    let blob_id = context
                        .state_store()
                        .put_blob(request.data, request.blob_type)
                        .await
                        .map_err(|e| {
                            log::error!("Failed to put blob: {e}");
                            Error::internal("Failed to put blob")
                        })?;
                    Ok(crate::protocol::PutBlobResult { blob_id })
                };

                // Wrap future in span if parent context exists
                if let Some(parent_ctx) = parent_span_ctx {
                    fut.in_span(Span::root("blobs/put", parent_ctx)).boxed()
                } else {
                    fut.boxed()
                }
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
    ) -> BoxFuture<'a, error_stack::Result<(), TransportError>> {
        handle_method_call(
            request,
            response_tx,
            |request: crate::protocol::GetBlobParams| {
                // Extract parent span context
                let parent_span_ctx = extract_parent_span(&request.observability);

                let fut = async move {
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
                };

                // Wrap future in span if parent context exists
                if let Some(parent_ctx) = parent_span_ctx {
                    fut.in_span(Span::root("blobs/get", parent_ctx)).boxed()
                } else {
                    fut.boxed()
                }
            },
        )
        .boxed()
    }
}
