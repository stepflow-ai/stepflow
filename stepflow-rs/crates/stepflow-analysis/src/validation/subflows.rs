use serde_json::Value;
use stepflow_core::values::ValueRef;
use stepflow_core::values::ValueTemplate;
use stepflow_core::values::ValueTemplateRepr;
use stepflow_core::workflow::Expr;
use stepflow_core::workflow::Flow;

use crate::Diagnostics;
use crate::Path;
use crate::Result;

/// Validate subflows appearing as literals within the flow.
pub fn validate_literal_subflows(flow: &Flow, diagnostics: &mut Diagnostics) -> Result<()> {
    // For now, we validate literal subflows appearing in `put_blob` requests.
    // Since we can't detect the `put_blob` component without looking at the
    // plugins, we just use the `schema: "https://stepflow.org/schemas/v1.flow.json"`
    // as the signal that we should validate something as a schema.
    //
    // We only look inside the inputs to steps for now.
    let mut path = Path::new();
    path.push("steps");
    for (step_index, step) in flow.steps().iter().enumerate() {
        path.push(step_index);
        validate_value_template(&step.input, &mut path, diagnostics)?;
        path.pop();
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

const FLOW_SCHEMA_URL: &str = "https://stepflow.org/schemas/v1/flow.json";

fn object_has_flow_schema(fields: &serde_json::Map<String, Value>) -> bool {
    if let Some(schema) = fields.get("schema") {
        schema.as_str() == Some(FLOW_SCHEMA_URL)
    } else {
        false
    }
}

fn validate_value_ref(
    value_ref: &ValueRef,
    path: &mut Path,
    diagnostics: &mut Diagnostics,
) -> Result<()> {
    match value_ref.value() {
        Value::Object(o) if object_has_flow_schema(o) => {
            // Parse the subflow.
            let flow: Flow = match serde_json::from_value(value_ref.clone_value()) {
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
        Value::Object(o) => {
            for (key, item) in o.iter() {
                path.push(key.to_string());
                validate_value_ref(&ValueRef::new(item.clone()), path, diagnostics)?;
                path.pop();
            }
        }
        Value::Array(arr) => {
            for (index, item) in arr.iter().enumerate() {
                path.push(index);
                validate_value_ref(&ValueRef::new(item.clone()), path, diagnostics)?;
                path.pop();
            }
        }
        _ => {}
    };
    Ok(())
}

fn validate_value_template(
    value_template: &ValueTemplate,
    path: &mut Path,
    diagnostics: &mut Diagnostics,
) -> Result<()> {
    match value_template.as_ref() {
        ValueTemplateRepr::Expression(Expr::Literal(literal)) => {
            validate_value_ref(literal, path, diagnostics)?;
        }
        ValueTemplateRepr::Expression(Expr::EscapedLiteral { literal }) => {
            path.push("$literal");
            validate_value_ref(literal, path, diagnostics)?;
            path.pop();
        }
        ValueTemplateRepr::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                path.push(index);
                validate_value_template(item, path, diagnostics)?;
                path.pop();
            }
        }
        ValueTemplateRepr::Object(index_map) => {
            // We only want to validate the sub-flow if it is a literal, in which case
            // it would be parsed as a literal or escaped literal.
            for (key, item) in index_map.iter() {
                path.push(key.to_string());
                validate_value_template(item, path, diagnostics)?;
                path.pop();
            }
        }
        _ => {}
    }
    Ok(())
}
