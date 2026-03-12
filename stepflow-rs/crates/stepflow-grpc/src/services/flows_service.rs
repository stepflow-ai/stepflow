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

use std::collections::HashMap;
use std::sync::Arc;

use stepflow_core::workflow::Flow;
use stepflow_core::{BlobId, BlobType};
use stepflow_plugin::StepflowEnvironment;
use stepflow_state::BlobStoreExt as _;
use tonic::{Request, Response, Status};

use crate::error as grpc_err;
use crate::proto::stepflow::v1::{Diagnostic, DiagnosticLevel, Diagnostics, ExampleInput};

use crate::proto::stepflow::v1::flows_service_server::FlowsService;
use crate::proto::stepflow::v1::{
    DeleteFlowRequest, DeleteFlowResponse, FlowVariable, GetFlowRequest, GetFlowResponse,
    GetFlowVariablesRequest, GetFlowVariablesResponse, StoreFlowRequest, StoreFlowResponse,
};

/// gRPC implementation of [FlowsService].
#[derive(Debug)]
pub struct FlowsServiceImpl {
    env: Arc<StepflowEnvironment>,
}

impl FlowsServiceImpl {
    pub fn new(env: Arc<StepflowEnvironment>) -> Self {
        Self { env }
    }
}

#[tonic::async_trait]
impl FlowsService for FlowsServiceImpl {
    async fn store_flow(
        &self,
        request: Request<StoreFlowRequest>,
    ) -> Result<Response<StoreFlowResponse>, Status> {
        let req = request.into_inner();

        let flow_struct = req
            .flow
            .ok_or_else(|| grpc_err::invalid_field("flow", "flow is required"))?;

        // Convert protobuf Struct to serde_json::Value, then deserialize to Flow
        let flow_json: serde_json::Value = serde_json::to_value(&flow_struct)
            .map_err(|e| grpc_err::internal(format!("failed to convert flow: {e}")))?;

        let flow: Flow = serde_json::from_value(flow_json).map_err(|e| {
            grpc_err::invalid_field("flow", format!("invalid flow definition: {e}"))
        })?;

        let flow = Arc::new(flow);

        // Validate the workflow
        let analysis_diagnostics = stepflow_analysis::validate(&flow)
            .map_err(|e| grpc_err::invalid_field("flow", format!("flow validation failed: {e}")))?;

        let proto_diagnostics = diagnostics_to_proto(&analysis_diagnostics);

        // Compute content-based ID
        let flow_id = BlobId::from_flow(&flow)
            .map_err(|e| grpc_err::internal(format!("failed to compute flow ID: {e}")))?;

        // Only store if no fatal diagnostics and not dry_run
        let stored = if !analysis_diagnostics.has_fatal() && !req.dry_run {
            let content = serde_json::to_vec(flow.as_ref())
                .map_err(|e| grpc_err::internal(format!("failed to serialize flow: {e}")))?;

            self.env
                .blob_store()
                .put_blob(&content, BlobType::Flow, Default::default())
                .await
                .map_err(|e| grpc_err::internal(format!("failed to store flow: {e}")))?;
            true
        } else {
            false
        };

        Ok(Response::new(StoreFlowResponse {
            flow_id: flow_id.to_string(),
            stored,
            diagnostics: Some(proto_diagnostics),
        }))
    }

    async fn get_flow(
        &self,
        request: Request<GetFlowRequest>,
    ) -> Result<Response<GetFlowResponse>, Status> {
        let req = request.into_inner();

        let blob_id = BlobId::new(req.flow_id.clone())
            .map_err(|_| grpc_err::invalid_field("flow_id", "invalid flow_id format"))?;

        let raw = self
            .env
            .blob_store()
            .get_blob(&blob_id)
            .await
            .map_err(|e| grpc_err::internal(format!("failed to retrieve flow: {e}")))?
            .ok_or_else(|| grpc_err::not_found("flow", &req.flow_id))?;

        if raw.blob_type != BlobType::Flow {
            return Err(grpc_err::invalid_argument("blob is not a flow"));
        }

        let flow: Flow = serde_json::from_slice(&raw.content)
            .map_err(|e| grpc_err::internal(format!("failed to deserialize flow: {e}")))?;

        let flow_struct: prost_wkt_types::Struct = serde_json::to_value(&flow)
            .and_then(serde_json::from_value)
            .map_err(|e| grpc_err::internal(format!("failed to convert flow to proto: {e}")))?;

        let all_examples = flow
            .get_all_examples()
            .into_iter()
            .map(|ex| {
                let input = serde_json::to_value(&ex.input)
                    .ok()
                    .and_then(|v| serde_json::from_value(v).ok());
                ExampleInput {
                    name: ex.name,
                    description: ex.description,
                    input,
                }
            })
            .collect();

        Ok(Response::new(GetFlowResponse {
            flow: Some(flow_struct),
            flow_id: req.flow_id,
            all_examples,
        }))
    }

    async fn delete_flow(
        &self,
        _request: Request<DeleteFlowRequest>,
    ) -> Result<Response<DeleteFlowResponse>, Status> {
        // Flows are content-addressed blobs — deletion is not yet implemented.
        // This matches the aide REST handler which also returns success as a no-op.
        Ok(Response::new(DeleteFlowResponse {}))
    }

    async fn get_flow_variables(
        &self,
        request: Request<GetFlowVariablesRequest>,
    ) -> Result<Response<GetFlowVariablesResponse>, Status> {
        let req = request.into_inner();

        let blob_id = BlobId::new(req.flow_id.clone())
            .map_err(|_| grpc_err::invalid_field("flow_id", "invalid flow_id format"))?;

        let raw = self
            .env
            .blob_store()
            .get_blob(&blob_id)
            .await
            .map_err(|e| grpc_err::internal(format!("failed to retrieve flow: {e}")))?
            .ok_or_else(|| grpc_err::not_found("flow", &req.flow_id))?;

        if raw.blob_type != BlobType::Flow {
            return Err(grpc_err::invalid_argument("blob is not a flow"));
        }

        let flow: Flow = serde_json::from_slice(&raw.content)
            .map_err(|e| grpc_err::internal(format!("failed to deserialize flow: {e}")))?;

        let variable_schema = flow.variables();
        let variables: HashMap<String, FlowVariable> = if let Some(ref var_schema) = variable_schema
        {
            var_schema
                .variables()
                .iter()
                .map(|name| {
                    let default_value = var_schema.default_value(name).and_then(|d| {
                        serde_json::to_value(d)
                            .ok()
                            .and_then(|v| serde_json::from_value(v).ok())
                    });
                    let required = var_schema.required_variables().any(|r| r == name);
                    let env_var = var_schema.env_var_map().get(name.as_str()).cloned();
                    // Per-variable schema extraction is not yet
                    // supported by the domain type; populate from the
                    // overall variables JSON Schema when available.
                    let schema: Option<prost_wkt_types::Struct> = None;
                    (
                        name.clone(),
                        FlowVariable {
                            description: None,
                            default_value,
                            required,
                            schema,
                            env_var,
                        },
                    )
                })
                .collect()
        } else {
            HashMap::new()
        };

        Ok(Response::new(GetFlowVariablesResponse {
            flow_id: req.flow_id,
            variables,
        }))
    }
}

/// Convert analysis diagnostics to proto Diagnostics.
fn diagnostics_to_proto(diags: &stepflow_analysis::Diagnostics) -> Diagnostics {
    Diagnostics {
        diagnostics: diags
            .diagnostics
            .iter()
            .map(|d| {
                let data = if d.data.is_null() {
                    None
                } else {
                    serde_json::to_value(&d.data)
                        .ok()
                        .and_then(|v| serde_json::from_value(v).ok())
                };
                let path = {
                    let s = d.path.to_string();
                    if s.is_empty() { None } else { Some(s) }
                };
                Diagnostic {
                    kind: d.kind.to_string(),
                    code: d.code as u32,
                    level: diagnostic_level_to_proto(d.level).into(),
                    formatted: d.formatted.clone(),
                    data,
                    path,
                    experimental: d.experimental,
                }
            })
            .collect(),
        num_fatal: diags.num_fatal,
        num_error: diags.num_error,
        num_warning: diags.num_warning,
    }
}

/// Convert analysis DiagnosticLevel to proto DiagnosticLevel.
fn diagnostic_level_to_proto(level: stepflow_analysis::DiagnosticLevel) -> DiagnosticLevel {
    match level {
        stepflow_analysis::DiagnosticLevel::Fatal => DiagnosticLevel::Fatal,
        stepflow_analysis::DiagnosticLevel::Error => DiagnosticLevel::Error,
        stepflow_analysis::DiagnosticLevel::Warning => DiagnosticLevel::Warning,
    }
}
