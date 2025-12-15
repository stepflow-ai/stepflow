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

use crate::{DiagnosticMessage, Diagnostics, Path, validation::path::make_path};

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

    // Validate skip condition references
    if let Some(skip_if) = &step.skip_if {
        path.push("skip_if".to_string());
        validate_value_expr(
            skip_if,
            &path,
            available_steps,
            &step.id,
            flow,
            diagnostics,
        );
        path.pop();
    }

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
        ValueExpr::Step { step, path: field_path } => {
            // Check for self-reference
            if current_step_id == step {
                diagnostics.add(
                    DiagnosticMessage::SelfReference {
                        step_id: step.clone(),
                    },
                    path.clone(),
                );
                return;
            }

            // Check if step exists and is available (defined earlier)
            if !available_steps.contains(step) {
                diagnostics.add(
                    DiagnosticMessage::UndefinedStepReference {
                        from_step: Some(current_step_id.to_string()),
                        referenced_step: step.clone(),
                    },
                    path.clone(),
                );
                return;
            }

            // Generate ignored diagnostic about potential field access issues (when we don't have schema info)
            if !field_path.is_empty() {
                diagnostics.add(
                    DiagnosticMessage::UnvalidatedFieldAccess {
                        step_id: step.clone(),
                        field: field_path.to_string(),
                        reason: "no output schema available".to_string(),
                    },
                    path.clone(),
                );
            }
        }
        ValueExpr::Input { input } => {
            // Workflow input reference is always valid
            // Generate ignored diagnostic about unvalidated field access on workflow input
            if !input.is_empty() {
                diagnostics.add(
                    DiagnosticMessage::UnvalidatedFieldAccess {
                        step_id: "workflow_input".to_string(),
                        field: input.to_string(),
                        reason: "no input schema available".to_string(),
                    },
                    path.clone(),
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
                        diagnostics.add(
                            DiagnosticMessage::UndefinedRequiredVariable {
                                variable: var_name.to_string(),
                                context: format!("step '{}'", current_step_id),
                            },
                            path.clone(),
                        );
                    } else {
                        diagnostics.add(
                            DiagnosticMessage::UndefinedVariable {
                                variable: var_name.to_string(),
                                context: format!("step '{}'", current_step_id),
                            },
                            path.clone(),
                        );
                    }
                }

                // Still generate warning for field access if no schema info available
                if parts.len() > 1 {
                    diagnostics.add(
                        DiagnosticMessage::UnvalidatedFieldAccess {
                            step_id: format!("variable_{}", var_name),
                            field: variable.to_string(),
                            reason: "variable field type validation not yet implemented".to_string(),
                        },
                        path.clone(),
                    );
                }
            } else {
                // No variable schema defined - add warning
                diagnostics.add(
                    DiagnosticMessage::MissingVariableSchema,
                    make_path!("variables"),
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
        ValueExpr::If { condition, then, else_expr } => {
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