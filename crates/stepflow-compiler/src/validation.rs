use std::collections::{HashMap, HashSet};

use stepflow_workflow::{Expr, Flow, Step, StepExecution, StepRef, ValueRef};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("invalid step {step_id}: {message}")]
    StepError {
        step_id: String,
        message: String,
    },
}

impl ValidationError {
    fn step(step: &Step, message: impl Into<String>) -> Self {
        Self::StepError {
            step_id: step.id.clone(),
            message: message.into(),
        }
    }
    // #[error("flow has no execution info")]
    // MissingFlowExecution,
    // #[error("step {step_idx}[{step_id:?}] has no execution info")]
    // MissingStepExecution { step_idx: usize, step_id: &'a str },
    // #[error("step {step_idx}[{step_id:?}] arg '{input_name}' has no value ref assigned")]
    // MissingStepInputValueRef {
    //     step_idx: usize,
    //     step_id: &'a str,
    //     arg_name: &'a str,
    // },
    // #[error("step {step_idx}[{step_id:?}] output '{output_name}' execution info missing uses")]
    // MissingStepUses {
    //     step_idx: usize,
    //     step_id: &'a str,
    //     output_name: &'a str,
    // },
    // #[error(
    //     "step {step_idx}[{step_id:?}] output '{output_name}' execution info missing slot (uses = {uses})"
    // )]
    // MissingStepSlot {
    //     step_idx: usize,
    //     step_id: &'a str,
    //     output_name: &'a str,
    //     uses: u32,
    // },
    // #[error(
    //     "unused step {step_idx}[{step_id:?}] output '{output_name}' execution info assigned slot (slot = {slot})"
    // )]
    // UnusedStepAssignedSlot {
    //     step_idx: usize,
    //     step_id: &'a str,
    //     output_name: &'a str,
    //     slot: u32,
    // },
    // #[error(
    //     "step {step_idx}[{step_id:?}] output '{output_name}' execution info slot out of bounds (slot = {slot})"
    // )]
    // OutputSlotOutOfBounds {
    //     step_idx: usize,
    //     step_id: &'a str,
    //     output_name: &'a str,
    //     slot: u32,
    // },
    // #[error("step {step_idx}[{step_id:?}] input '{input_name}' slot out of bounds (slot = {slot})")]
    // InputSlotOutOfBounds {
    //     step_idx: usize,
    //     step_id: &'a str,
    //     input_name: &'a str,
    //     slot: u32,
    // },
    // #[error(
    //     "step {step_idx}[{step_id:?}] output '{output_name}' execution info slot in use (slot = {slot})"
    // )]
    // SlotInUse {
    //     step_idx: usize,
    //     step_id: &'a str,
    //     output_name: &'a str,
    //     slot: u32,
    // },
    // #[error(
    //     "step {step_idx}[{step_id:?}] input '{input_name}' slot dropped (slot = {slot}, step_ref = {step_ref:?})"
    // )]
    // InputSlotDropped {
    //     step_idx: usize,
    //     step_id: &'a str,
    //     input_name: &'a str,
    //     step_ref: &'a StepRef,
    //     slot: u32,
    // },
    // #[error(
    //     "step {step_idx}[{step_id:?}] input '{input_name}' slot mismatch (slot = {slot}, expected = {expected_step_ref:?}, actual = {actual_step_ref:?})"
    // )]
    // InputValueRefMismatch {
    //     step_idx: usize,
    //     step_id: &'a str,
    //     input_name: &'a str,
    //     slot: u32,
    //     expected_step_ref: &'a StepRef,
    //     actual_step_ref: StepRef,
    // },
    // #[error("duplicate step ID: {step_id}")]
    // DuplicateStepId { step_id: &'a str },
    // #[error("step {step_idx}[{step_id:?}] drop slot {slot} out of bounds")]
    // DropSlotOutOfBounds {
    //     step_idx: usize,
    //     step_id: &'a str,
    //     slot: u32,
    // },
    // #[error("step {step_idx}[{step_id:?}] drop slot {slot} in use")]
    // DropSlotInUse {
    //     step_idx: usize,
    //     step_id: &'a str,
    //     slot: u32,
    // },
    // #[error("step {step_idx}[{step_id:?}] drop slot {slot} missing")]
    // DropSlotMissing {
    //     step_idx: usize,
    //     step_id: &'a str,
    //     slot: u32,
    // },
    // #[error("step {step_idx}[{step_id:?}] unused slot {slot} not dropped")]
    // UnusedSlotNotDropped {
    //     step_idx: usize,
    //     step_id: &'a str,
    //     slot: u32,
    // },
}

#[derive(Clone)]
struct ValueInfo {
    remaining_uses: u32,
    step_ref: StepRef,
}

struct Validator<'a> {
    flow: &'a Flow,
    values: HashMap<ValueRef, ValueInfo>,
}

impl<'a> Validator<'a> {
    fn new(flow: &'a Flow) -> Result<Self, ValidationError> {
        let values = HashMap::new();
        Ok(Self { flow, values })
    }

    fn validate_flow(&mut self) -> Result<(), ValidationError> {
        // Iterate over the steps tracking slot usage information.
        let mut step_ids = HashSet::new();
        for (step_index, step) in self.flow.steps.iter().enumerate() {
            let step_id = step.id.as_str();

            if !step_ids.insert(step_id) {
                return Err(ValidationError::step(step, "duplicate step ID"));
            }

            let step_execution = step
                .execution
                .as_ref()
                .ok_or(ValidationError::step(step, "missing step execution"))?;

            self.validate_step_inputs(step)?;
            self.validate_step_outputs(step, step_index, step_execution)?;
        }
        Ok(())
    }

    fn validate_step_inputs(
        &mut self,
        step: &'a Step,
    ) -> Result<(), ValidationError> {
        // Validate step inputs.
        for (name, expr) in step.args.iter() {
            match expr {
                Expr::Literal { .. } => {
                    // TODO: Validate types?
                }
                Expr::Step { value_ref: None, .. } => {
                    return Err(ValidationError::step(step, format!("no value ref assigned to argument {name:?}")));
                }
                Expr::Step {
                    step_ref,
                    value_ref: Some(value_ref),
                } => {
                    if let Some(value_info) = self.values.get_mut(value_ref) {
                        assert!(value_info.remaining_uses > 0);

                        if &value_info.step_ref != step_ref {
                            return Err(ValidationError::step(step, 
                                format!("assigned value ref {value_ref:?} does not match step ref {step_ref:?}")));
                        }
                        if value_info.remaining_uses == 0 {
                            return Err(ValidationError::step(step, 
                                format!("value ref {value_ref:?} has no remaining uses")));
                        }
                        value_info.remaining_uses -= 1;
                    } else {
                        return Err(ValidationError::step(step, format!("invalid value ref {value_ref:?} not assigned to {step_ref:?}")))
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_step_outputs(
        &mut self,
        step: &'a Step,
        step_index: usize,
        step_execution: &'a StepExecution,
    ) -> Result<(), ValidationError> {
        // Validate step outputs.
        for output in &step_execution.outputs {
            let output_name = output.name.as_str();
            let uses = output.uses.ok_or(ValidationError::step(step,
            format!("missing uses for output '{output_name}'")))?;

            self.validate_step_output(step, step_index, output_name, output.value_ref.as_ref(), uses)?;
        }

        Ok(())
    }

    fn validate_step_output(
        &mut self,
        step: &'a Step,
        step_index: usize,
        output_name: &'a str,
        value_ref: Option<&'a ValueRef>,
        uses: u32,
    ) -> Result<(), ValidationError> {
        let value_ref = value_ref.ok_or(ValidationError::step(step,
            format!("missing value ref for output '{output_name}'")))?;

        if value_ref.step_index != step_index as u32 {
            return Err(ValidationError::step(step,
                format!("value ref {value_ref:?} assigned to {output_name} is assigned to incorret step {step_index}")));
        }


        self.values.insert(value_ref.clone(), ValueInfo {
            remaining_uses: uses,
            step_ref: StepRef {
                step_id: step.id.to_owned(),
                output: output_name.to_owned(),
            },
        });
    

        Ok(())
    }
}

/// Validates a compiled flow.
///
/// This ensures that:
/// - All steps, step references, and the flow itself have execution information.
/// - All step references are valid.
/// - All step outputs have the correct number of uses.
/// - Slots are dropped when no longer needed.
/// - Step outputs are not referenced after the corresponding slot is dropped.
/// - Step outputs are not referenced before the step has executed.
pub fn validate_flow(flow: &Flow) -> Result<(), ValidationError> {
    Validator::new(flow)?.validate_flow()
}
