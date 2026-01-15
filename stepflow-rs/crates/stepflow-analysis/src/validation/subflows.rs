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

use stepflow_core::ValueExpr;
use stepflow_core::workflow::{Flow, Step};

use crate::Diagnostics;
use crate::Path;
use crate::Result;

/// Validate subflows appearing as literals within the flow.
///
/// Currently, subflows can appear as the `data` field of `put_blob` steps
/// when `blob_type` is set to `"flow"`.
pub fn validate_literal_subflows(flow: &Flow, diagnostics: &mut Diagnostics) -> Result<()> {
    let mut path = Path::new();
    path.push("steps");
    for (step_index, step) in flow.steps().iter().enumerate() {
        path.push(step_index);
        validate_step_for_subflows(step, &mut path, diagnostics)?;
        path.pop();
    }
    Ok(())
}

/// Check if a step is a put_blob step with blob_type: flow
fn validate_step_for_subflows(
    step: &Step,
    path: &mut Path,
    diagnostics: &mut Diagnostics,
) -> Result<()> {
    // Only check put_blob steps
    let component_path = step.component.path();
    if component_path != "/builtin/put_blob" && component_path != "/put_blob" {
        return Ok(());
    }

    // Check if blob_type is "flow" and validate the data field
    if let ValueExpr::Object(fields) = &step.input {
        let is_flow_blob = fields.iter().find(|(k, _)| k == "blob_type").is_some_and(
            |(_, bt)| matches!(bt, ValueExpr::Literal(v) if v.as_str() == Some("flow")),
        );

        if is_flow_blob && let Some((_, data_expr)) = fields.iter().find(|(k, _)| k == "data") {
            path.push("input");
            path.push("data");
            validate_flow_literal(data_expr, path, diagnostics)?;
            path.pop();
            path.pop();
        }
    }
    Ok(())
}

fn validate_subflow(path: &Path, flow: &Flow, diagnostics: &mut Diagnostics) -> Result<()> {
    // Validate the subflow. Paths will be *within* the flow.
    let mut subflow_diagnostics = super::validate(flow)?;

    // Update paths to include the path to the subflow.
    for diagnostic in subflow_diagnostics.iter_mut() {
        diagnostic.path.prepend(path);
    }
    // Add the diagnostics to the enclosing diagnostics.
    diagnostics.extend(subflow_diagnostics);
    Ok(())
}

/// Validate a value expression that should contain a flow literal
fn validate_flow_literal(
    value_expr: &ValueExpr,
    path: &mut Path,
    diagnostics: &mut Diagnostics,
) -> Result<()> {
    match value_expr {
        ValueExpr::Literal(literal) => {
            // Parse the literal as a flow
            let flow: Flow = match serde_json::from_value(literal.clone()) {
                Ok(flow) => flow,
                Err(e) => {
                    diagnostics.add(
                        crate::DiagnosticMessage::InvalidSubflowLiteral {
                            error: format!("Failed to parse subflow: {}", e),
                        },
                        path.clone(),
                    );
                    return Ok(());
                }
            };
            validate_subflow(path, &flow, diagnostics)?;
        }
        ValueExpr::EscapedLiteral { literal } => {
            path.push("$literal");
            // Parse the escaped literal as a flow
            let flow: Flow = match serde_json::from_value(literal.clone()) {
                Ok(flow) => flow,
                Err(e) => {
                    diagnostics.add(
                        crate::DiagnosticMessage::InvalidSubflowLiteral {
                            error: format!("Failed to parse subflow: {}", e),
                        },
                        path.clone(),
                    );
                    path.pop();
                    return Ok(());
                }
            };
            validate_subflow(path, &flow, diagnostics)?;
            path.pop();
        }
        // References and other expressions can't be validated statically
        _ => {}
    }
    Ok(())
}
