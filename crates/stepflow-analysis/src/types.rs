use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use stepflow_core::workflow::{Flow, FlowHash};

use crate::tracker::{Dependencies, DependencyTracker};

/// Represents a dependency between two steps in a workflow
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct StepDependency {
    /// Index of the step that depends on another step.
    pub step_index: usize,
    /// Index of the step that this step depends on
    pub depends_on_step_index: usize,
    /// Optional source path within the dependency step's output
    pub src_path: Option<String>,
    /// Where the dependency is used in the dependent step.
    pub dst_field: DestinationField,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DestinationField {
    /// The dependency is used to determine if the step should be skipped.
    SkipIf,
    /// The dependency is used as the complete input to the step.
    Input,
    /// The dependency is used in a specific top-level field of the step's input.
    InputField(String),
}

/// Results of analyzing a workflow for dependencies
#[derive(Debug, Clone)]
pub struct WorkflowAnalysis {
    /// The workflow hash for this analysis
    pub workflow_hash: FlowHash,
    /// All step dependencies found in the workflow
    pub dependencies: Vec<StepDependency>,
    /// Map from step ID to step index for efficient lookup
    pub step_id_to_index: HashMap<String, usize>,
    /// The workflow reference for step names and count
    pub flow: Arc<Flow>,
    /// Pre-computed dependencies for efficient tracker creation
    pub computed_dependencies: Arc<Dependencies>,
}

impl WorkflowAnalysis {
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

    /// Create a new dependency tracker from this analysis
    /// This is very efficient as it just clones the pre-computed Arc<Dependencies>
    pub fn new_dependency_tracker(&self) -> DependencyTracker {
        DependencyTracker::new(self.computed_dependencies.clone())
    }
}
