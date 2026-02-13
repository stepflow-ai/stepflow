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
use utoipa::ToSchema;

use crate::protocol::Method;

use super::{ObservabilityContext, ProtocolMethod, ProtocolNotification};

/// Sent from Stepflow to the component server to begin the initialization process.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    /// Maximum version of the protocol being used by the Stepflow runtime.
    pub runtime_protocol_version: u32,
    /// Observability context for tracing initialization (trace context only, no flow/run).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observability: Option<ObservabilityContext>,
    /// Runtime capabilities provided to the component server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<RuntimeCapabilities>,
}

/// Runtime capabilities advertised by the Stepflow runtime during initialization.
///
/// Component servers can use these capabilities to access runtime services.
#[derive(Debug, Default, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeCapabilities {
    /// Base URL for the Blob HTTP API.
    ///
    /// When provided, component servers should use direct HTTP requests
    /// (`GET {blob_api_url}/{blob_id}`, `POST {blob_api_url}`) for blob operations
    /// instead of SSE bidirectional protocol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob_api_url: Option<String>,

    /// Byte size threshold for automatic blobification.
    ///
    /// When set to a non-zero value, the orchestrator may replace large input fields
    /// with `$blob` references. Component servers that report `supports_blob_refs`
    /// should resolve these references before processing. Component servers should
    /// also blobify output fields exceeding this threshold.
    ///
    /// A value of 0 or `None` means automatic blobification is disabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<i64>)]
    pub blob_threshold: Option<usize>,
}

/// Sent from the component server back to Stepflow with the result of initialization.
/// The component server will not be initialized until it receives the `initialized` notification.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    /// Version of the protocol being used by the component server.
    pub server_protocol_version: u32,

    /// Whether this component server supports `$blob` references in inputs/outputs.
    ///
    /// When `true`, the orchestrator may send `$blob` references in component inputs
    /// and expects the server to resolve them. The server may also return `$blob`
    /// references in outputs for the orchestrator to resolve.
    ///
    /// When `false` (default), the orchestrator will not send blob refs and will
    /// resolve any refs before delivering input to this server.
    #[serde(default, skip_serializing_if = "is_false")]
    pub supports_blob_refs: bool,
}

fn is_false(v: &bool) -> bool {
    !v
}

impl ProtocolMethod for InitializeParams {
    const METHOD_NAME: Method = Method::Initialize;
    type Response = InitializeResult;
}

/// Sent from Stepflow to the component server after initialization is complete.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Initialized {}

impl ProtocolNotification for Initialized {
    const METHOD_NAME: Method = Method::Initialized;
}
