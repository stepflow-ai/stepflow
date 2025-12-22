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
pub struct InitializeParams {
    /// Maximum version of the protocol being used by the Stepflow runtime.
    pub runtime_protocol_version: u32,
    /// Observability context for tracing initialization (trace context only, no flow/run).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observability: Option<ObservabilityContext>,
}

/// Sent from the component server back to Stepflow with the result of initialization.
/// The component server will not be initialized until it receives the `initialized` notification.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct InitializeResult {
    /// Version of the protocol being used by the component server.
    pub server_protocol_version: u32,
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
