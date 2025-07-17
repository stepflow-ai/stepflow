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
use stepflow_core::FlowResult;
use stepflow_core::workflow::{Flow, ValueRef};

use crate::protocol::Method;

use super::ProtocolMethod;

/// Sent from the component server to the StepFlow to evaluate a flow with the provided input.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct EvaluateFlowParams {
    /// The flow to evaluate.
    pub flow: Flow,
    /// The input to provide to the flow.
    pub input: ValueRef,
}

/// Sent from the StepFlow back to the component server with the result of the flow evaluation.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct EvaluateFlowResult {
    /// The result of the flow evaluation.
    pub result: FlowResult,
}

impl ProtocolMethod for EvaluateFlowParams {
    const METHOD_NAME: Method = Method::FlowsEvaluate;
    type Response = EvaluateFlowResult;
}
