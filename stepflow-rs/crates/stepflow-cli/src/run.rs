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
use stepflow_core::{BlobId, FlowResult, workflow::Flow};
use stepflow_plugin::StepflowEnvironment;

pub async fn run(
    env: Arc<StepflowEnvironment>,
    flow: Arc<Flow>,
    flow_id: BlobId,
    input: stepflow_core::workflow::ValueRef,
    overrides: Option<stepflow_core::workflow::WorkflowOverrides>,
) -> Result<(uuid::Uuid, FlowResult)> {
    let params = stepflow_core::SubmitRunParams {
        overrides: overrides.unwrap_or_default(),
        ..Default::default()
    };

    let run_status = stepflow_execution::submit_run(&env, flow, flow_id, vec![input], params)
        .await
        .change_context(MainError::FlowExecution)?;

    let run_id = run_status.run_id;

    // Wait for completion and fetch results
    stepflow_execution::wait_for_completion(&env, run_id)
        .await
        .change_context(MainError::FlowExecution)?;
    let run_status = stepflow_execution::get_run(
        &env,
        run_id,
        stepflow_core::GetRunParams {
            include_results: true,
            ..Default::default()
        },
    )
    .await
    .change_context(MainError::FlowExecution)?;

    // Extract result from run status
    let output = run_status
        .results
        .and_then(|r| r.into_iter().next())
        .and_then(|item| item.result)
        .ok_or_else(|| {
            error_stack::report!(MainError::FlowExecution).attach_printable("No result available")
        })?;

    Ok((run_id, output))
}
