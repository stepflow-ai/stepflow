mod error;
mod state;

pub use error::{ExecutionError, Result};
use error_stack::ResultExt;
use stepflow_workflow::{Flow, Step, Value};
use stepflow_steps::Plugins;
use state::VecState;

pub async fn execute(
    plugins: &Plugins,
    flow: &Flow,
    inputs: Vec<Value>,
) -> Result<Vec<Value>> {
    let mut state = VecState::try_new(flow)?;

    if inputs.len() > 0 {
        todo!("flow inputs not yet supported")
    }

    for step in flow.steps.iter() {
        execute_step(&mut state, step, plugins)
            .attach_printable_lazy(|| format!("error executing step {:?}", step))?;
    }

    let resolved_outputs = state.resolve(&flow.outputs)?;
    Ok(resolved_outputs)
}

fn execute_step(
    state: &mut VecState,
    step: &Step,
    plugins: &Plugins,
) -> Result<()> {
    let plugin = plugins.get_dyn(&step.component).change_context(ExecutionError::PluginNotFound)?;
    // TODO: Resolving should return references to values. Builtins and other things
    // likely should not create copies.
    let args = state.resolve(&step.args)?;

    let results = plugin.execute(&step.component, args).change_context(ExecutionError::PluginError)?;

    let step_execution = step.execution.as_ref().ok_or(ExecutionError::FlowNotCompiled)?;
    state.record_results(step_execution, results)?;

    Ok(())
}
