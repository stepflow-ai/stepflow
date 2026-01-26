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

use std::collections::HashSet;

use stepflow_core::workflow::Flow;

use crate::validation::path::make_path;
use crate::{DiagnosticKind, Diagnostics, diagnostic};

/// Validate basic workflow structure
pub fn validate_flow_structure(flow: &Flow, diagnostics: &mut Diagnostics) {
    // Check for duplicate step IDs
    let mut seen_ids = HashSet::new();
    for (index, step) in flow.steps().iter().enumerate() {
        if !seen_ids.insert(&step.id) {
            let step_id = &step.id;
            diagnostics.add(
                diagnostic!(
                    DiagnosticKind::DuplicateStepId,
                    "Duplicate step ID '{step_id}'",
                    { step_id }
                )
                .at(make_path!("steps", index, "id")),
            );
        }
    }

    // Check for empty step IDs
    for (index, step) in flow.steps().iter().enumerate() {
        if step.id.trim().is_empty() {
            diagnostics.add(
                diagnostic!(DiagnosticKind::EmptyStepId, "Step has empty ID")
                    .at(make_path!("steps", index, "id")),
            );
        }
    }

    // Warn if flow has no name
    if flow.name().is_none() || flow.name().unwrap().trim().is_empty() {
        diagnostics.add(
            diagnostic!(DiagnosticKind::MissingFlowName, "Workflow has no name")
                .at(make_path!("name")),
        );
    }

    // Warn if flow has no description
    if flow.description().is_none() {
        diagnostics.add(
            diagnostic!(
                DiagnosticKind::MissingFlowDescription,
                "Workflow has no description"
            )
            .at(make_path!("description")),
        );
    }
}
