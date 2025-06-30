use std::sync::Arc;

use crate::{MainError, Result};
use error_stack::ResultExt as _;
use stepflow_core::{FlowResult, workflow::Flow, workflow::FlowHash};
use stepflow_execution::StepFlowExecutor;
use stepflow_plugin::Context as _;

pub async fn run(
    executor: Arc<StepFlowExecutor>,
    flow: Arc<Flow>,
    workflow_hash: FlowHash,
    input: stepflow_core::workflow::ValueRef,
) -> Result<FlowResult> {
    let run_id = executor
        .submit_flow(flow, workflow_hash, input)
        .await
        .change_context(MainError::FlowExecution)?;
    let output = executor
        .flow_result(run_id)
        .await
        .change_context(MainError::FlowExecution)?;
    Ok(output)
}
