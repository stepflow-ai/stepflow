use error_stack::ResultExt as _;
use stepflow_steps::{Plugins, StepPlugin as _};
use stepflow_workflow::{Flow, StepExecution};

use crate::error::{CompileError, Result};

pub(crate) async fn populate<'a>(plugins: &'a Plugins<'a>, flow: &mut Flow) -> Result<()> {
    for (index, step) in flow.steps.iter_mut().enumerate() {
        println!("Populating step: {step:?}");
        let plugin = plugins
            .get(&step.component)
            .change_context(CompileError::NoPluginFound)?;

        let component_info = plugin
            .component_info(&step.component)
            .await
            .change_context(CompileError::ComponentInfo)?;
        if let Some(step_execution) = &step.execution {
            error_stack::ensure!(
                step_execution.always_execute == component_info.always_execute,
                CompileError::AlwaysExecuteDisagreement {
                    step: index,
                    expected: component_info.always_execute,
                    actual: step_execution.always_execute,
                }
            );
            error_stack::ensure!(
                step_execution.outputs == component_info.outputs,
                CompileError::OutputDisagreement {
                    step: index,
                    expected: component_info.outputs,
                    actual: step_execution.outputs.clone(),
                }
            );
        } else {
            step.execution = Some(StepExecution {
                always_execute: component_info.always_execute,
                outputs: component_info.outputs,
                ..Default::default()
            })
        }
    }
    Ok(())
}
