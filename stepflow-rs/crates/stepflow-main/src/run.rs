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

use std::sync::Arc;

use crate::{MainError, Result};
use error_stack::ResultExt as _;
use stepflow_core::{BlobId, FlowResult, workflow::Flow};
use stepflow_execution::StepflowExecutor;
use stepflow_plugin::Context as _;

pub async fn run(
    executor: Arc<StepflowExecutor>,
    flow: Arc<Flow>,
    flow_id: BlobId,
    input: stepflow_core::workflow::ValueRef,
) -> Result<FlowResult> {
    let run_id = executor
        .submit_flow(flow, flow_id, input)
        .await
        .change_context(MainError::FlowExecution)?;
    let output = executor
        .flow_result(run_id)
        .await
        .change_context(MainError::FlowExecution)?;
    Ok(output)
}
