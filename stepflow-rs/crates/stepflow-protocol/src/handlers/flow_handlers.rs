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
use stepflow_core::GetRunParams;
use stepflow_plugin::RunContext;
use stepflow_state::StateStoreExt as _;
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
        run_context: &'a Arc<RunContext>,
    ) -> BoxFuture<'a, error_stack::Result<(), TransportError>> {
        let env = run_context.env().clone();
        handle_method_call(
            request,
            response_tx,
            async move |request: crate::protocol::SubmitRunProtocolParams| {
                // Fetch the flow from the state store
                let flow_id = &request.flow_id;
                let blob_data = env.state_store().get_blob(flow_id).await.map_err(|e| {
                    log::error!("Failed to get flow blob: {e}");
                    Error::not_found("flow", flow_id.as_str())
                })?;
                let flow = blob_data
                    .as_flow()
                    .ok_or_else(|| Error::internal("Invalid flow blob"))?
                    .clone();

                // Build submit params
                let params = stepflow_core::SubmitRunParams {
                    max_concurrency: request.max_concurrency,
                    overrides: request.overrides.unwrap_or_default(),
                    ..Default::default()
                };

                // Submit the run using stepflow_execution
                let run_status = stepflow_execution::submit_run(
                    &env,
                    flow,
                    request.flow_id,
                    request.inputs,
                    params,
                )
                .await
                .map_err(|e| {
                    log::error!("Failed to submit run: {e}");
                    Error::internal("Failed to submit run")
                })?;

                // If wait=true, wait for completion and fetch results
                let run_status = if request.wait {
                    stepflow_execution::wait_for_completion(&env, run_status.run_id)
                        .await
                        .map_err(|e| {
                            log::error!("Failed to wait for run completion: {e}");
                            Error::internal("Failed to wait for run completion")
                        })?;

                    // Fetch final status with results

                    stepflow_execution::get_run(
                        &env,
                        run_status.run_id,
                        GetRunParams {
                            include_results: true,
                            ..Default::default()
                        },
                    )
                    .await
                    .map_err(|e| {
                        log::error!("Failed to get run after completion: {e}");
                        Error::internal("Failed to get run after completion")
                    })?
                } else {
                    run_status
                };

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
        run_context: &'a Arc<RunContext>,
    ) -> BoxFuture<'a, error_stack::Result<(), TransportError>> {
        let env = run_context.env().clone();
        handle_method_call(
            request,
            response_tx,
            async move |request: crate::protocol::GetRunProtocolParams| {
                let run_id = uuid::Uuid::parse_str(&request.run_id).map_err(|e| {
                    log::error!("Invalid run ID: {e}");
                    Error::invalid_value("run_id", "valid UUID")
                })?;

                // If wait=true, wait for completion first
                if request.wait {
                    stepflow_execution::wait_for_completion(&env, run_id)
                        .await
                        .map_err(|e| {
                            log::error!("Failed to wait for run completion: {e}");
                            Error::internal("Failed to wait for run completion")
                        })?;
                }

                let params = GetRunParams {
                    include_results: request.include_results,
                    result_order: request.result_order,
                };

                let run_status = stepflow_execution::get_run(&env, run_id, params)
                    .await
                    .map_err(|e| {
                        log::error!("Failed to get run: {e}");
                        Error::internal("Failed to get run")
                    })?;

                Ok(crate::protocol::RunStatusProtocol::from(run_status))
            },
        )
        .boxed()
    }
}
