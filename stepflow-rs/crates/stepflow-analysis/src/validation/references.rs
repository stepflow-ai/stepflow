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

use stepflow_core::{
    values::{PathPart, ValueExpr},
    workflow::{Flow, Step},
};

use crate::validation::path::make_path;
use crate::{DiagnosticKind, Diagnostics, Path, diagnostic};

/// Validate step ordering and references - steps can only reference earlier steps
pub fn validate_references(flow: &Flow, diagnostics: &mut Diagnostics) {
    let mut available_steps = HashSet::new();

    for (index, step) in flow.steps().iter().enumerate() {
        // Validate this step only references previously defined steps
        validate_step_references(step, index, &available_steps, flow, diagnostics);

        // Add this step to available set for future steps
        available_steps.insert(step.id.clone());
    }

    // Validate the flow outputs against all of the available steps.
    validate_value_expr(
        flow.output(),
        &make_path!("output"),
        &available_steps,
        "workflow_output",
        flow,
        diagnostics,
    );
}

/// Validate that a step only references available (previously defined) steps
fn validate_step_references(
    step: &Step,
    step_index: usize,
    available_steps: &HashSet<String>,
    flow: &Flow,
    diagnostics: &mut Diagnostics,
) {
    let mut path = make_path!("steps", step_index);

    path.push("input");
    validate_value_expr(
        &step.input,
        &path,
        available_steps,
        &step.id,
        flow,
        diagnostics,
    );

    // TODO: Warn about mock components. We'll need to look at the plugin
    // definitions to find out which ones are actually registered as mocsk.
}

/// Validate all references within a ValueExpr
fn validate_value_expr(
    expr: &ValueExpr,
    path: &Path,
    available_steps: &HashSet<String>,
    current_step_id: &str,
    flow: &Flow,
    diagnostics: &mut Diagnostics,
) {
    match expr {
        ValueExpr::Step {
            step,
            path: field_path,
        } => {
            // Check for self-reference
            if current_step_id == step {
                let step_id = step.clone();
                diagnostics.add(
                    diagnostic!(
                        DiagnosticKind::SelfReference,
                        "Step '{step_id}' references itself",
                        { step_id }
                    )
                    .at(path.clone()),
                );
                return;
            }

            // Check if step exists and is available (defined earlier)
            if !available_steps.contains(step) {
                let from_step = current_step_id.to_string();
                let referenced_step = step.clone();
                diagnostics.add(
                    diagnostic!(
                        DiagnosticKind::UndefinedStepReference,
                        "Step '{from_step}' references undefined step '{referenced_step}'",
                        { from_step, referenced_step }
                    )
                    .at(path.clone()),
                );
                return;
            }

            // Generate experimental diagnostic about potential field access issues (when we don't have schema info)
            if !field_path.is_empty() {
                let step_id = step.clone();
                let field = field_path.to_string();
                let reason = "no output schema available".to_string();
                diagnostics.add(
                    diagnostic!(
                        DiagnosticKind::UnvalidatedFieldAccess,
                        "Field access '{field}' on step '{step_id}' cannot be validated: {reason}",
                        { step_id, field, reason }
                    )
                    .at(path.clone())
                    .experimental(),
                );
            }
        }
        ValueExpr::Input { input } => {
            // Workflow input reference is always valid
            // Generate experimental warning about unvalidated field access only if there's no input schema
            if !input.is_empty() && flow.input_schema().is_none() {
                let step_id = "workflow_input".to_string();
                let field = input.to_string();
                let reason = "no input schema available".to_string();
                diagnostics.add(
                    diagnostic!(
                        DiagnosticKind::UnvalidatedFieldAccess,
                        "Field access '{field}' on step '{step_id}' cannot be validated: {reason}",
                        { step_id, field, reason }
                    )
                    .at(path.clone())
                    .experimental(),
                );
            }
        }
        ValueExpr::Variable { variable, default } => {
            // Extract variable name from the path
            let parts = variable.parts();
            if parts.is_empty() {
                return;
            }

            let var_name = match &parts[0] {
                PathPart::Field(name) | PathPart::IndexStr(name) => name.as_str(),
                PathPart::Index(_) => {
                    // Variable paths should start with a field name
                    return;
                }
            };

            // Validate variable references against the variables schema
            if let Some(var_schema) = flow.variables() {
                if !var_schema.variables().contains(&var_name.to_string()) {
                    // Check if it's required or has a default
                    let has_inline_default = default.is_some();
                    let has_schema_default = var_schema.default_value(var_name).is_some();

                    if !has_inline_default && !has_schema_default {
                        let variable = var_name.to_string();
                        let context = format!("step '{}'", current_step_id);
                        diagnostics.add(
                            diagnostic!(
                                DiagnosticKind::UndefinedRequiredVariable,
                                "Required variable '{variable}' is not defined in {context}",
                                { variable, context }
                            )
                            .at(path.clone()),
                        );
                    } else {
                        let variable = var_name.to_string();
                        let context = format!("step '{}'", current_step_id);
                        diagnostics.add(
                            diagnostic!(
                                DiagnosticKind::UndefinedVariable,
                                "Variable '{variable}' is not defined in {context}",
                                { variable, context }
                            )
                            .at(path.clone()),
                        );
                    }
                }

                // Still generate experimental warning for field access if no schema info available
                if parts.len() > 1 {
                    let step_id = format!("variable_{}", var_name);
                    let field = variable.to_string();
                    let reason = "variable field type validation not yet implemented".to_string();
                    diagnostics.add(
                        diagnostic!(
                            DiagnosticKind::UnvalidatedFieldAccess,
                            "Field access '{field}' on step '{step_id}' cannot be validated: {reason}",
                            { step_id, field, reason }
                        )
                        .at(path.clone())
                        .experimental(),
                    );
                }
            } else {
                // No variable schema defined - add warning
                diagnostics.add(
                    diagnostic!(
                        DiagnosticKind::MissingVariableSchema,
                        "No variable schema defined"
                    )
                    .at(make_path!("variables")),
                );
            }

            // Recursively validate the default expression if present
            if let Some(default_expr) = default {
                validate_value_expr(
                    default_expr,
                    path,
                    available_steps,
                    current_step_id,
                    flow,
                    diagnostics,
                );
            }
        }
        ValueExpr::Array(items) => {
            // Validate each array element
            let mut element_path = path.clone();
            for (index, item) in items.iter().enumerate() {
                element_path.push(index);
                validate_value_expr(
                    item,
                    &element_path,
                    available_steps,
                    current_step_id,
                    flow,
                    diagnostics,
                );
                element_path.pop();
            }
        }
        ValueExpr::Object(fields) => {
            // Recursively validate object fields
            let mut field_path = path.clone();
            for (key, value) in fields {
                field_path.push(key.to_string());
                validate_value_expr(
                    value,
                    &field_path,
                    available_steps,
                    current_step_id,
                    flow,
                    diagnostics,
                );
                field_path.pop();
            }
        }
        ValueExpr::If {
            condition,
            then,
            else_expr,
        } => {
            // Validate condition
            let mut condition_path = path.clone();
            condition_path.push("condition");
            validate_value_expr(
                condition,
                &condition_path,
                available_steps,
                current_step_id,
                flow,
                diagnostics,
            );

            // Validate then branch
            let mut then_path = path.clone();
            then_path.push("then");
            validate_value_expr(
                then,
                &then_path,
                available_steps,
                current_step_id,
                flow,
                diagnostics,
            );

            // Validate else branch if present
            if let Some(else_val) = else_expr {
                let mut else_path = path.clone();
                else_path.push("else");
                validate_value_expr(
                    else_val,
                    &else_path,
                    available_steps,
                    current_step_id,
                    flow,
                    diagnostics,
                );
            }
        }
        ValueExpr::Coalesce { values } => {
            // Validate each coalesce value
            let mut value_path = path.clone();
            for (index, value) in values.iter().enumerate() {
                value_path.push(index);
                validate_value_expr(
                    value,
                    &value_path,
                    available_steps,
                    current_step_id,
                    flow,
                    diagnostics,
                );
                value_path.pop();
            }
        }
        // Literals and escaped literals are always valid
        ValueExpr::Literal(_) | ValueExpr::EscapedLiteral { .. } => {}
    }
}
