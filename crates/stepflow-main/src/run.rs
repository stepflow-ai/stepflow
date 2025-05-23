use crate::{MainError, Result};
use error_stack::ResultExt as _;
use stepflow_plugin::Plugins;
use stepflow_core::workflow::Flow;

pub async fn run(
    plugins: &Plugins,
    flow: Flow,
    input: stepflow_core::workflow::Value,
) -> Result<stepflow_core::workflow::Value> {
    let result = stepflow_execution::execute(plugins, &flow, input)
        .await
        .change_context(MainError::FlowExecution)?;

    Ok(result)
}
