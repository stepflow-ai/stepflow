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
use stepflow_core::workflow::BaseRef;
use stepflow_core::workflow::Expr;
use stepflow_core::workflow::Flow;

use crate::DiagnosticMessage;
use crate::Diagnostics;
use crate::Result;
use crate::validation::path::make_path;

/// Detect unreachable steps (steps that no other step or output depends on)
pub fn validate_step_reachability(flow: &Flow, diagnostics: &mut Diagnostics) -> Result<()> {
    let mut referenced_steps = HashSet::new();

    // Collect steps referenced by other steps
    for step in flow.steps() {
        collect_value_template_dependencies(&step.input, &mut referenced_steps)?;
        if let Some(skip_if) = &step.skip_if {
            collect_expression_dependencies(skip_if, &mut referenced_steps);
        }
    }

    // Collect steps referenced by workflow output
    collect_value_template_dependencies(flow.output(), &mut referenced_steps)?;

    // Find unreachable steps
    for (index, step) in flow.steps().iter().enumerate() {
        if !referenced_steps.contains(&step.id) {
            diagnostics.add(
                DiagnosticMessage::UnreachableStep {
                    step_id: step.id.clone(),
                },
                make_path!("steps", index),
            );
        }
    }

    Ok(())
}

/// Collect step dependencies from an expression
fn collect_expression_dependencies(expr: &Expr, dependencies: &mut HashSet<String>) {
    match expr {
        Expr::Ref { from, .. } => {
            if let BaseRef::Step { step } = from {
                dependencies.insert(step.clone());
            }
        }
        Expr::EscapedLiteral { .. } | Expr::Literal(_) => {}
    }
}

/// Collect step dependencies from ValueExpr
fn collect_value_template_dependencies(
    expr: &ValueExpr,
    dependencies: &mut HashSet<String>,
) -> Result<()> {
    // Extract dependencies using analyze_template_dependencies
    let deps = crate::dependency::analyze_template_dependencies(expr)?;

    for dep in deps.dependencies() {
        if let Some(step_id) = dep.step_id() {
            dependencies.insert(step_id.to_string());
        }
    }

    Ok(())
}
