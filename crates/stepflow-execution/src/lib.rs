mod error;
mod state;

pub use error::{ExecutionError, Result};
use error_stack::ResultExt;
use state::VecState;
use stepflow_steps::{Plugins, StepPlugin as _};
use stepflow_workflow::{Flow, Step, Value};

pub async fn execute<'a>(
    plugins: &'a Plugins<'a>,
    flow: &Flow,
    inputs: Vec<Value>,
) -> Result<Vec<Value>> {
    let mut state = VecState::try_new(flow)?;

    if !inputs.is_empty() {
        todo!("flow inputs not yet supported")
    }

    for step in flow.steps.iter() {
        execute_step(&mut state, step, plugins)
            .await
            .attach_printable_lazy(|| format!("error executing step {:?}", step))?;
    }

    let resolved_outputs = state.resolve(&flow.outputs)?;
    Ok(resolved_outputs)
}

async fn execute_step<'a>(
    state: &mut VecState,
    step: &Step,
    plugins: &'a Plugins<'a>,
) -> Result<()> {
    let plugin = plugins
        .get(&step.component)
        .change_context(ExecutionError::PluginNotFound)?;
    // TODO: Resolving should return references to values. Builtins and other things
    // likely should not create copies.
    let args = state.resolve(&step.args)?;

    let results = plugin
        .execute(&step.component, args)
        .await
        .change_context(ExecutionError::PluginError)?;

    let step_execution = step
        .execution
        .as_ref()
        .ok_or(ExecutionError::FlowNotCompiled)?;
    state.record_results(step_execution, results)?;

    Ok(())
}
