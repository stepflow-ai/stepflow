use std::collections::HashMap;

use stepflow_workflow::{Expr, Flow, FlowExecution, StepRef};

use crate::{CompileError, Result};

pub(crate) fn assign_slots(flow: &mut Flow) -> error_stack::Result<(), CompileError> {
    Slots::default().assign_slots(flow)
}

struct Allocated {
    slot: u32,
    uses: u32,
}

#[derive(Default)]
struct Slots {
    next_slot: u32,
    free: Vec<u32>,
    allocated: HashMap<StepRef, Allocated>,
}

impl Slots {
    fn release_args<'a>(&mut self, args: impl Iterator<Item = &'a mut Expr>) -> Result<Vec<u32>>{
        let mut drop_slots = Vec::new();
        for arg in args {
            match arg {
                Expr::Step { step_ref, slot } => {
                    let allocated = self.allocated.get_mut(step_ref).unwrap();
                    *slot = Some(allocated.slot);
                    allocated.uses -= 1;
                    if allocated.uses == 0 {
                        drop_slots.push(allocated.slot);
                        self.free.push(allocated.slot);
                        self.allocated.remove(step_ref);
                    }
                },
                Expr::Literal { .. } => {} ,
            }
        }

        Ok(drop_slots)
    }

    fn assign_slot(&mut self, step_ref: StepRef, uses: Option<u32>) -> Option<u32> {
        let uses = uses.unwrap_or(0);
        if uses == 0 {
            return None;
        }

        let slot = if let Some(slot) = self.free.pop() {
            slot
        } else {
            let slot = self.next_slot;
            self.next_slot += 1;
            slot
        };
        self.allocated.insert(step_ref, Allocated {
            slot,
            uses
        });
        Some(slot)
    }

    fn assign_slots(&mut self, flow: &mut Flow) -> Result<()> {
        // TODO: Assign slot to the flow inputs.

        for step in flow.steps.iter_mut() {
            let execution = step.execution.as_mut().unwrap();
            execution.drop = self.release_args(step.args.values_mut())?;

            for output in execution.outputs.iter_mut() {
                output.slot = self.assign_slot(StepRef {
                    step_id: step.id.clone(),
                    output: output.name.clone(),
                },
                output.uses);
            }
        }

        flow.execution = Some(FlowExecution {
            slots: self.next_slot,
        });

        Ok(())
    }
}
