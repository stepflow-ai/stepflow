use std::collections::HashMap;

use error_stack::ResultExt as _;
use stepflow_workflow::{Expr, Flow, StepRef};

use crate::{CompileError, Result};

pub(crate) fn compute_uses(flow: &mut Flow) -> error_stack::Result<(), CompileError> {
    Uses::default().update_uses(flow)
}

#[derive(Default)]
struct Uses {
    /// Uses of each (step, output).
    uses: HashMap<StepRef, u32>,
}

impl Uses {
    fn update_uses(&mut self, flow: &mut Flow) -> Result<()> {
        // First, iterate over the steps and initialize the use counts.
        for step in flow.steps.iter_mut() {
            if let Some(step_id) = &step.id {
                let step_id = step_id.clone();
                let step_execution = step.execution_mut().change_context(CompileError::MissingStepExecution)?;
                for output in step_execution.outputs.iter() {
                    self.uses.insert(StepRef {
                        step_id: step_id.clone(),
                        output: output.name.clone(),
                    }, 0);
                }
            } else {
                // Without a name the step's outputs can't be referenced, so no uses.
                // We set the uses of each output to 0.
                let step_execution = step.execution_mut().change_context(CompileError::MissingStepExecution)?;
                for output in step_execution.outputs.iter_mut() {
                    output.uses = Some(0);
                }
            }
        }

        // Now, compute the uses. We do this starting from the flow outputs.
        for output in flow.outputs.values() {
            self.increment_use(output)?;
        }

        for step in flow.steps.iter_mut().rev() {
            let execution = step.execution().change_context(CompileError::MissingStepExecution)?;
            let mut needs_args = execution.always_execute;

            // By the time we reach a step, it should be finalized.
            // So, we write the usage information to the step.
            if let Some(step_id) = &step.id {
                let step_id = step_id.clone();
                let execution = step.execution_mut().expect("step has execution info");
                for output in execution.outputs.iter_mut() {
                    let step_ref = StepRef { step_id: step_id.clone(), output: output.name.clone() };
                    let uses = self.uses.remove(&step_ref).expect("all outputs registered earlier");
                    output.uses = Some(uses);
                    if uses > 0 {
                        needs_args = true;
                    }
                }
            }

            // If the step is always executed, or at least one of it's outputs is used,
            // then we need to evaluate the arguments, so increment those uses.
            if needs_args {
                for arg in step.args.values() {
                    self.increment_use(arg)?;
                }
            }
        }
        Ok(())
    }

    fn increment_use(&mut self, arg: &Expr) -> Result<()> {
        if let Expr::Step(step_ref) = arg {
            let uses = self
                .uses
                .get_mut(step_ref)
                .ok_or(CompileError::InvalidStepRef(step_ref.clone()))?;
            *uses += 1;
        }
        Ok(())
    }

}
