// Copyright 2025 DataStax Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
// in compliance with the License. You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied. See the License for the specific language governing permissions and limitations under
// the License.

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
