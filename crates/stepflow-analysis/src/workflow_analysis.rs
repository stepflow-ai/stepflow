use std::collections::HashSet;
use stepflow_core::workflow::{BaseRef, Expr, ValueRef, WorkflowRef};
use error_stack::Result;

use crate::dependency_analysis::AnalysisError;

/// A reference found in a workflow expression
#[derive(Debug, Clone, PartialEq)]
pub struct ExpressionReference {
    /// The base reference (step or workflow)
    pub base_ref: BaseRef,
    /// Optional path for accessing sub-fields
    pub path: Option<String>,
    /// Location where this reference was found (for debugging/tracing)
    pub location: ReferenceLocation,
}

/// Location where a reference was found in the workflow
#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceLocation {
    /// Context where the reference was found (e.g., "step_input", "skip_condition", "workflow_output")
    pub context: String,
    /// Step ID if the reference is within a step's configuration
    pub step_id: Option<String>,
    /// Field path within the context (e.g., "input.name", "output.result")
    pub field_path: Option<String>,
}

impl ReferenceLocation {
    /// Create a new reference location
    pub fn new(context: impl Into<String>) -> Self {
        Self {
            context: context.into(),
            step_id: None,
            field_path: None,
        }
    }

    /// Set the step ID for this location
    pub fn with_step_id(mut self, step_id: impl Into<String>) -> Self {
        self.step_id = Some(step_id.into());
        self
    }

    /// Set the field path for this location
    pub fn with_field_path(mut self, field_path: impl Into<String>) -> Self {
        self.field_path = Some(field_path.into());
        self
    }
}

/// Extract all references from a ValueRef with their locations
pub fn extract_references_from_value_ref(
    value_ref: &ValueRef,
    location: ReferenceLocation,
) -> Result<Vec<ExpressionReference>, AnalysisError> {
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
) -> Result<(), AnalysisError> {
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

/// Extract step dependencies from references (filtering out workflow input references)
pub fn extract_step_dependencies(references: &[ExpressionReference]) -> HashSet<String> {
    references
        .iter()
        .filter_map(|reference| {
            if let BaseRef::Step { step } = &reference.base_ref {
                Some(step.clone())
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn test_extract_step_dependencies() {
        let references = vec![
            ExpressionReference {
                base_ref: BaseRef::Step {
                    step: "step1".to_string(),
                },
                path: None,
                location: ReferenceLocation::new("test"),
            },
            ExpressionReference {
                base_ref: BaseRef::Workflow(WorkflowRef::Input),
                path: None,
                location: ReferenceLocation::new("test"),
            },
            ExpressionReference {
                base_ref: BaseRef::Step {
                    step: "step2".to_string(),
                },
                path: Some("output".to_string()),
                location: ReferenceLocation::new("test"),
            },
        ];

        let dependencies = extract_step_dependencies(&references);

        assert_eq!(dependencies.len(), 2);
        assert!(dependencies.contains("step1"));
        assert!(dependencies.contains("step2"));
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