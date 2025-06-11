/// Shared analysis functionality for workflows
/// 
/// This module provides common functionality for extracting dependencies and edges
/// from workflow expressions. It unifies the logic currently spread across:
/// - Edge extraction in Flow::analyze_ports_and_edges
/// - Dependency analysis in stepflow-execution::dependency_analysis
/// - Value resolution patterns in stepflow-execution::value_resolver

use std::collections::HashSet;

use super::{BaseRef, Expr, ValueRef, Edge, EdgeEndpoint, WorkflowRef};

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
) -> Vec<ExpressionReference> {
    let mut references = Vec::new();
    extract_references_recursive(value_ref.as_ref(), location, "", &mut references);
    references
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
) {
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
                        return; // Don't recurse into reference objects
                    }
                    Expr::Literal(_) => {
                        // Check if this is actually a $literal wrapper
                        if fields.contains_key("$literal") {
                            // It's a literal wrapper - don't recurse
                            return;
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
                extract_references_recursive(v, location.clone(), &field_path, references);
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
                extract_references_recursive(v, location.clone(), &array_path, references);
            }
        }
        _ => {
            // Primitive values cannot contain references
        }
    }
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

/// Convert references to edges for workflow graph analysis
pub fn references_to_edges(
    references: &[ExpressionReference],
    edge_counter: &mut usize,
) -> Vec<Edge> {
    references
        .iter()
        .map(|reference| {
            let source_endpoint = match &reference.base_ref {
                BaseRef::Workflow(_) => EdgeEndpoint::workflow_input("input"),
                BaseRef::Step { step } => EdgeEndpoint::new(step.clone(), "result"),
            };

            let target_step_id = reference.location.step_id.as_deref().unwrap_or("workflow");
            let target_port = reference.location.field_path.as_deref().unwrap_or("input");
            let target_endpoint = EdgeEndpoint::new(target_step_id, target_port);

            let edge_id = format!("edge_{}", edge_counter);
            *edge_counter += 1;

            let mut edge = Edge::new(edge_id, source_endpoint, target_endpoint);
            if let Some(path) = &reference.path {
                edge = edge.with_path(path.clone());
            }

            edge
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
        
        let location = ReferenceLocation::new("test_input")
            .with_step_id("step2");
        
        let references = extract_references_from_value_ref(&value_ref, location);
        
        assert_eq!(references.len(), 1);
        assert_eq!(references[0].base_ref, BaseRef::Step { step: "step1".to_string() });
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
        
        let location = ReferenceLocation::new("step_input")
            .with_step_id("step3");
        
        let references = extract_references_from_value_ref(&value_ref, location);
        
        assert_eq!(references.len(), 3);
        
        // Find each reference by its field path
        let input1_ref = references.iter().find(|r| r.location.field_path.as_deref() == Some("input1")).unwrap();
        assert_eq!(input1_ref.base_ref, BaseRef::Step { step: "step1".to_string() });
        assert_eq!(input1_ref.path, Some("result".to_string()));
        
        let input2_ref = references.iter().find(|r| r.location.field_path.as_deref() == Some("input2")).unwrap();
        assert_eq!(input2_ref.base_ref, BaseRef::Workflow(WorkflowRef::Input));
        
        let nested_ref = references.iter().find(|r| r.location.field_path.as_deref() == Some("nested.dynamic")).unwrap();
        assert_eq!(nested_ref.base_ref, BaseRef::Step { step: "step2".to_string() });
    }

    #[test]
    fn test_extract_step_dependencies() {
        let references = vec![
            ExpressionReference {
                base_ref: BaseRef::Step { step: "step1".to_string() },
                path: None,
                location: ReferenceLocation::new("test"),
            },
            ExpressionReference {
                base_ref: BaseRef::Workflow(WorkflowRef::Input),
                path: None,
                location: ReferenceLocation::new("test"),
            },
            ExpressionReference {
                base_ref: BaseRef::Step { step: "step2".to_string() },
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
    fn test_references_to_edges() {
        let references = vec![
            ExpressionReference {
                base_ref: BaseRef::Step { step: "step1".to_string() },
                path: Some("output".to_string()),
                location: ReferenceLocation::new("step_input")
                    .with_step_id("step2")
                    .with_field_path("input.data"),
            },
            ExpressionReference {
                base_ref: BaseRef::Workflow(WorkflowRef::Input),
                path: Some("user".to_string()),
                location: ReferenceLocation::new("step_input")
                    .with_step_id("step1")
                    .with_field_path("input.user_data"),
            },
        ];
        
        let mut edge_counter = 0;
        let edges = references_to_edges(&references, &mut edge_counter);
        
        assert_eq!(edges.len(), 2);
        
        // Check step-to-step edge
        let step_edge = edges.iter().find(|e| e.source.step_id == "step1").unwrap();
        assert_eq!(step_edge.source.port_name, "result");
        assert_eq!(step_edge.target.step_id, "step2");
        assert_eq!(step_edge.target.port_name, "input.data");
        assert_eq!(step_edge.path, Some("output".to_string()));
        
        // Check workflow-to-step edge
        let workflow_edge = edges.iter().find(|e| e.source.step_id == "workflow").unwrap();
        assert_eq!(workflow_edge.source.port_name, "input");
        assert_eq!(workflow_edge.target.step_id, "step1");
        assert_eq!(workflow_edge.target.port_name, "input.user_data");
        assert_eq!(workflow_edge.path, Some("user".to_string()));
    }

    #[test]
    fn test_literal_wrapper_ignored() {
        let value_ref = ValueRef::new(json!({
            "normal_ref": {"$from": {"step": "step1"}},
            "literal_wrapper": {"$literal": {"$from": "not_a_real_reference"}},
            "static": "value"
        }));
        
        let location = ReferenceLocation::new("test");
        let references = extract_references_from_value_ref(&value_ref, location);
        
        // Should only find the normal reference, not the one inside $literal
        assert_eq!(references.len(), 1);
        assert_eq!(references[0].base_ref, BaseRef::Step { step: "step1".to_string() });
        assert_eq!(references[0].location.field_path, Some("normal_ref".to_string()));
    }
}