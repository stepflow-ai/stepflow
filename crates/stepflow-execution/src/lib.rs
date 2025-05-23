mod error;
mod state;

use std::sync::Arc;

pub use error::{ExecutionError, Result};
use error_stack::ResultExt;
use futures::{StreamExt as _, stream::FuturesUnordered};
use state::State;
use stepflow_core::workflow::{BaseRef, Component, Flow, Value};
use stepflow_plugin::{DynPlugin, Plugin as _, Plugins};
use tokio::sync::oneshot;
use tracing::Instrument as _;

pub async fn execute(plugins: &Plugins, flow: &Flow, input: Value) -> Result<Value> {
    tracing::info!("Creating flow futures");
    let mut state = State::new();

    tracing::debug!("Recording input {input:?}");
    state.record_literal(BaseRef::Input, input)?;

    let mut step_tasks = FuturesUnordered::new();
    for step in flow.steps.iter() {
        tracing::debug!("Creating future for step {step:?}");
        // Record the future step result.
        let result_tx = state.record_future(BaseRef::Step(step.id.clone()))?;

        let plugin = plugins
            .get(&step.component)
            .change_context(ExecutionError::PluginNotFound)?;

        let input = state.resolve(&step.args)?;
        // TODO: Eliminate cloning? Arc the component?
        let step_span = tracing::info_span!("step", id = step.id);
        let step =
            execute_step(plugin, step.component.clone(), input, result_tx).instrument(step_span);
        let step_task = tokio::spawn(step);
        step_tasks.push(step_task);
    }

    tracing::info!("Resolving output");
    loop {
        tokio::select! {
            output_result = state.resolve(flow.outputs())? => {
                match output_result {
                    Ok(output) => return Ok(output),
                    Err(e) => {
                        if e.current_context() == &ExecutionError::RecvInput {
                        // Look at the step tasks to see if we can find a more interesting error.
                            while let Some(step_result) = step_tasks.next().await {
                                match step_result {
                                    Ok(Err(e)) if e.current_context() != &ExecutionError::RecvInput => {
                                        return Err(e)
                                    }
                                    _ => {}
                                }
                            }
                        }
                        return Err(e);
                    }
                }
            }
            Some(step_result) = step_tasks.next() => {
                match step_result {
                    Ok(Ok(())) => {
                        // nothing to do -- step completed
                        continue
                    }
                    Ok(Err(e)) => {
                        tracing::error!(?e, "Error during step execution.");
                        return Err(e);
                    }
                    Err(e) => {
                        tracing::error!(?e, "Panic during step execution.");
                        error_stack::bail!(ExecutionError::StepPanic);
                    }
                }
            }
        }
    }
}

async fn execute_step(
    plugin: Arc<DynPlugin<'static>>,
    component: Component,
    input: impl Future<Output = Result<Value>>,
    result_tx: oneshot::Sender<Value>,
) -> Result<()> {
    tracing::info!("Waiting for inputs.");
    let input = input.await?;

    tracing::info!("Executing step {component:?}");
    let result = plugin
        .execute(&component, input)
        .await
        .change_context(ExecutionError::PluginError)?;

    tracing::info!("Sending result {result:?}");
    result_tx
        .send(result)
        .map_err(|_| ExecutionError::RecordResult)?;

    Ok(())
}
