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

use std::sync::Arc;

use crate::{MainError, Result};
use error_stack::ResultExt as _;
use stepflow_core::{
    BlobId, FlowResult,
    workflow::{Flow, WorkflowOverrides},
};
use stepflow_execution::{StepflowExecutor, execute_workflow_with_overrides};
use stepflow_plugin::Context as _;

pub async fn run(
    executor: Arc<StepflowExecutor>,
    flow: Arc<Flow>,
    flow_id: BlobId,
    input: stepflow_core::workflow::ValueRef,
) -> Result<(uuid::Uuid, FlowResult)> {
    let run_id = executor
        .submit_flow(flow, flow_id, input, None)
        .await
        .change_context(MainError::FlowExecution)?;
    let output = executor
        .flow_result(run_id)
        .await
        .change_context(MainError::FlowExecution)?;
    Ok((run_id, output))
}

pub async fn run_with_overrides(
    executor: Arc<StepflowExecutor>,
    flow: Arc<Flow>,
    flow_id: BlobId,
    input: stepflow_core::workflow::ValueRef,
    overrides: WorkflowOverrides,
) -> Result<(uuid::Uuid, FlowResult)> {
    // For now, we'll use the executor's direct execution capability with overrides.
    // The StepflowExecutor doesn't currently support overrides via submit_flow,
    // so we'll use the lower-level execution API directly.

    let run_id = uuid::Uuid::new_v4();

    // Execute the workflow directly with overrides using the workflow executor

    let output = execute_workflow_with_overrides(
        executor.executor(),
        flow,
        flow_id,
        run_id,
        input,
        executor.state_store().clone(),
        overrides,
    )
    .await
    .change_context(MainError::FlowExecution)?;

    Ok((run_id, output))
}
