use std::borrow::Cow;

use stepflow_core::workflow::{Component, Flow};

use crate::{DiagnosticMessage, Diagnostics, validation::path::make_path};

pub fn validate_components(flow: &Flow, diagnostics: &mut Diagnostics) {
    for (index, step) in flow.steps().iter().enumerate() {
        validate_component(&step.component, index, &step.id, diagnostics);
    }
}

/// Validate a component URL
fn validate_component(
    component: &Component,
    step_index: usize,
    step_id: &str,
    diagnostics: &mut Diagnostics,
) {
    let path_str = component.path();
    if !path_str.starts_with('/') {
        let error = Cow::Borrowed("Component path must start with '/'");

        diagnostics.add(
            DiagnosticMessage::InvalidComponent {
                step_id: step_id.to_owned(),
                component: path_str.to_string(),
                error,
            },
            make_path!("steps", step_index, "component"),
        );
    }

    // TODO: Validate components against the plugins/routing configuration.
    // TODO: Validate input/output schema information.
}
