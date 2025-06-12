use std::collections::{HashMap, HashSet};
use stepflow_core::workflow::{BaseRef, Flow};

/// Represents a dependency between two steps in a workflow
#[derive(Debug, Clone, PartialEq)]
pub struct StepDependency {
    /// Hash of the workflow this dependency belongs to
    pub workflow_hash: String,
    /// Index of the step that depends on another step
    pub step_index: usize,
    /// Index of the step that this step depends on
    pub depends_on_step_index: usize,
    /// Optional source path within the dependency step's output
    pub src_path: Option<String>,
    /// Optional destination field where the dependency is used
    pub dst_field: Option<String>,
}

/// Results of analyzing a workflow for dependencies
#[derive(Debug, Clone)]
pub struct WorkflowAnalysis {
    /// The workflow hash for this analysis
    pub workflow_hash: String,
    /// All step dependencies found in the workflow
    pub dependencies: Vec<StepDependency>,
    /// Map from step ID to step index for efficient lookup
    pub step_id_to_index: HashMap<String, usize>,
}

impl WorkflowAnalysis {
    /// Create a new workflow analysis
    pub fn new(workflow_hash: String, dependencies: Vec<StepDependency>, flow: &Flow) -> Self {
        let step_id_to_index = flow
            .steps
            .iter()
            .enumerate()
            .map(|(idx, step)| (step.id.clone(), idx))
            .collect();

        Self {
            workflow_hash,
            dependencies,
            step_id_to_index,
        }
    }

    /// Get dependencies for a specific step
    pub fn get_dependencies_for_step(&self, step_index: usize) -> Vec<&StepDependency> {
        self.dependencies
            .iter()
            .filter(|dep| dep.step_index == step_index)
            .collect()
    }

    /// Get the step index for a step ID
    pub fn get_step_index(&self, step_id: &str) -> Option<usize> {
        self.step_id_to_index.get(step_id).copied()
    }
}

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
    use stepflow_core::workflow::WorkflowRef;

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
}