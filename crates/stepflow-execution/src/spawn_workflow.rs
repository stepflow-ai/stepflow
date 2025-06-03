use std::sync::Arc;

use crate::state::State;
use crate::{ExecutionError, Result};
use error_stack::ResultExt as _;
use futures::{StreamExt as _, stream::FuturesUnordered};
use owning_ref::ArcRef;
use stepflow_core::{
    FlowResult,
    workflow::{BaseRef, ErrorAction, Flow, Step, ValueRef},
};
use stepflow_plugin::{DynPlugin, ExecutionContext, Plugin as _};
use tokio::sync::oneshot;
use tracing::Instrument as _;
use uuid::Uuid;

pub(crate) async fn spawn_workflow(
    executor: Arc<crate::executor::StepFlowExecutor>,
    execution_id: Uuid,
    flow: Arc<Flow>,
    input: ValueRef,
) -> Result<FlowResult> {
    tracing::info!("Creating flow futures");
    let mut state = State::new();

    tracing::debug!("Recording input {input:?}");
    state.record_literal(BaseRef::WORKFLOW_INPUT, input)?;

    let mut step_tasks = FuturesUnordered::new();

    let context = executor.execution_context(execution_id);

    let steps = (0..flow.steps.len()).map(|i| ArcRef::new(flow.clone()).map(|flow| flow.step(i)));
    for step in steps {
        tracing::debug!("Creating future for step {:?}", step.id);
        // Record the future step result.
        let result_tx = state.record_future(BaseRef::step_output(step.id.clone()))?;

        let plugin = executor.get_plugin(&step.component).await?;

        let skip_if = if let Some(skip_if) = &step.skip_if {
            Some(state.resolve_expr(skip_if)?)
        } else {
            None
        };

        let input = state.resolve_object(&step.input)?;
        // TODO: Eliminate cloning? Arc the component?
        let step_span = tracing::info_span!("step", id = step.id);

        let step = execute_step(plugin, step, skip_if, input, result_tx, context.clone())
            .instrument(step_span);
        let step_task = tokio::spawn(step);
        step_tasks.push(step_task);
    }

    tracing::info!("Resolving output");
    loop {
        tokio::select! {
            output_result = state.resolve_object(flow.output())? => {
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

fn send_skip(result_tx: oneshot::Sender<FlowResult>) -> Result<()> {
    result_tx
        .send(FlowResult::Skipped)
        .map_err(|_| ExecutionError::RecordResult)?;
    Ok(())
}

async fn execute_step(
    plugin: Arc<DynPlugin<'static>>,
    step: ArcRef<'static, Flow, Step>,
    skip_if: Option<impl Future<Output = Result<FlowResult>>>,
    input: impl Future<Output = Result<FlowResult>>,
    result_tx: oneshot::Sender<FlowResult>,
    context: ExecutionContext,
) -> Result<()> {
    if let Some(skip) = skip_if {
        // TODO: Should we wait on skip and input in parallel?
        match skip.await? {
            FlowResult::Success { result } if result.is_truthy() => {
                tracing::info!("Skipping step {} due to condition", step.id);
                return send_skip(result_tx);
            }
            FlowResult::Success { .. } => {
                // Nothing to do here. We need to run the step.
            }
            FlowResult::Skipped => {
                tracing::info!("Skipping step {} due to skipped condition", step.id);
                return send_skip(result_tx);
            }
            FlowResult::Failed { .. } => {
                // Failed steps should terminate the workflow.
                // We don't return an error since that should already be happening.
                return Ok(());
            }
        }
    }

    tracing::info!("Waiting for inputs.");
    let input = match input.await? {
        FlowResult::Success { result } => result,
        FlowResult::Skipped => {
            // This indicates a required input was skipped, so we
            // we propagate the skipped state.
            return send_skip(result_tx);
        }
        FlowResult::Failed { .. } => {
            // Failed inputs should terminate the workflow.
            // We don't return an error here since that should already be happening.
            return Ok(());
        }
    };

    let component = &step.component;
    tracing::debug!("Executing step {component:?} on {input:?}");
    let result = plugin
        .execute(&step.component, context, input)
        .await
        .change_context(ExecutionError::PluginError)?;
    tracing::debug!("Step {component:?} result: {result:?}");

    // Apply failure handling logic, if appropraite.
    let result = match result {
        FlowResult::Failed { error } => {
            let result = match &step.on_error {
                ErrorAction::Skip => {
                    tracing::debug!("Step {component:?} failed with error {error:?}, skipping");
                    FlowResult::Skipped
                }
                ErrorAction::UseDefault { default_value } => {
                    tracing::debug!("Step {component:?} failed, using default value");
                    // We expect the value to be nullable or for the default_value to be set.
                    let value = default_value
                        .to_owned()
                        .unwrap_or(serde_json::Value::Null.into());
                    FlowResult::Success { result: value }
                }
                ErrorAction::Fail => {
                    tracing::error!(?error, "Step {component:?} failed, failing");
                    return Err(error_stack::report!(ExecutionError::StepFailed {
                        step: step.id.clone(),
                    })
                    .attach_printable(error));
                }
                ErrorAction::Retry => {
                    todo!("Implement step retry logic");
                }
            };
            result_tx
                .send(result)
                .map_err(|_| ExecutionError::RecordResult)?;
            return Ok(());
        }
        other => other,
    };

    tracing::info!("Sending result {result:?}");
    result_tx
        .send(result)
        .map_err(|_| ExecutionError::RecordResult)?;

    Ok(())
}
