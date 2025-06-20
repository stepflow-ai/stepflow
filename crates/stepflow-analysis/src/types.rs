use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use stepflow_core::workflow::{Flow, FlowHash};

use crate::tracker::{Dependencies, DependencyTracker};

/// High-level analysis of a flow that supports both frontend consumption and execution
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FlowAnalysis {
    /// The workflow hash for this analysis
    pub flow_hash: FlowHash,
    /// The workflow reference
    pub flow: Arc<Flow>,
    /// Step-by-step analysis keyed by step ID for easy lookup
    pub steps: IndexMap<String, StepAnalysis>,
    /// Dependencies for the workflow output.
    pub output_depends: ValueDependencies,
    /// Validation results
    pub validation_errors: Vec<ValidationMessage>,
    pub validation_warnings: Vec<ValidationMessage>,
    /// Pre-built dependency graph for execution (not serialized)
    #[serde(skip)]
    pub dependencies: Arc<Dependencies>,
}

impl FlowAnalysis {
    /// Get the step index for a step ID
    pub fn get_step_index(&self, step_id: &str) -> Option<usize> {
        self.steps.get_index_of(step_id)
    }

    /// Create a dependency tracker for execution
    pub fn new_dependency_tracker(&self) -> DependencyTracker {
        DependencyTracker::new(self.dependencies.clone())
    }
}

/// Analysis for a single step
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StepAnalysis {
    /// Input dependencies for this step
    pub input_depends: ValueDependencies,
    /// Optional skip condition dependency
    pub skip_if_depend: Option<Dependency>,
}

/// How a step receives its input data
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ValueDependencies {
    /// Step input is an object with named fields
    Object(IndexMap<String, HashSet<Dependency>>),
    /// Step input is not an object (single value, array, etc.)
    Other(HashSet<Dependency>),
}

impl ValueDependencies {
    /// Iterator over all dependencies of this value.
    pub fn dependencies(&self) -> Box<dyn Iterator<Item = &Dependency> + '_> {
        match self {
            ValueDependencies::Object(map) => Box::new(map.values().flatten()),
            ValueDependencies::Other(set) => Box::new(set.iter()),
        }
    }

    /// Iterator over all steps this value depends on.
    pub fn step_dependencies(&self) -> impl Iterator<Item = &str> {
        self.dependencies().filter_map(Dependency::step_id)
    }
}

/// Source of input data for a step
#[derive(
    Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema, Hash, PartialEq, Eq,
)]
#[serde(rename_all = "camelCase")]
pub enum Dependency {
    /// Comes from workflow input
    #[serde(rename_all = "camelCase")]
    FlowInput {
        /// Optional field path within workflow input
        field: Option<String>,
    },
    /// Comes from another step's output
    #[serde(rename_all = "camelCase")]
    StepOutput {
        /// Which step produces this data
        step_id: String,
        /// Optional field path within step output
        field: Option<String>,
        /// If true, the step_id may be skipped and this step still executed.
        optional: bool,
    },
}

impl Dependency {
    /// Return the step this dependency is on, if any.
    pub fn step_id(&self) -> Option<&str> {
        match self {
            Dependency::FlowInput { .. } => None,
            Dependency::StepOutput { step_id, .. } => Some(step_id),
        }
    }
}

/// Validation error in the flow
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidationMessage {
    pub message: String,
    pub step_id: Option<String>,
    pub field: Option<String>,
}
