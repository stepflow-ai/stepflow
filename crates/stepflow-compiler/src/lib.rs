mod error;
mod uses;
mod value_refs;

mod validation;

use std::collections::HashSet;

pub use error::{CompileError, Result};
use stepflow_components::Plugins;
use stepflow_workflow::Flow;
pub use validation::validate_flow;

pub async fn compile<'a>(plugins: &'a Plugins<'a>, mut flow: Flow) -> Result<Flow> {
    // 1. Fill in information about components.
    value_refs::populate(plugins, &mut flow).await?;

    // 2. Validate the flow is well-formed.
    //
    // This doesn't do the full validation since that checks the results
    // of compiling the flow.
    validate_unique_step_ids(&flow)?;
    validate_references(&flow)?;

    // 3. Compute which outputs are used.
    uses::compute_uses(&mut flow)?;

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
