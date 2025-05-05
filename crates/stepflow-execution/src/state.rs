use crate::{ExecutionError, Result};
use error_stack::ResultExt;
use indexmap::IndexMap;
use stepflow_workflow::{Expr, Flow, Value};

pub trait State {
    /// Resolve any references in the given arguments.
    ///
    /// The resulting `Arg`s should *not* include any references to another step's output.
    fn resolve(&mut self, args: &IndexMap<String, Expr>) -> Result<Vec<Value>>;

    /// Record results for the given step.
    ///
    /// This may fail if the given step is not the next step and out-of-order
    /// execution is not supported by this state backend.
    ///
    /// The outputs must all be literal `Arg` and should *not* be references
    /// to other step's output.
    fn record_results(&mut self, outputs: Vec<Value>) -> Result<()>;
}

pub struct VecState {
    pub slots: Vec<Value>,
}

impl VecState {
    pub fn try_new(flow: &Flow) -> Result<Self> {
        let execution = flow.execution.as_ref().ok_or(ExecutionError::FlowNotCompiled)?;
        let slots = execution.slots as usize;
        let slots = Vec::with_capacity(slots);
        Ok(Self { slots })
    }

    fn get_value(&mut self, slot: u32) -> Result<&Value> {
        // Ensure the step index exists
        // TODO: Error handling? Mention the step? Or push this to validation and
        // assume no errors?
        let Some(output) = self.slots.get(slot as usize) else {
            error_stack::bail!(ExecutionError::SlotOutOfBounds { slot });
        };

        Ok(output)
    }
}

impl State for VecState {
    fn resolve(&mut self, args: &IndexMap<String, Expr>) -> Result<Vec<Value>> {
        let mut resolved_args = Vec::new();

        for arg in args.values() {
            match arg {
                Expr::Step { slot, step_ref } => {
                    // Resolve the step output to an actual argument
                    let slot = slot.ok_or(ExecutionError::FlowNotCompiled)?;
                    let resolved_arg = self.get_value(slot)
                        .attach_printable_lazy(|| format!("step {step_ref:?} has no slot"))?;
                    resolved_args.push(resolved_arg.clone());
                }
                Expr::Literal{literal} => {
                    // Literal arguments can be added directly
                    resolved_args.push(literal.clone());
                }
            }
        }

        Ok(resolved_args)
    }

    fn record_results(&mut self, outputs: Vec<Value>) -> Result<()> {
        // TODO: Ensure that all outputs are literals and not references
        // TODO: debug_assert the outputs match the next step?
        self.slots.extend(outputs);
        Ok(())
    }
}
