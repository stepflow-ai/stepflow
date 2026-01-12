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

use serde::{Deserialize, Serialize};
use stepflow_core::workflow::{ValueRef, WorkflowOverrides};
use stepflow_core::{BlobId, ResultOrder};
use stepflow_dtos::{ItemResult, ItemStatistics, RunStatus};
use utoipa::ToSchema;

use crate::protocol::Method;

use super::{ObservabilityContext, ProtocolMethod};

/// Parameters for submitting a run via the protocol.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubmitRunProtocolParams {
    /// The ID of the flow to execute (blob ID).
    pub flow_id: BlobId,
    /// Input values for each item in the run.
    pub inputs: Vec<ValueRef>,
    /// If true, wait for completion before returning.
    #[serde(default)]
    pub wait: bool,
    /// Maximum number of concurrent executions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrency: Option<usize>,
    /// Optional workflow overrides to apply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overrides: Option<WorkflowOverrides>,
    /// Observability context for tracing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observability: Option<ObservabilityContext>,
}

impl ProtocolMethod for SubmitRunProtocolParams {
    const METHOD_NAME: Method = Method::RunsSubmit;
    type Response = RunStatusProtocol;
}

/// Parameters for getting a run via the protocol.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetRunProtocolParams {
    /// The run ID to query.
    pub run_id: String,
    /// If true, wait for run completion before returning.
    #[serde(default)]
    pub wait: bool,
    /// If true, include item results in the response.
    #[serde(default)]
    pub include_results: bool,
    /// Order of results (byIndex or byCompletion).
    #[serde(default)]
    pub result_order: ResultOrder,
    /// Observability context for tracing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observability: Option<ObservabilityContext>,
}

impl ProtocolMethod for GetRunProtocolParams {
    const METHOD_NAME: Method = Method::RunsGet;
    type Response = RunStatusProtocol;
}

/// Run status returned by the protocol (serializable version of RunStatus).
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunStatusProtocol {
    pub run_id: String,
    pub flow_id: BlobId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow_name: Option<String>,
    pub status: String,
    pub items: ItemStatistics,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<Vec<ItemResult>>,
}

impl From<RunStatus> for RunStatusProtocol {
    fn from(status: RunStatus) -> Self {
        Self {
            run_id: status.run_id.to_string(),
            flow_id: status.flow_id,
            flow_name: status.flow_name,
            status: format!("{:?}", status.status).to_lowercase(),
            items: status.items,
            created_at: status.created_at.to_rfc3339(),
            completed_at: status.completed_at.map(|dt| dt.to_rfc3339()),
            results: status.results,
        }
    }
}
