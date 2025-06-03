use std::sync::Arc;

use crate::{MainError, Result};
use error_stack::ResultExt as _;
use stepflow_core::{FlowResult, workflow::Flow};
use stepflow_execution::StepFlowExecutor;
use stepflow_plugin::Context as _;

pub async fn run(
    executor: Arc<StepFlowExecutor>,
    flow: Arc<Flow>,
    input: stepflow_core::workflow::ValueRef,
) -> Result<FlowResult> {
    let execution_id = executor
        .submit_flow(flow, input)
        .await
        .change_context(MainError::FlowExecution)?;
    let output = executor
        .flow_result(execution_id)
        .await
        .change_context(MainError::FlowExecution)?;
    Ok(output)
}
