use std::collections::{HashMap, HashSet};

use stepflow_workflow::{Expr, Flow, Step, StepExecution, StepRef};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("invalid flow input {name}: {message}")]
    Input { name: String, message: String },
    #[error("invalid step {step}: {message}")]
    StepError { step: String, message: String },
}

impl ValidationError {
    fn input(name: &str, message: impl Into<String>) -> Self {
        Self::Input {
            name: name.to_owned(),
            message: message.into(),
        }
    }

    fn step(step: &Step, message: impl Into<String>) -> Self {
        Self::StepError {
            step: step.id.clone(),
            message: message.into(),
        }
    }
}

#[derive(Clone)]
struct ValueInfo {
    remaining_uses: u32,
    step_ref: Option<StepRef>,
    input: Option<String>,
}

struct Validator<'a> {
    flow: &'a Flow,
    values: HashMap<Expr, ValueInfo>,
}

impl<'a> Validator<'a> {
    fn new(flow: &'a Flow) -> Result<Self, ValidationError> {
        let values = HashMap::new();
        Ok(Self { flow, values })
    }

    fn validate_flow(&mut self) -> Result<(), ValidationError> {
        // Iterate over the steps.
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

    fn validate_flow_input(
        &mut self,
        name: &str,
        input: &FlowInput,
    ) -> Result<(), ValidationError> {
        let value_ref = input
            .value_ref
            .ok_or(ValidationError::input(name, "missing value ref"))?;
        let uses = input
            .uses
            .ok_or(ValidationError::input(name, "missing uses"))?;

        self.values.insert(
            value_ref,
            ValueInfo {
                remaining_uses: uses,
                step_ref: None,
                input: Some(name.to_owned()),
            },
        );
        Ok(())
    }

    fn validate_step_inputs(&mut self, step: &'a Step) -> Result<(), ValidationError> {
        // Validate step inputs.
        for (name, expr) in step.args.iter() {
            match expr {
                Expr::Literal { .. } => {
                    // TODO: Validate types?
                }
                Expr::Input {
                    value_ref: None, ..
                }
                | Expr::Step {
                    value_ref: None, ..
                } => {
                    return Err(ValidationError::step(
                        step,
                        format!("no value ref assigned to argument {name:?}"),
                    ));
                }
                Expr::Step {
                    step_ref,
                    value_ref: Some(value_ref),
                } => {
                    if let Some(value_info) = self.values.get_mut(value_ref) {
                        assert!(value_info.remaining_uses > 0);

                        if value_info.step_ref.as_ref() != Some(step_ref) {
                            return Err(ValidationError::step(
                                step,
                                format!(
                                    "assigned value ref {value_ref:?} does not match step ref {step_ref:?}"
                                ),
                            ));
                        }
                        if value_info.remaining_uses == 0 {
                            return Err(ValidationError::step(
                                step,
                                format!("value ref {value_ref:?} has no remaining uses"),
                            ));
                        }
                        value_info.remaining_uses -= 1;
                    } else {
                        return Err(ValidationError::step(
                            step,
                            format!("invalid value ref {value_ref:?} assigned to {step_ref:?}"),
                        ));
                    }
                }
                Expr::Input {
                    input,
                    value_ref: Some(value_ref),
                } => {
                    if let Some(value_info) = self.values.get_mut(value_ref) {
                        assert!(value_info.remaining_uses > 0);

                        if value_info.input.as_ref().map(|s| s.as_str()) != Some(input.as_str()) {
                            return Err(ValidationError::step(
                                step,
                                format!(
                                    "assigned value ref {value_ref:?} does not match input ref {input:?}"
                                ),
                            ));
                        }
                        if value_info.remaining_uses == 0 {
                            return Err(ValidationError::step(
                                step,
                                format!("value ref {value_ref:?} has no remaining uses"),
                            ));
                        }
                        value_info.remaining_uses -= 1;
                    } else {
                        return Err(ValidationError::step(
                            step,
                            format!("invalid value ref {value_ref:?} assigned to input {input:?}"),
                        ));
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
            let uses = output.uses.ok_or(ValidationError::step(
                step,
                format!("missing uses for output '{output_name}'"),
            ))?;

            self.validate_step_output(
                step,
                step_index,
                output_name,
                output.value_ref.as_ref(),
                uses,
            )?;
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
        let value_ref = value_ref.ok_or(ValidationError::step(
            step,
            format!("missing value ref for output '{output_name}'"),
        ))?;

        match value_ref {
            ValueRef::Step { step, .. } if *step == step_index as u32 => {
                // Step is valid
            }
            value_ref => {
                return Err(ValidationError::step(
                    step,
                    format!(
                        "value ref {value_ref:?} assigned to {output_name} is assigned to incorrect step"
                    ),
                ));
            }
        }

        self.values.insert(
            value_ref.clone(),
            ValueInfo {
                remaining_uses: uses,
                step_ref: Some(StepRef {
                    step: step.id.to_owned(),
                    field: output_name.to_owned(),
                }),
                input: None,
            },
        );

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
