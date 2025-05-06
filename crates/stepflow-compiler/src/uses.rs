use std::collections::HashMap;

use stepflow_workflow::{Expr, Flow, ValueRef};

use crate::{CompileError, Result};

pub(crate) fn compute_uses(flow: &mut Flow) -> error_stack::Result<(), CompileError> {
    Uses::default().update_uses(flow)
}

#[derive(Default)]
struct Uses {
    /// Uses of each (step, output).
    uses: HashMap<ValueRef, u32>,
}

impl Uses {
    fn update_uses(&mut self, flow: &mut Flow) -> Result<()> {
        // First, iterate over the steps and initialize the use counts.
        for step in flow.steps.iter_mut() {
            let step_execution = step
                .execution
                .as_mut()
                .ok_or(CompileError::MissingStepExecution)?;
            for output in step_execution.outputs.iter() {
                self.uses.insert(
                    output.value_ref.as_ref().unwrap().clone(),
                    0,
                );
            }
        }

        // Now, compute the uses. We do this starting from the flow outputs.
        for output in flow.outputs.values() {
            self.increment_use(output)?;
        }

        for step in flow.steps.iter_mut().rev() {
            let execution = step
                .execution
                .as_mut()
                .ok_or(CompileError::MissingStepExecution)?;
            let mut needs_args = execution.always_execute;

            // By the time we reach a step, it should be finalized.
            // So, we write the usage information to the step.
            for output in execution.outputs.iter_mut() {
                let uses = self
                    .uses
                    .remove(output.value_ref.as_ref().unwrap())
                    .expect("all outputs registered earlier");
                output.uses = Some(uses);
                if uses > 0 {
                    needs_args = true;
                }
            }

            // If the step is always executed, or at least one of it's outputs is used,
            // then we need to evaluate the arguments, so increment those uses.
            if needs_args {
                println!("STEP: {step:?}");
                for arg in step.args.values() {
                    self.increment_use(arg)?;
                }
            }
        }
        Ok(())
    }

    fn increment_use(&mut self, arg: &Expr) -> Result<()> {
        match arg {
            Expr::Literal { .. } => {}
            Expr::Step { step_ref, value_ref, .. } => {
                let value_ref = value_ref.as_ref().ok_or(CompileError::InvalidValueRefFor(step_ref.clone()))?;
                let uses = self
                    .uses
                    .get_mut(value_ref)
                    .ok_or(CompileError::InvalidValueRef(value_ref.clone()))?;
                *uses += 1;
            }
        }
        Ok(())
    }
}
