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

use stepflow_core::ValueExpr;
use stepflow_core::workflow::Flow;

use crate::validation::path::make_path;
use crate::{DiagnosticKind, Diagnostics, diagnostic};

/// Detect unreachable steps (steps that no other step or output depends on)
pub fn validate_step_reachability(flow: &Flow, diagnostics: &mut Diagnostics) {
    let mut referenced_steps = HashSet::new();

    // Collect steps referenced by other steps
    for step in flow.steps() {
        collect_step_references(&step.input, &mut referenced_steps);
    }

    // Collect steps referenced by workflow output
    collect_step_references(flow.output(), &mut referenced_steps);

    // Find unreachable steps
    for (index, step) in flow.steps().iter().enumerate() {
        if !referenced_steps.contains(&step.id) {
            let step_id = &step.id;
            diagnostics.add(
                diagnostic!(
                    DiagnosticKind::UnreachableStep,
                    "Step '{step_id}' is not reachable from the output",
                    { step_id }
                )
                .at(make_path!("steps", index)),
            );
        }
    }
}

/// Recursively collect all step IDs referenced by a ValueExpr
fn collect_step_references(expr: &ValueExpr, references: &mut HashSet<String>) {
    match expr {
        ValueExpr::Step { step, .. } => {
            references.insert(step.clone());
        }
        ValueExpr::Input { .. } | ValueExpr::Variable { .. } => {
            // No step references
        }
        ValueExpr::If {
            condition,
            then,
            else_expr,
        } => {
            collect_step_references(condition, references);
            collect_step_references(then, references);
            if let Some(else_val) = else_expr {
                collect_step_references(else_val, references);
            }
        }
        ValueExpr::Coalesce { values } => {
            for value in values {
                collect_step_references(value, references);
            }
        }
        ValueExpr::Array(items) => {
            for item in items {
                collect_step_references(item, references);
            }
        }
        ValueExpr::Object(fields) => {
            for (_key, value) in fields {
                collect_step_references(value, references);
            }
        }
        ValueExpr::Literal(_) | ValueExpr::EscapedLiteral { .. } => {
            // Literals have no references
        }
    }
}
