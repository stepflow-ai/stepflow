// Licensed to the Apache Software Foundation (ASF) under one or more contributor license agreements.
// See the NOTICE file distributed with this work for additional information regarding copyright
// ownership.  The ASF licenses this file to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance with the License.  You may obtain a
// copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied.  See the License for the specific language governing permissions and limitations under
// the License.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::protocol::Method;

use super::{ProtocolMethod, ProtocolNotification};

/// Sent from StepFlow to the component server to begin the initialization process.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct InitializeParams {
    /// Maximum version of the protocol being used by the StepFlow runtime.
    pub runtime_protocol_version: u32,
}

/// Sent from the component server back to StepFlow with the result of initialization.
/// The component server will not be initialized until it receives the `initialized` notification.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct InitializeResult {
    /// Version of the protocol being used by the component server.
    pub server_protocol_version: u32,
}

impl ProtocolMethod for InitializeParams {
    const METHOD_NAME: Method = Method::Initialize;
    type Response = InitializeResult;
}

/// Sent from StepFlow to the component server after initialization is complete.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct Initialized {}

impl ProtocolNotification for Initialized {
    const METHOD_NAME: Method = Method::Initialized;
}
