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

use crate::dependencies::{Dependency, ValueDependencies};
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use stepflow_core::{BlobId, workflow::Flow};

use crate::{
    diagnostics::Diagnostics,
    tracker::{Dependencies, DependenciesBuilder, DependencyTracker},
};

/// Result of workflow analysis including diagnostics
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisResult {
    /// Analysis results (None if fatal diagnostics prevented analysis)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis: Option<FlowAnalysis>,
    /// All diagnostics found during analysis
    pub diagnostics: Diagnostics,
}

impl AnalysisResult {
    /// Create a new analysis result with analysis and diagnostics
    pub fn with_analysis(analysis: FlowAnalysis, diagnostics: Diagnostics) -> Self {
        Self {
            analysis: Some(analysis),
            diagnostics,
        }
    }

    /// Create a new analysis result with only diagnostics (analysis failed)
    pub fn with_diagnostics_only(diagnostics: Diagnostics) -> Self {
        Self {
            analysis: None,
            diagnostics,
        }
    }

    /// Check if analysis was successful
    pub fn has_analysis(&self) -> bool {
        self.analysis.is_some()
    }

    pub fn diagnostics(&self) -> &Diagnostics {
        &self.diagnostics
    }
}

/// High-level analysis of a flow that supports both frontend consumption and execution
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FlowAnalysis {
    /// The workflow ID for this analysis
    pub flow_id: BlobId,
    /// The workflow reference
    pub flow: Arc<Flow>,
    /// Step-by-step analysis keyed by step ID for easy lookup
    pub steps: IndexMap<String, StepAnalysis>,
    /// Dependencies for the workflow output.
    pub output_depends: ValueDependencies,
    /// Pre-built dependency graph for execution (not serialized)
    #[serde(skip, default = "default_dependencies")]
    pub dependencies: Arc<Dependencies>,
}

impl FlowAnalysis {
    /// Get the step index for a step ID
    pub fn get_step_index(&self, step_id: &str) -> Option<usize> {
        self.steps.get_index_of(step_id)
    }

    /// Create a dependency tracker for execution
    pub fn new_dependency_tracker(&self) -> DependencyTracker {
        use bit_set::BitSet;

        let mut tracker = DependencyTracker::new(self.dependencies.clone());

        // Collect step indices that are directly required:
        // 1. All steps that the output depends on
        let mut directly_required: BitSet = self
            .output_depends
            .dependencies()
            .filter_map(|dep| {
                if let crate::dependencies::Dependency::StepOutput { step_id, .. } = dep {
                    self.get_step_index(step_id)
                } else {
                    None
                }
            })
            .collect();

        // 2. All steps marked as must_execute
        for (idx, step) in self.flow.steps().iter().enumerate() {
            if step.must_execute() {
                directly_required.insert(idx);
            }
        }

        // 3. Compute transitive closure: include all dependencies of directly required steps
        let required_steps = self
            .dependencies
            .transitive_dependencies(&directly_required);

        tracker.set_required_steps(required_steps);
        tracker
    }
}

/// Default empty dependencies for deserialization
fn default_dependencies() -> Arc<Dependencies> {
    DependenciesBuilder::new(0).finish()
}

/// Analysis for a single step
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StepAnalysis {
    /// Input dependencies for this step
    pub input_depends: ValueDependencies,
    /// Skip condition dependencies (empty if no skip condition or no dependencies)
    pub skip_if_depends: HashSet<Dependency>,
}
