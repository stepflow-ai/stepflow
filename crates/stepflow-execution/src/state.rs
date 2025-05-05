use crate::{ExecutionError, Result};
use error_stack::ResultExt;
use indexmap::IndexMap;
use stepflow_workflow::{Expr, Flow, StepExecution, Value};

pub struct VecState {
    pub slots: Vec<Value>,
}

impl VecState {
    pub fn try_new(flow: &Flow) -> Result<Self> {
        let execution = flow
            .execution
            .as_ref()
            .ok_or(ExecutionError::FlowNotCompiled)?;
        let slots = execution.slots as usize;
        let slots = vec![Value::NULL; slots];
        Ok(Self { slots })
    }

    fn get_value(&mut self, slot: u32) -> Result<&Value> {
        let Some(output) = self.slots.get(slot as usize) else {
            error_stack::bail!(ExecutionError::SlotOutOfBounds { slot });
        };

        Ok(output)
    }

    /// Resolve any references in the given arguments.
    ///
    /// The resulting `Arg`s should *not* include any references to another step's output.
    pub fn resolve(&mut self, args: &IndexMap<String, Expr>) -> Result<Vec<Value>> {
        let mut resolved_args = Vec::new();

        for arg in args.values() {
            match arg {
                Expr::Step { slot, step_ref } => {
                    // Resolve the step output to an actual argument
                    let slot = slot.ok_or(ExecutionError::FlowNotCompiled)?;
                    let resolved_arg = self
                        .get_value(slot)
                        .attach_printable_lazy(|| format!("step {step_ref:?} has no slot"))?;
                    resolved_args.push(resolved_arg.clone());
                }
                Expr::Literal { literal } => {
                    // Literal arguments can be added directly
                    resolved_args.push(literal.clone());
                }
            }
        }

        Ok(resolved_args)
    }

    /// Record results for the given step.
    ///
    /// This may fail if the given step is not the next step and out-of-order
    /// execution is not supported by this state backend.
    ///
    /// The outputs must all be literal `Arg` and should *not* be references
    /// to other step's output.
    pub fn record_results(&mut self, step: &StepExecution, results: Vec<Value>) -> Result<()> {
        for slot in &step.drop {
            self.slots[*slot as usize] = Value::NULL;
        }

        for (output, result) in step.outputs.iter().zip(results) {
            let slot = output.slot.ok_or(ExecutionError::FlowNotCompiled)? as usize;
            self.slots[slot] = result;
        }
        Ok(())
    }
}
