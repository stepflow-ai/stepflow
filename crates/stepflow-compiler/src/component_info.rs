use std::collections::HashMap;

use error_stack::ResultExt as _;
use stepflow_steps::{Plugins, StepPlugin as _};
use stepflow_workflow::{Expr, Flow, StepExecution, StepRef, ValueRef};

use crate::error::{CompileError, Result};

pub(crate) async fn populate<'a>(plugins: &'a Plugins<'a>, flow: &mut Flow) -> Result<()> {
    let mut value_refs = HashMap::new();

    for (index, step) in flow.steps.iter_mut().enumerate() {
        println!("Populating step: {step:?}");
        let plugin = plugins
            .get(&step.component)
            .change_context(CompileError::NoPluginFound)?;

        let component_info = plugin
            .component_info(&step.component)
            .await
            .change_context(CompileError::ComponentInfo)?;

        if step.execution.is_some() {
            tracing::info!("step '{}' already has execution info", step.id);
        }

        populate_arg_value_refs(&value_refs, step.args.values_mut())?;

        let always_execute = component_info.always_execute;
        let mut outputs = component_info.outputs;
        for (output_idx, output) in outputs.iter_mut().enumerate() {
            let value_ref = ValueRef {
                step_index: index as u32,
                output_index: output_idx as u32,
            };
            println!("Using value ref: {value_ref:?} for step {} output '{}'", step.id, output.name);
            value_refs.insert(StepRef {
                step: step.id.clone(),
                output: output.name.clone(),
            }, value_ref.clone());
            output.value_ref = Some(value_ref);
        }

        step.execution = Some(StepExecution {
            always_execute,
            outputs,
        });
    }

    populate_arg_value_refs(&value_refs, flow.outputs.values_mut())?;

    Ok(())
}

fn populate_arg_value_refs<'a>(value_refs: &HashMap<StepRef, ValueRef>, args: impl Iterator<Item = &'a mut Expr>) -> Result<()> {
    for arg in args {
        match arg {
            Expr::Step { step_ref, value_ref, .. } => {
                let assigned_value_ref = value_refs.get(step_ref).cloned().ok_or(CompileError::InvalidValueRefFor(step_ref.clone()))?;
                *value_ref = Some(assigned_value_ref);
            }
            Expr::Literal { .. } => {}
        }
    }
    Ok(())
}