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

use crate::{Result, error::MainError};
use error_stack::report;
use std::collections::HashMap;
use stepflow_core::workflow::{Flow, ValueRef, WorkflowOverrides};
use stepflow_core::{FlowError, FlowResult, TaskErrorCode};
use stepflow_proto::stepflow::v1::{
    self as proto, flows_service_client::FlowsServiceClient, runs_service_client::RunsServiceClient,
};

/// Display gRPC error status with context.
fn display_grpc_error(status: &tonic::Status, context: &str) {
    log::error!(
        "gRPC error {}: {} (code: {})",
        context,
        status.message(),
        status.code(),
    );
}

/// Display proto validation diagnostics and return the number of failures (fatal + error).
#[allow(clippy::print_stdout)]
fn display_proto_diagnostics(diagnostics: &proto::Diagnostics) -> u32 {
    let fatal_count = diagnostics.num_fatal;
    let error_count = diagnostics.num_error;
    let warning_count = diagnostics.num_warning;
    let failure_count = fatal_count + error_count;

    if diagnostics.diagnostics.is_empty() {
        println!("✅ Validation passed with no errors");
    } else {
        println!(
            "📊 Validation results: {fatal_count} fatal, {error_count} errors, {warning_count} warnings"
        );

        for d in &diagnostics.diagnostics {
            let level_str = match proto::DiagnosticLevel::try_from(d.level) {
                Ok(proto::DiagnosticLevel::Fatal) => "FATAL",
                Ok(proto::DiagnosticLevel::Error) => "ERROR",
                Ok(proto::DiagnosticLevel::Warning) => "WARN",
                _ => "UNKNOWN",
            };
            let path = d.path.as_deref().unwrap_or("");
            match proto::DiagnosticLevel::try_from(d.level) {
                Ok(proto::DiagnosticLevel::Fatal) if fatal_count > 0 => {
                    println!("  {level_str} {} ({path})", d.formatted);
                }
                Ok(proto::DiagnosticLevel::Error) if error_count > 0 => {
                    println!("  {level_str} {} ({path})", d.formatted);
                }
                Ok(proto::DiagnosticLevel::Warning) if warning_count > 0 => {
                    println!("  {level_str} {} ({path})", d.formatted);
                }
                _ => {}
            }
        }
    }

    failure_count
}

/// Convert a serializable value to a `prost_wkt_types::Struct`.
fn to_proto_struct<T: serde::Serialize>(value: &T) -> Result<prost_wkt_types::Struct> {
    let json = serde_json::to_value(value)
        .map_err(|e| report!(MainError::SerializationError).attach_printable(e.to_string()))?;
    serde_json::from_value(json)
        .map_err(|e| report!(MainError::SerializationError).attach_printable(e.to_string()))
}

/// Convert a serializable value to a `prost_wkt_types::Value`.
fn to_proto_value<T: serde::Serialize>(value: &T) -> Result<prost_wkt_types::Value> {
    let json = serde_json::to_value(value)
        .map_err(|e| report!(MainError::SerializationError).attach_printable(e.to_string()))?;
    serde_json::from_value(json)
        .map_err(|e| report!(MainError::SerializationError).attach_printable(e.to_string()))
}

/// Convert a proto `ItemResult` to a domain `FlowResult`.
fn proto_item_to_flow_result(item: &proto::ItemResult) -> FlowResult {
    if let Some(ref output) = item.output {
        let value: serde_json::Value = match serde_json::to_value(output) {
            Ok(v) => v,
            Err(e) => {
                return FlowResult::Failed(FlowError::new(
                    TaskErrorCode::OrchestratorError,
                    format!("Failed to deserialize output: {e}"),
                ));
            }
        };
        match serde_json::from_value(value) {
            Ok(value_ref) => FlowResult::Success(value_ref),
            Err(e) => FlowResult::Failed(FlowError::new(
                TaskErrorCode::OrchestratorError,
                format!("Failed to convert output to ValueRef: {e}"),
            )),
        }
    } else if let Some(ref error_msg) = item.error_message {
        let code = item
            .error_code
            .and_then(|c| TaskErrorCode::try_from(c).ok())
            .unwrap_or(TaskErrorCode::OrchestratorError);
        FlowResult::Failed(FlowError::new(code, error_msg.clone()))
    } else {
        FlowResult::Failed(FlowError::new(
            TaskErrorCode::OrchestratorError,
            "Server returned item with no output or error",
        ))
    }
}

/// Submit a workflow to a Stepflow service for execution via gRPC.
///
/// This function handles both single-item and multi-item (batch) submissions.
/// When a single input is provided, it returns a single result.
/// When multiple inputs are provided, it returns results for all items.
pub async fn submit(
    service_url: &str,
    flow: Flow,
    inputs: Vec<ValueRef>,
    overrides: &WorkflowOverrides,
    max_concurrency: Option<usize>,
    variables: HashMap<String, ValueRef>,
) -> Result<Vec<FlowResult>> {
    let channel = tonic::transport::Endpoint::from_shared(service_url.to_string())
        .map_err(|e| {
            log::error!("Invalid gRPC endpoint URL '{}': {}", service_url, e);
            report!(MainError::Configuration)
        })?
        .connect()
        .await
        .map_err(|e| {
            log::error!(
                "Failed to connect to gRPC server at '{}': {}",
                service_url,
                e
            );
            report!(MainError::Configuration)
        })?;

    // Step 1: Store the flow to get its hash
    let flow_struct = to_proto_struct(&flow)?;

    let store_request = proto::StoreFlowRequest {
        flow: Some(flow_struct),
        dry_run: false,
    };

    let mut flows_client = FlowsServiceClient::new(channel.clone());
    let store_response = flows_client
        .store_flow(store_request)
        .await
        .map_err(|status| {
            display_grpc_error(&status, "storing workflow");
            report!(MainError::ServerError)
        })?
        .into_inner();

    // Display validation results
    let failure_count = if let Some(ref diagnostics) = store_response.diagnostics {
        display_proto_diagnostics(diagnostics)
    } else {
        0
    };

    // Check if the workflow was stored successfully
    if !store_response.stored {
        if failure_count > 0 {
            log::error!("Workflow validation failed - see diagnostics above");
        } else {
            log::error!("Workflow was not stored (dry_run mode)");
        }
        return Err(MainError::ValidationError("Workflow not stored".to_string()).into());
    }

    let flow_id = store_response.flow_id;

    // Step 2: Execute the workflow — works for both single and batch
    let proto_inputs: Vec<prost_wkt_types::Value> = inputs
        .iter()
        .map(to_proto_value)
        .collect::<Result<Vec<_>>>()?;

    let proto_overrides = if overrides.is_empty() {
        None
    } else {
        Some(to_proto_struct(overrides)?)
    };

    let proto_variables: HashMap<String, prost_wkt_types::Value> = variables
        .iter()
        .map(|(k, v)| Ok((k.clone(), to_proto_value(v)?)))
        .collect::<Result<HashMap<_, _>>>()?;

    let create_request = proto::CreateRunRequest {
        flow_id,
        input: proto_inputs,
        overrides: proto_overrides,
        variables: proto_variables,
        max_concurrency: max_concurrency
            .map(|n| {
                u32::try_from(n).map_err(|_| {
                    MainError::InvalidArgument(format!(
                        "max_concurrency {n} exceeds maximum ({})",
                        u32::MAX
                    ))
                })
            })
            .transpose()?,
        wait: true,
        timeout_secs: None,
    };

    let mut runs_client = RunsServiceClient::new(channel);
    let create_response = runs_client
        .create_run(create_request)
        .await
        .map_err(|status| {
            display_grpc_error(&status, "executing workflow");
            report!(MainError::ServerError)
        })?
        .into_inner();

    // With wait=true, results are always populated in the response
    let items = create_response.results;
    if items.is_empty() {
        log::error!("No results in response");
        return Err(MainError::ServerError.into());
    }

    // Extract FlowResult from each proto ItemResult
    Ok(items.iter().map(proto_item_to_flow_result).collect())
}
