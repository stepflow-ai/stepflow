mod component_info;
mod error;
mod uses;

#[cfg(test)]
mod testing;

use std::collections::HashSet;

pub use error::{CompileError, Result};
use stepflow_workflow::Flow;
use stepflow_steps::Plugins;

pub fn compile(plugins: &Plugins, mut flow: Flow) -> Result<Flow> {
    // 1. Fill in information about components.
    component_info::populate(plugins, &mut flow)?;

    // 2. Validate the flow is well-formed.
    //
    // This doesn't do the full validation since that checks the results
    // of compiling the flow.
    validate_unique_step_names(&flow)?;
    validate_references(&flow)?;

    // 3. Compute which outputs are used.
    uses::compute_uses(&mut flow)?;

    Ok(flow)
}

/// Validate that all step names are unique.
fn validate_unique_step_names(flow: &Flow) -> Result<()> {
    let mut step_names: HashSet<&'_ str> = HashSet::with_capacity(flow.steps.len());
    for step in &flow.steps {
        if let Some(name) = &step.id {
            error_stack::ensure!(
                step_names.insert(name),
                CompileError::DuplicateStepName(name.to_owned())
            );
        }
    }

    Ok(())
}

/// Check that all references are to previous steps and valid outputs.
fn validate_references(_flow: &Flow) -> Result<()> {
    // TODO
    Ok(())
}

fn validate_slots_and_uses(_flow: &Flow) -> Result<()> {
    // TODO
    Ok(())
}

pub fn validate(flow: &Flow) -> Result<()> {
    validate_unique_step_names(flow)?;
    validate_references(flow)?;
    validate_slots_and_uses(flow)?;
    Ok(())
}
