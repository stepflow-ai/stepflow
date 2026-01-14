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

//! Per-run execution state.
//!
//! This module provides [`RunState`], which encapsulates the execution state for a single run
//! (either a top-level run or a sub-flow). Each run has its own `ItemsState` for tracking
//! step execution progress.
//!
//! Completion notification is handled through the unified `StateStore::wait_for_completion()`
//! mechanism rather than per-run watch channels.

use std::collections::HashMap;
use std::sync::Arc;

use stepflow_core::BlobId;
use stepflow_core::values::ValueRef;
use stepflow_core::workflow::Flow;
use uuid::Uuid;

use crate::state::ItemsState;
use crate::task::Task;

/// Per-run execution state.
///
/// `RunState` encapsulates the in-memory execution state for a single run, including
/// tracking which steps are ready, executing, or complete.
///
/// For completion notification, use `StateStore::wait_for_completion(run_id)` which
/// provides a unified notification mechanism for all runs.
pub struct RunState {
    /// Unique ID for this run.
    run_id: Uuid,
    /// Flow ID (content hash) for this run's workflow.
    flow_id: BlobId,
    /// Root run ID (same as run_id for top-level runs).
    root_run_id: Uuid,
    /// Parent run ID (None for top-level runs).
    parent_run_id: Option<Uuid>,
    /// Per-item execution state.
    items_state: ItemsState,
}

impl RunState {
    /// Create a new top-level run state.
    pub fn new(
        run_id: Uuid,
        flow_id: BlobId,
        flow: Arc<Flow>,
        inputs: Vec<ValueRef>,
        variables: HashMap<String, ValueRef>,
    ) -> Self {
        let items_state = ItemsState::batch(run_id, flow, inputs, variables);

        Self {
            run_id,
            flow_id,
            root_run_id: run_id, // Top-level run is its own root
            parent_run_id: None,
            items_state,
        }
    }

    /// Create a new sub-flow run state.
    pub fn new_subflow(
        run_id: Uuid,
        flow_id: BlobId,
        root_run_id: Uuid,
        parent_run_id: Uuid,
        flow: Arc<Flow>,
        inputs: Vec<ValueRef>,
        variables: HashMap<String, ValueRef>,
    ) -> Self {
        let items_state = ItemsState::batch(run_id, flow, inputs, variables);

        Self {
            run_id,
            flow_id,
            root_run_id,
            parent_run_id: Some(parent_run_id),
            items_state,
        }
    }

    /// Get the run ID.
    pub fn run_id(&self) -> Uuid {
        self.run_id
    }

    /// Get the flow ID.
    pub fn flow_id(&self) -> &BlobId {
        &self.flow_id
    }

    /// Get the root run ID.
    pub fn root_run_id(&self) -> Uuid {
        self.root_run_id
    }

    /// Get the parent run ID (None for top-level runs).
    pub fn parent_run_id(&self) -> Option<Uuid> {
        self.parent_run_id
    }

    /// Get a reference to the items state.
    pub fn items_state(&self) -> &ItemsState {
        &self.items_state
    }

    /// Get a mutable reference to the items state.
    pub fn items_state_mut(&mut self) -> &mut ItemsState {
        &mut self.items_state
    }

    /// Check if the run is complete (all items done).
    pub fn is_complete(&self) -> bool {
        self.items_state.incomplete() == 0
    }

    /// Get the number of items in this run.
    pub fn item_count(&self) -> u32 {
        self.items_state.item_count()
    }

    /// Initialize all items and return ready tasks.
    pub fn initialize_all(&mut self) -> Vec<Task> {
        self.items_state.initialize_all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use stepflow_core::ValueExpr;
    use stepflow_core::workflow::{FlowBuilder, StepBuilder};

    fn create_test_flow() -> Arc<Flow> {
        Arc::new(
            FlowBuilder::test_flow()
                .steps(vec![
                    StepBuilder::mock_step("step1")
                        .input(ValueExpr::Input {
                            input: Default::default(),
                        })
                        .build(),
                ])
                .output(ValueExpr::Step {
                    step: "step1".to_string(),
                    path: Default::default(),
                })
                .build(),
        )
    }

    #[test]
    fn test_run_state_creation() {
        let run_id = Uuid::now_v7();
        let flow = create_test_flow();
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let inputs = vec![ValueRef::new(json!({"x": 1}))];

        let mut run_state = RunState::new(run_id, flow_id.clone(), flow, inputs, HashMap::new());

        assert_eq!(run_state.run_id(), run_id);
        assert_eq!(run_state.root_run_id(), run_id);
        assert!(run_state.parent_run_id().is_none());
        assert_eq!(run_state.flow_id(), &flow_id);
        assert_eq!(run_state.item_count(), 1);

        // Before initialization, items are "trivially complete" (no needed steps)
        assert!(run_state.is_complete());

        // After initialization, items have needed steps
        let tasks = run_state.initialize_all();
        assert!(!tasks.is_empty());
        assert!(!run_state.is_complete());
    }

    #[test]
    fn test_subflow_run_state_creation() {
        let run_id = Uuid::now_v7();
        let root_run_id = Uuid::now_v7();
        let parent_run_id = Uuid::now_v7();
        let flow = create_test_flow();
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let inputs = vec![ValueRef::new(json!({"x": 1}))];

        let run_state = RunState::new_subflow(
            run_id,
            flow_id.clone(),
            root_run_id,
            parent_run_id,
            flow,
            inputs,
            HashMap::new(),
        );

        assert_eq!(run_state.run_id(), run_id);
        assert_eq!(run_state.root_run_id(), root_run_id);
        assert_eq!(run_state.parent_run_id(), Some(parent_run_id));
    }

    #[test]
    fn test_items_state_access() {
        let run_id = Uuid::now_v7();
        let flow = create_test_flow();
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let inputs = vec![
            ValueRef::new(json!({"x": 1})),
            ValueRef::new(json!({"x": 2})),
        ];

        let mut run_state = RunState::new(run_id, flow_id, flow, inputs, HashMap::new());

        // Test immutable access
        assert_eq!(run_state.items_state().item_count(), 2);

        // Test mutable access - initialize items
        let tasks = run_state.items_state_mut().initialize_all();
        assert_eq!(tasks.len(), 2); // One task per item (single step flow)
    }
}
