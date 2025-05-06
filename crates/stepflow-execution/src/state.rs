use std::collections::HashMap;

use crate::{ExecutionError, Result};
use indexmap::IndexMap;
use stepflow_workflow::{Expr, StepExecution, StepRef, Value, ValueRef};

pub struct VecState {
    pub values: HashMap<ValueRef, Value>,
}

impl VecState {
    pub fn new() -> Self {
        Self { values: HashMap::new() }
    }

    fn get_value(&mut self, step: &StepRef, value_ref: &ValueRef) -> Result<&Value> {
        let Some(output) = self.values.get(value_ref) else {
            error_stack::bail!(ExecutionError::UndefinedValue { step_ref: step.clone(), value_ref: value_ref.clone() });
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
                Expr::Step { value_ref, step_ref } => {
                    // Resolve the step output to an actual argument
                    let value_ref = value_ref.as_ref().ok_or(ExecutionError::FlowNotCompiled)?;
                    let resolved_arg = self
                        .get_value(step_ref, value_ref)?;
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
        for (output, result) in step.outputs.iter().zip(results) {
            let value_ref = output.value_ref.ok_or(ExecutionError::FlowNotCompiled)?;
            self.values.insert(value_ref, result);
        }
        Ok(())
    }
}
