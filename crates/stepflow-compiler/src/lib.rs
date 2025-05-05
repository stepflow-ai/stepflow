mod component_info;
mod error;
mod uses;

pub mod testing;
mod validation;

use std::collections::HashSet;

pub use error::{CompileError, Result};
use stepflow_workflow::Flow;
use stepflow_steps::Plugins;
pub use validation::validate_flow;

pub fn compile(plugins: &Plugins, mut flow: Flow) -> Result<Flow> {
    // 1. Fill in information about components.
    component_info::populate(plugins, &mut flow)?;

    // 2. Validate the flow is well-formed.
    //
    // This doesn't do the full validation since that checks the results
    // of compiling the flow.
    validate_unique_step_ids(&flow)?;
    validate_references(&flow)?;

    // 3. Compute which outputs are used.
    uses::compute_uses(&mut flow)?;

    // 4. Validate the result is valid.
    validate_flow(&flow)
        .map_err(|e| CompileError::Validation(e.to_string()))?;

    Ok(flow)
}

/// Validate that all step names are unique.
fn validate_unique_step_ids(flow: &Flow) -> Result<()> {
    let mut step_names: HashSet<&'_ str> = HashSet::with_capacity(flow.steps.len());
    for step in &flow.steps {
        let id = step.id.as_str();
        error_stack::ensure!(
            step_names.insert(id),
            CompileError::DuplicateStepId(id.to_owned())
        );
    }

    Ok(())
}

/// Check that all references are to previous steps and valid outputs.
fn validate_references(_flow: &Flow) -> Result<()> {
    // TODO
    Ok(())
}