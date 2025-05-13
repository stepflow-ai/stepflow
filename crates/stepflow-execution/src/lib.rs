mod error;
mod state;

pub use error::{ExecutionError, Result};
use error_stack::ResultExt;
use state::VecState;
use stepflow_plugin::{Plugin as _, Plugins};
use stepflow_workflow::{BaseRef, Flow, Step, Value};

pub async fn execute<'a>(
    plugins: &'a Plugins<'a>,
    flow: &Flow,
    inputs: Vec<Value>,
) -> Result<Value> {
    let mut state = VecState::new();

    if !inputs.is_empty() {
        todo!("flow inputs not yet supported")
    }

    for step in flow.steps.iter() {
        execute_step(&mut state, step, plugins)
            .await
            .attach_printable_lazy(|| format!("error executing step {:?}", step))?;
    }

    let output = state.resolve(flow.outputs())?;
    Ok(output)
}

async fn execute_step<'a>(
    state: &mut VecState,
    step: &Step,
    plugins: &'a Plugins<'a>,
) -> Result<()> {
    let plugin = plugins
        .get(&step.component)
        .change_context(ExecutionError::PluginNotFound)?;
    let input = state.resolve(&step.args)?;

    let results = plugin
        .execute(&step.component, input)
        .await
        .change_context(ExecutionError::PluginError)?;

    state.record_value(
        BaseRef::Step {
            step: step.id.clone(),
        },
        results,
    )?;

    Ok(())
}
