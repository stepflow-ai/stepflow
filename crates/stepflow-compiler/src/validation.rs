use std::collections::HashSet;

use stepflow_workflow::{Expr, Flow, Step, StepExecution, StepRef};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ValidationError<'a> {
    #[error("flow has no execution info")]
    MissingFlowExecution,
    #[error("step {step_idx}[{step_id:?}] has no execution info")]
    MissingStepExecution {
        step_idx: usize,
        step_id: &'a str,
    },
    #[error("step {step_idx}[{step_id:?}] input '{input_name}' has no slot assigned")]
    MissingStepInputSlot {
        step_idx: usize,
        step_id: &'a str,
        input_name: &'a str,
    },
    #[error("step {step_idx}[{step_id:?}] output '{output_name}' execution info missing uses")]
    MissingStepUses {
        step_idx: usize,
        step_id: &'a str,
        output_name: &'a str,
    },
    #[error("step {step_idx}[{step_id:?}] output '{output_name}' execution info missing slot (uses = {uses}")]
    MissingStepSlot {
        step_idx: usize,
        step_id: &'a str,
        output_name: &'a str,
        uses: u32,
    },
    #[error("unused step {step_idx}[{step_id:?}] output '{output_name}' execution info assigned slot (slot = {slot})")]
    UnusedStepAssignedSlot {
        step_idx: usize,
        step_id: &'a str,
        output_name: &'a str,
        slot: u32,
    },
    #[error("step {step_idx}[{step_id:?}] output '{output_name}' execution info slot out of bounds (slot = {slot}")]
    OutputSlotOutOfBounds {
        step_idx: usize,
        step_id: &'a str,
        output_name: &'a str,
        slot: u32,
    },
    #[error("step {step_idx}[{step_id:?}] input '{input_name}' slot out of bounds (slot = {slot}")]
    InputSlotOutOfBounds {
        step_idx: usize,
        step_id: &'a str,
        input_name: &'a str,
        slot: u32,
    },
    #[error("step {step_idx}[{step_id:?}] output '{output_name}' execution info slot in use (slot = {slot})")]
    SlotInUse {
        step_idx: usize,
        step_id: &'a str,
        output_name: &'a str,
        slot: u32,
    },
    #[error("step {step_idx}[{step_id:?}] input '{input_name}' slot dropped (slot = {slot}, step_ref = {step_ref:?})")]
    InputSlotDropped {
        step_idx: usize,
        step_id: &'a str,
        input_name: &'a str,
        step_ref: &'a StepRef,
        slot: u32,
    },
    #[error("step {step_idx}[{step_id:?}] input '{input_name}' slot mismatch (slot = {slot}, expected = {expected_step_ref:?}, actual = {actual_step_ref:?})")]
    InputSlotMismatch {
        step_idx: usize,
        step_id: &'a str,
        input_name: &'a str,
        slot: u32,
        expected_step_ref: &'a StepRef,
        actual_step_ref: StepRef,
    },
    #[error("duplicate step ID: {step_id}")]
    DuplicateStepId {
        step_id: &'a str,
    },
    #[error("step {step_idx}[{step_id:?}] drop slot {slot} out of bounds")]
    DropSlotOutOfBounds { step_idx: usize, step_id: &'a str, slot: u32 },
    #[error("step {step_idx}[{step_id:?}] drop slot {slot} in use")]
    DropSlotInUse { step_idx: usize, step_id: &'a str, slot: u32 },
    #[error("step {step_idx}[{step_id:?}] drop slot {slot} missing")]
    DropSlotMissing { step_idx: usize, step_id: &'a str, slot: u32 },
    #[error("step {step_idx}[{step_id:?}] unused slot {slot} not dropped")]
    UnusedSlotNotDropped { step_idx: usize, step_id: &'a str, slot: u32 },
}

#[derive(Clone)]
struct SlotInfo {
    remaining_uses: u32,
    step_ref: StepRef,
}

struct Validator<'a> {
    flow: &'a Flow,
    slots: Vec<Option<SlotInfo>>,
}

impl<'a> Validator<'a> {
    fn new(flow: &'a Flow) -> Self {
        let slots: Vec<Option<SlotInfo>> = vec![None; flow.execution.as_ref().unwrap().slots as usize];
        Self { flow, slots }
    }

    fn validate_flow(&mut self) -> Result<(), ValidationError<'a>> {
        // Iterate over the steps tracking slot usage information.
        let mut step_ids = HashSet::new();
        for (step_idx, step) in self.flow.steps.iter().enumerate() {
            let step_id = step.id.as_str();

            if !step_ids.insert(step_id) {
                return Err(ValidationError::DuplicateStepId { step_id });
            }
            
            let step_execution = step.execution.as_ref().ok_or(ValidationError::MissingStepExecution {
                step_idx,
                step_id,
            })?;

            self.validate_step_inputs(step_idx, step_id, step)?;
            self.validate_step_drops(step_idx, step_id, step_execution)?;
            self.validate_step_outputs(step_idx, step_id, step_execution)?;
        }

        Ok(())
    }

    fn validate_step_inputs(&mut self,
        step_idx: usize, step_id: &'a str, step: &'a Step) -> Result<(), ValidationError<'a>>{
        // Validate step inputs.
        for (name, expr) in step.args.iter() {
            match expr {
                Expr::Literal { .. } => {
                    // TODO: Validate types?
                }
                Expr::Step { slot: None, .. } => {
                    return Err(ValidationError::MissingStepInputSlot {
                        step_idx,
                        step_id,
                        input_name: name.as_str(),
                    })
                }
                Expr::Step { step_ref, slot: Some(slot) } => {
                    let slot = *slot;
                    if slot as usize > self.slots.len() {
                        return Err(ValidationError::InputSlotOutOfBounds {
                            step_idx,
                            step_id,
                            input_name: name.as_str(),
                            slot,
                        });
                    }

                    if let Some(slot_info) = self.slots[slot as usize].as_mut() {
                        assert!(slot_info.remaining_uses > 0);

                        if &slot_info.step_ref != step_ref {
                            return Err(ValidationError::InputSlotMismatch {
                                step_idx,
                                step_id,
                                input_name: name.as_str(),
                                slot,
                                expected_step_ref: step_ref,
                                actual_step_ref: slot_info.step_ref.clone(),
                            })
                        }
                        slot_info.remaining_uses -= 1;
                    } else {
                        return Err(ValidationError::InputSlotDropped {
                            step_idx,
                            step_id,
                            input_name: name.as_str(),
                            step_ref,
                            slot,
                        })
                    }                        
                }
            }
        }
        Ok(())
    }
    
    fn validate_step_drops(&mut self,
        step_idx: usize, step_id: &'a str, step_execution: &'a StepExecution) -> Result<(), ValidationError<'a>> {
        // 1. All slots that are dropped should be no longer used..
        for drop in step_execution.drop.iter() {
            let index = *drop as usize;
            if index >= self.slots.len() {
                return Err(ValidationError::DropSlotOutOfBounds {
                    step_idx,
                    step_id,
                    slot: *drop,
                });
            }
            if let Some(slot_info) = self.slots[index].take() {
                if slot_info.remaining_uses > 0 {
                    return Err(ValidationError::DropSlotInUse {
                        step_idx,
                        step_id,
                        slot: *drop,
                    });
                }
            } else {
                return Err(ValidationError::DropSlotMissing {
                    step_idx,
                    step_id,
                    slot: *drop,
                });
            }
        }

        // 2. All slots that are no longer used should be dropped.
        for (slot, slot_info) in self.slots.iter().enumerate() {
            if let Some(slot_info) = slot_info {
                if slot_info.remaining_uses > 0 {
                    return Err(ValidationError::UnusedSlotNotDropped {
                        step_idx,
                        step_id,
                        slot: slot as u32,
                    });
                }
            }
        }

        Ok(())
    }

    fn validate_step_outputs(&mut self,
        step_idx: usize, step_id: &'a str, 
            step_execution: &'a StepExecution) -> Result<(), ValidationError<'a>>{
        
        // Validate step outputs.
        for output in &step_execution.outputs {
            let output_name = output.name.as_str();
            let uses = output.uses.ok_or(ValidationError::MissingStepUses {
                step_idx,
                step_id,
                output_name,
            })?;

            self.validate_step_output(step_idx, step_id, output_name, output.slot, uses)?;
        }

        Ok(())
    }

    fn validate_step_output(&mut self,
        step_idx: usize, step_id: &'a str, output_name: &'a str, slot: Option<u32>, uses: u32) -> Result<(), ValidationError<'a>> {

        if uses == 0 {
            if let Some(slot) = slot {
                return Err(ValidationError::UnusedStepAssignedSlot {
                    step_idx,
                    step_id,
                    output_name,
                    slot,
                });
            }
        } else {
            let slot = slot.ok_or(ValidationError::MissingStepSlot {
                step_idx,
                step_id,
                output_name,
                uses,
            })?;        

            if slot as usize >= self.slots.len() {
                return Err(ValidationError::OutputSlotOutOfBounds {
                    step_idx,
                    step_id,
                    output_name,
                    slot,
                });
            }
            if self.slots[slot as usize].is_some() {
                return Err(ValidationError::SlotInUse {
                    step_idx,
                    step_id,
                    output_name,
                    slot,
                });
            }
            self.slots[slot as usize] = Some(SlotInfo {
                remaining_uses: uses,
                step_ref: StepRef { step_id: step_id.to_owned(), output: output_name.to_owned() },
            });
        }

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
pub fn validate_flow(flow: &Flow) -> Result<(), ValidationError<'_>> {
    Validator::new(flow).validate_flow()
}
