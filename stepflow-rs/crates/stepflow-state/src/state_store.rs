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

//! Run creation parameters.
//!
//! This module provides the [`CreateRunParams`] struct used when creating
//! workflow runs via [`MetadataStore::create_run`](crate::MetadataStore::create_run).

use std::collections::HashMap;

use stepflow_core::{
    BlobId,
    workflow::{ValueRef, WorkflowOverrides},
};
use uuid::Uuid;

/// Parameters for creating a new workflow run.
///
/// A run can have one or more input items. Use `vec![input]` for single-item runs.
#[derive(Debug, Clone)]
pub struct CreateRunParams {
    /// Unique identifier for the workflow execution
    pub run_id: Uuid,
    /// The flow ID (blob hash) of the workflow to execute
    pub flow_id: BlobId,
    /// Optional workflow name for display and organization
    pub workflow_name: Option<String>,
    /// Input data for the workflow execution (one per item)
    pub inputs: Vec<ValueRef>,
    /// Runtime overrides for step inputs
    pub overrides: WorkflowOverrides,
    /// Variables to provide for variable references in the workflow
    pub variables: HashMap<String, ValueRef>,
    /// Root run ID for this execution tree.
    ///
    /// For top-level runs, this equals `run_id`.
    /// For sub-flows, this is the original run that started the tree.
    pub root_run_id: Uuid,
    /// Parent run ID if this is a sub-flow.
    ///
    /// None for top-level runs, Some(parent_id) for sub-flows.
    pub parent_run_id: Option<Uuid>,
    /// The orchestrator that owns this run.
    ///
    /// Set when the run is created and updated during recovery.
    /// None means the run is orphaned (no orchestrator owns it).
    pub orchestrator_id: Option<String>,
}

impl CreateRunParams {
    /// Create run params with the given inputs for a root (top-level) run.
    ///
    /// For single-item runs, pass `vec![input]`.
    /// Sets `root_run_id` equal to `run_id` and `parent_run_id` to None.
    pub fn new(run_id: Uuid, flow_id: BlobId, inputs: Vec<ValueRef>) -> Self {
        Self {
            run_id,
            flow_id,
            inputs,
            workflow_name: None,
            overrides: WorkflowOverrides::default(),
            variables: HashMap::new(),
            root_run_id: run_id,
            parent_run_id: None,
            orchestrator_id: None,
        }
    }

    /// Create run params for a sub-flow within an existing run tree.
    ///
    /// Inherits the `root_run_id` from the parent context and sets this run's
    /// parent to the provided `parent_run_id`.
    pub fn new_subflow(
        run_id: Uuid,
        flow_id: BlobId,
        inputs: Vec<ValueRef>,
        root_run_id: Uuid,
        parent_run_id: Uuid,
    ) -> Self {
        Self {
            run_id,
            flow_id,
            inputs,
            workflow_name: None,
            overrides: WorkflowOverrides::default(),
            variables: HashMap::new(),
            root_run_id,
            parent_run_id: Some(parent_run_id),
            orchestrator_id: None,
        }
    }

    /// Get the number of items in this run.
    pub fn item_count(&self) -> usize {
        self.inputs.len()
    }

    /// Get the input for a single-item run.
    /// Panics if this is a multi-item run.
    pub fn input(&self) -> &ValueRef {
        assert_eq!(self.inputs.len(), 1, "Expected single-item run");
        &self.inputs[0]
    }
}
