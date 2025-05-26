use std::sync::Arc;

use crate::{MainError, Result};
use error_stack::ResultExt as _;
use stepflow_core::{FlowResult, workflow::Flow};
use stepflow_plugin::Plugins;

pub async fn run(
    plugins: &Plugins,
    flow: Flow,
    input: stepflow_core::workflow::ValueRef,
) -> Result<FlowResult> {
    let result = stepflow_execution::execute(plugins, Arc::new(flow), input)
        .await
        .change_context(MainError::FlowExecution)?;

    Ok(result)
}
