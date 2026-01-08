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
use stepflow_core::GetRunOptions;
use stepflow_plugin::{Context, RunContext};
use tokio::sync::mpsc;

use crate::error::TransportError;
use crate::{Error, MethodHandler, MethodRequest};

use super::handle_method_call;

/// Handler for run submission method calls from component servers.
pub struct SubmitRunHandler;

/// Handler for run retrieval method calls from component servers.
pub struct GetRunHandler;

impl MethodHandler for SubmitRunHandler {
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
            async move |request: crate::protocol::SubmitRunProtocolParams| {
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

                // Build submit params
                let mut params =
                    stepflow_core::SubmitRunParams::new(flow, request.flow_id, request.inputs);
                params = params.with_wait(request.wait);
                if let Some(max_concurrency) = request.max_concurrency {
                    params = params.with_max_concurrency(max_concurrency);
                }
                if let Some(overrides) = request.overrides {
                    params = params.with_overrides(overrides);
                }

                // Submit the run
                let run_status = context.submit_run(params).await.map_err(|e| {
                    log::error!("Failed to submit run: {e}");
                    Error::internal("Failed to submit run")
                })?;

                Ok(crate::protocol::RunStatusProtocol::from(run_status))
            },
        )
        .boxed()
    }
}

impl MethodHandler for GetRunHandler {
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
            async move |request: crate::protocol::GetRunProtocolParams| {
                let run_id = uuid::Uuid::parse_str(&request.run_id).map_err(|e| {
                    log::error!("Invalid run ID: {e}");
                    Error::invalid_value("run_id", "valid UUID")
                })?;

                let options = GetRunOptions::new()
                    .with_wait(request.wait)
                    .with_result_order(request.result_order);
                let options = if request.include_results {
                    options.with_results()
                } else {
                    options
                };

                let run_status = context.get_run(run_id, options).await.map_err(|e| {
                    log::error!("Failed to get run: {e}");
                    Error::internal("Failed to get run")
                })?;

                Ok(crate::protocol::RunStatusProtocol::from(run_status))
            },
        )
        .boxed()
    }
}
