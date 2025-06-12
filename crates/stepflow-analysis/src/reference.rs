use stepflow_core::workflow::{BaseRef, Expr, ValueRef};

use crate::{types::{ExpressionReference, ReferenceLocation}, Result};

/// Extract all references from a ValueRef with their locations
pub fn extract_references_from_value_ref(
    value_ref: &ValueRef,
    location: ReferenceLocation,
) -> Result<Vec<ExpressionReference>> {
    let mut references = Vec::new();
    extract_references_recursive(value_ref.as_ref(), location, "", &mut references)?;
    Ok(references)
}

/// Extract references from an expression
pub fn extract_reference_from_expr(
    expr: &Expr,
    location: ReferenceLocation,
) -> Option<ExpressionReference> {
    if let Expr::Ref { from, path, .. } = expr {
        Some(ExpressionReference {
            base_ref: from.clone(),
            path: path.clone(),
            location,
        })
    } else {
        None
    }
}

/// Recursively extract references from a JSON value
fn extract_references_recursive(
    value: &serde_json::Value,
    location: ReferenceLocation,
    current_path: &str,
    references: &mut Vec<ExpressionReference>,
) -> Result<()> {
    match value {
        serde_json::Value::Object(fields) => {
            // Try to parse as an expression first
            if let Ok(expr) = serde_json::from_value::<Expr>(value.clone()) {
                match expr {
                    Expr::Ref { from, path, .. } => {
                        // It's a reference - extract it
                        let location_with_path = if current_path.is_empty() {
                            location
                        } else {
                            ReferenceLocation {
                                field_path: Some(current_path.to_string()),
                                ..location
                            }
                        };

                        references.push(ExpressionReference {
                            base_ref: from,
                            path,
                            location: location_with_path,
                        });
                        return Ok(()); // Don't recurse into reference objects
                    }
                    Expr::Literal(_) => {
                        // Check if this is actually a $literal wrapper
                        if fields.contains_key("$literal") {
                            // It's a literal wrapper - don't recurse
                            return Ok(());
                        }
                        // Otherwise, it's just a regular object that happened to parse as Literal
                        // Fall through to recursive processing
                    }
                }
            }

            // Not an expression - process object recursively
            for (k, v) in fields {
                let field_path = if current_path.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", current_path, k)
                };
                extract_references_recursive(v, location.clone(), &field_path, references)?;
            }
        }
        serde_json::Value::Array(arr) => {
            // Process array recursively
            for (index, v) in arr.iter().enumerate() {
                let array_path = if current_path.is_empty() {
                    format!("[{}]", index)
                } else {
                    format!("{}[{}]", current_path, index)
                };
                extract_references_recursive(v, location.clone(), &array_path, references)?;
            }
        }
        _ => {
            // Primitive values cannot contain references
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use stepflow_core::workflow::WorkflowRef;
    use serde_json::json;

    #[test]
    fn test_extract_simple_reference() {
        let value_ref = ValueRef::new(json!({
            "$from": {"step": "step1"},
            "path": "output"
        }));

        let location = ReferenceLocation::new("test_input").with_step_id("step2");

        let references = extract_references_from_value_ref(&value_ref, location).unwrap();

        assert_eq!(references.len(), 1);
        assert_eq!(
            references[0].base_ref,
            BaseRef::Step {
                step: "step1".to_string()
            }
        );
        assert_eq!(references[0].path, Some("output".to_string()));
        assert_eq!(references[0].location.context, "test_input");
        assert_eq!(references[0].location.step_id, Some("step2".to_string()));
    }

    #[test]
    fn test_extract_nested_references() {
        let value_ref = ValueRef::new(json!({
            "input1": {"$from": {"step": "step1"}, "path": "result"},
            "input2": {"$from": {"workflow": "input"}, "path": "user_data"},
            "static": "value",
            "nested": {
                "dynamic": {"$from": {"step": "step2"}}
            }
        }));

        let location = ReferenceLocation::new("step_input").with_step_id("step3");

        let references = extract_references_from_value_ref(&value_ref, location).unwrap();

        assert_eq!(references.len(), 3);

        // Find each reference by its field path
        let input1_ref = references
            .iter()
            .find(|r| r.location.field_path.as_deref() == Some("input1"))
            .unwrap();
        assert_eq!(
            input1_ref.base_ref,
            BaseRef::Step {
                step: "step1".to_string()
            }
        );
        assert_eq!(input1_ref.path, Some("result".to_string()));

        let input2_ref = references
            .iter()
            .find(|r| r.location.field_path.as_deref() == Some("input2"))
            .unwrap();
        assert_eq!(input2_ref.base_ref, BaseRef::Workflow(WorkflowRef::Input));

        let nested_ref = references
            .iter()
            .find(|r| r.location.field_path.as_deref() == Some("nested.dynamic"))
            .unwrap();
        assert_eq!(
            nested_ref.base_ref,
            BaseRef::Step {
                step: "step2".to_string()
            }
        );
    }

    #[test]
    fn test_literal_wrapper_ignored() {
        let value_ref = ValueRef::new(json!({
            "normal_ref": {"$from": {"step": "step1"}},
            "literal_wrapper": {"$literal": {"$from": "not_a_real_reference"}},
            "static": "value"
        }));

        let location = ReferenceLocation::new("test");
        let references = extract_references_from_value_ref(&value_ref, location).unwrap();

        // Should only find the normal reference, not the one inside $literal
        assert_eq!(references.len(), 1);
        assert_eq!(
            references[0].base_ref,
            BaseRef::Step {
                step: "step1".to_string()
            }
        );
        assert_eq!(
            references[0].location.field_path,
            Some("normal_ref".to_string())
        );
    }
}