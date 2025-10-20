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

use fastrace::prelude::*;
use futures::future::{BoxFuture, FutureExt as _};
use std::sync::Arc;
use stepflow_plugin::Context;
use tokio::sync::mpsc;

use crate::error::TransportError;
use crate::protocol::ObservabilityContext;
use crate::{Error, MethodHandler, MethodRequest};

use super::blob_handlers::handle_method_call;

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

/// Handler for flow evaluation method calls from component servers.
pub struct EvaluateFlowHandler;

/// Handler for flow metadata method calls from component servers.
pub struct GetFlowMetadataHandler;

/// Handler for batch submission method calls from component servers.
pub struct SubmitBatchHandler;

/// Handler for batch retrieval method calls from component servers.
pub struct GetBatchHandler;

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
            |request: crate::protocol::EvaluateFlowParams| {
                // Extract parent span context
                let parent_span_ctx = extract_parent_span(&request.observability);

                let fut = async move {
                    // Execute the flow using the shared utility
                    let result = context
                        .execute_flow_by_id(&request.flow_id, request.input)
                        .await
                        .map_err(|e| {
                            log::error!("Failed to evaluate flow: {e}");
                            Error::internal("Failed to evaluate flow")
                        })?;

                    Ok(crate::protocol::EvaluateFlowResult { result })
                };

                // Wrap future in span if parent context exists
                if let Some(parent_ctx) = parent_span_ctx {
                    fut.in_span(Span::root("flows/evaluate", parent_ctx))
                        .boxed()
                } else {
                    fut.boxed()
                }
            },
        )
        .boxed()
    }
}

impl MethodHandler for GetFlowMetadataHandler {
    fn handle_message<'a>(
        &self,
        request: &'a MethodRequest<'a>,
        response_tx: mpsc::Sender<String>,
        context: Arc<dyn Context>,
    ) -> BoxFuture<'a, error_stack::Result<(), TransportError>> {
        handle_method_call(
            request,
            response_tx,
            |request: crate::protocol::GetFlowMetadataParams| {
                // Extract parent span context
                let parent_span_ctx = extract_parent_span(&request.observability);

                let fut = async move {
                    // Fetch the flow from the state store
                    let flow_id = &request.flow_id;
                    let blob_data = context.state_store().get_blob(flow_id).await.map_err(|e| {
                        log::error!("Failed to get flow blob: {e}");
                        Error::not_found("flow", flow_id.as_str())
                    })?;
                    let flow = blob_data
                        .as_flow()
                        .ok_or_else(|| Error::internal("Invalid flow blob"))?
                        .clone();

                    let flow_metadata = flow.metadata().clone();

                    let step_metadata = if let Some(step_id) = request.step_id.as_ref() {
                        let Some(step) = flow.steps().iter().find(|s| &s.id == step_id) else {
                            return Err(Error::not_found("step", step_id.as_str()));
                        };
                        Some(step.metadata.clone())
                    } else {
                        None
                    };

                    Ok(crate::protocol::GetFlowMetadataResult {
                        flow_metadata,
                        step_metadata,
                    })
                };

                // Wrap future in span if parent context exists
                if let Some(parent_ctx) = parent_span_ctx {
                    fut.in_span(Span::root("flows/get_metadata", parent_ctx))
                        .boxed()
                } else {
                    fut.boxed()
                }
            },
        )
        .boxed()
    }
}

impl MethodHandler for SubmitBatchHandler {
    fn handle_message<'a>(
        &self,
        request: &'a MethodRequest<'a>,
        response_tx: mpsc::Sender<String>,
        context: Arc<dyn Context>,
    ) -> BoxFuture<'a, error_stack::Result<(), TransportError>> {
        handle_method_call(
            request,
            response_tx,
            |request: crate::protocol::SubmitBatchParams| {
                // Extract parent span context
                let parent_span_ctx = extract_parent_span(&request.observability);

                let fut = async move {
                    // Fetch the flow from the state store
                    let flow_id = &request.flow_id;
                    let blob_data = context.state_store().get_blob(flow_id).await.map_err(|e| {
                        log::error!("Failed to get flow blob: {e}");
                        Error::not_found("flow", flow_id.as_str())
                    })?;
                    let flow = blob_data
                        .as_flow()
                        .ok_or_else(|| Error::internal("Invalid flow blob"))?
                        .clone();

                    // Submit the batch
                    let batch_id = context
                        .submit_batch(
                            flow,
                            request.flow_id,
                            request.inputs,
                            request.max_concurrency,
                        )
                        .await
                        .map_err(|e| {
                            log::error!("Failed to submit batch: {e}");
                            Error::internal("Failed to submit batch")
                        })?;

                    let batch_metadata =
                        context
                            .state_store()
                            .get_batch(batch_id)
                            .await
                            .map_err(|e| {
                                log::error!("Failed to get batch metadata: {e}");
                                Error::internal("Failed to get batch metadata")
                            })?;

                    Ok(crate::protocol::SubmitBatchResult {
                        batch_id: batch_id.to_string(),
                        total_runs: batch_metadata.map(|m| m.total_inputs).unwrap_or(0),
                    })
                };

                // Wrap future in span if parent context exists
                if let Some(parent_ctx) = parent_span_ctx {
                    fut.in_span(Span::root("flows/submit_batch", parent_ctx))
                        .boxed()
                } else {
                    fut.boxed()
                }
            },
        )
        .boxed()
    }
}

impl MethodHandler for GetBatchHandler {
    fn handle_message<'a>(
        &self,
        request: &'a MethodRequest<'a>,
        response_tx: mpsc::Sender<String>,
        context: Arc<dyn Context>,
    ) -> BoxFuture<'a, error_stack::Result<(), TransportError>> {
        handle_method_call(
            request,
            response_tx,
            |request: crate::protocol::GetBatchParams| {
                // Extract parent span context
                let parent_span_ctx = extract_parent_span(&request.observability);

                let fut = async move {
                    let batch_id = uuid::Uuid::parse_str(&request.batch_id).map_err(|e| {
                        log::error!("Invalid batch ID: {e}");
                        Error::invalid_value("batch_id", "valid UUID")
                    })?;

                    // Get batch details and optionally outputs
                    let (state_details, state_outputs) = context
                        .get_batch(batch_id, request.wait, request.include_results)
                        .await
                        .map_err(|e| {
                            log::error!("Failed to get batch: {e}");
                            Error::internal("Failed to get batch")
                        })?;

                    // Convert state store types to protocol types
                    let details = crate::protocol::BatchDetails {
                        batch_id: state_details.metadata.batch_id.to_string(),
                        flow_id: state_details.metadata.flow_id,
                        flow_name: state_details.metadata.flow_name,
                        total_runs: state_details.metadata.total_inputs,
                        status: format!("{:?}", state_details.metadata.status).to_lowercase(),
                        created_at: state_details.metadata.created_at.to_rfc3339(),
                        completed_runs: state_details.statistics.completed_runs,
                        running_runs: state_details.statistics.running_runs,
                        failed_runs: state_details.statistics.failed_runs,
                        cancelled_runs: state_details.statistics.cancelled_runs,
                        paused_runs: state_details.statistics.paused_runs,
                        completed_at: state_details.completed_at.map(|dt| dt.to_rfc3339()),
                    };

                    let outputs = state_outputs.map(|outs| {
                        outs.into_iter()
                            .map(|out| crate::protocol::BatchOutputInfo {
                                batch_input_index: out.batch_input_index,
                                status: out.status.to_string(),
                                result: out.result,
                            })
                            .collect()
                    });

                    Ok(crate::protocol::GetBatchResult { details, outputs })
                };

                // Wrap future in span if parent context exists
                if let Some(parent_ctx) = parent_span_ctx {
                    fut.in_span(Span::root("flows/get_batch", parent_ctx))
                        .boxed()
                } else {
                    fut.boxed()
                }
            },
        )
        .boxed()
    }
}
