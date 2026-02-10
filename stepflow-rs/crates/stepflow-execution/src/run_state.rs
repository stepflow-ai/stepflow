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

use stepflow_core::values::ValueRef;
use stepflow_core::workflow::Flow;
use stepflow_core::{BlobId, FlowResult};
use stepflow_state::JournalEvent;
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
    /// The flow being executed.
    flow: Arc<Flow>,
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
        let items_state = ItemsState::batch(run_id, flow.clone(), inputs, variables);

        Self {
            run_id,
            flow_id,
            root_run_id: run_id, // Top-level run is its own root
            parent_run_id: None,
            flow,
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
        let items_state = ItemsState::batch(run_id, flow.clone(), inputs, variables);

        Self {
            run_id,
            flow_id,
            root_run_id,
            parent_run_id: Some(parent_run_id),
            flow,
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

    /// Get the flow for this run.
    pub fn flow(&self) -> Arc<Flow> {
        self.flow.clone()
    }

    /// Get the inputs for this run.
    ///
    /// Collects the input from each item.
    pub fn inputs(&self) -> Vec<ValueRef> {
        (0..self.items_state.item_count())
            .map(|i| self.items_state.item(i).input().clone())
            .collect()
    }

    /// Get the variables for this run.
    pub fn variables(&self) -> &HashMap<String, ValueRef> {
        self.items_state.variables()
    }

    /// Initialize all items and return ready tasks.
    pub fn initialize_all(&mut self) -> Vec<Task> {
        self.items_state.initialize_all()
    }

    // =========================================================================
    // Unified State Transitions (used by both execution and recovery)
    // =========================================================================

    /// Apply a journal event to update state.
    ///
    /// This is the unified state transition method used by both:
    /// - Execution: create event → apply → persist to journal
    /// - Recovery: load events → apply in sequence
    ///
    /// Returns the tasks that became ready as a result of this event.
    ///
    /// # Event Handling
    ///
    /// - `RunInitialized`: Sets up needed steps for each item with their dependencies.
    ///   Returns all initially ready tasks.
    /// - `TaskCompleted`: Marks a task as complete, updates dependencies, discovers
    ///   newly ready tasks. Returns the newly ready tasks.
    /// - Other events (`RunCreated`, `RunCompleted`, `StepsUnblocked`, `ItemCompleted`):
    ///   Informational or handled elsewhere. Returns empty.
    ///
    /// Note: Subflow events are identified by their own `RunCreated` event with
    /// `parent_run_id` set. Since all events for an execution tree share the same
    /// journal (keyed by `root_run_id`), subflow events are filtered by `run_id`
    /// during replay.
    pub fn apply_event(&mut self, event: &JournalEvent) -> Vec<Task> {
        match event {
            JournalEvent::RunCreated { .. } => {
                // RunCreated is handled at construction time, not via apply_event
                Vec::new()
            }
            JournalEvent::RunInitialized { needed_steps } => {
                let mut tasks = Vec::new();
                for item_steps in needed_steps {
                    tasks.extend(self.items_state.initialize_item_with_steps(
                        item_steps.item_index,
                        &item_steps.step_indices,
                    ));
                }
                tasks
            }
            JournalEvent::TaskCompleted {
                item_index,
                step_index,
                result,
            } => {
                let task = Task::new(self.run_id, *item_index, *step_index);
                self.items_state
                    .complete_task_and_get_ready(task, result.clone())
            }
            JournalEvent::StepsUnblocked { .. } => {
                // Informational - state update happens as part of TaskCompleted
                Vec::new()
            }
            JournalEvent::ItemCompleted { .. } => {
                // Informational - completion is implicit from task completions
                Vec::new()
            }
            JournalEvent::RunCompleted { .. } => {
                // Terminal state - no state update needed
                Vec::new()
            }
        }
    }

    // =========================================================================
    // Recovery Methods
    // =========================================================================

    /// Reconstruct a run state from recovery data.
    ///
    /// This creates a RunState from the data found in journal entries:
    /// - RunCreated provides flow_id, inputs, variables, parent_run_id
    /// - TaskCompleted entries provide which steps completed and their results
    ///
    /// After applying all completions, call `initialize_all()` to discover
    /// which steps are still needed and get the ready tasks for resumption.
    ///
    /// # Arguments
    /// * `run_id` - The run ID to reconstruct
    /// * `flow_id` - The flow ID (blob hash) from RunCreated
    /// * `flow` - The loaded flow (must be loaded from blob store first)
    /// * `inputs` - Original inputs from RunCreated
    /// * `variables` - Original variables from RunCreated
    /// * `completed_tasks` - List of (item_index, step_index, result) from TaskCompleted entries
    /// * `root_run_id` - Root run ID (from RunCreated or the run tree)
    /// * `parent_run_id` - Parent run ID from RunCreated (None for root runs)
    #[allow(clippy::too_many_arguments)]
    pub fn from_recovery_data(
        run_id: Uuid,
        flow_id: BlobId,
        flow: Arc<Flow>,
        inputs: Vec<ValueRef>,
        variables: HashMap<String, ValueRef>,
        completed_tasks: &[(u32, usize, FlowResult)],
        root_run_id: Uuid,
        parent_run_id: Option<Uuid>,
    ) -> Self {
        // Create the run state
        let mut state = if let Some(parent) = parent_run_id {
            Self::new_subflow(
                run_id,
                flow_id,
                root_run_id,
                parent,
                flow,
                inputs,
                variables,
            )
        } else {
            Self::new(run_id, flow_id, flow, inputs, variables)
        };

        // Apply completed tasks (before initialization)
        // This marks steps as completed so initialize_all() knows to skip them
        for (item_index, step_index, result) in completed_tasks {
            state
                .items_state
                .apply_completed(*item_index, *step_index, result.clone());
        }

        state
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

    fn create_chain_flow() -> Arc<Flow> {
        // step1 -> step2 -> step3 (each depends on previous)
        Arc::new(
            FlowBuilder::test_flow()
                .steps(vec![
                    StepBuilder::mock_step("step1")
                        .input(ValueExpr::Input {
                            input: Default::default(),
                        })
                        .build(),
                    StepBuilder::mock_step("step2")
                        .input(ValueExpr::Step {
                            step: "step1".to_string(),
                            path: Default::default(),
                        })
                        .build(),
                    StepBuilder::mock_step("step3")
                        .input(ValueExpr::Step {
                            step: "step2".to_string(),
                            path: Default::default(),
                        })
                        .build(),
                ])
                .output(ValueExpr::Step {
                    step: "step3".to_string(),
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

    // =========================================================================
    // Recovery Tests
    // =========================================================================

    #[test]
    fn test_from_recovery_data_no_completed() {
        // Recovery with no completed tasks - should behave like fresh run
        let run_id = Uuid::now_v7();
        let flow = create_chain_flow();
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let inputs = vec![ValueRef::new(json!({"x": 1}))];

        let mut run_state = RunState::from_recovery_data(
            run_id,
            flow_id.clone(),
            flow,
            inputs,
            HashMap::new(),
            &[], // No completed tasks
            run_id,
            None,
        );

        assert_eq!(run_state.run_id(), run_id);
        assert_eq!(run_state.flow_id(), &flow_id);
        assert!(run_state.parent_run_id().is_none());

        // Initialize should discover all steps needed
        let ready = run_state.initialize_all();

        // Only step1 should be ready (step2/3 depend on previous)
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].step_index, 0);
        assert!(!run_state.is_complete());
    }

    #[test]
    fn test_from_recovery_data_partial_completion() {
        // Recovery with step1 completed - step2 should be ready
        let run_id = Uuid::now_v7();
        let flow = create_chain_flow();
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let inputs = vec![ValueRef::new(json!({"x": 1}))];

        let completed_tasks = vec![(
            0u32,   // item_index
            0usize, // step_index (step1)
            FlowResult::Success(ValueRef::new(json!({"result": 1}))),
        )];

        let mut run_state = RunState::from_recovery_data(
            run_id,
            flow_id.clone(),
            flow,
            inputs,
            HashMap::new(),
            &completed_tasks,
            run_id,
            None,
        );

        // Initialize discovers remaining steps
        let ready = run_state.initialize_all();

        // step1 is already completed, step2 should be ready
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].step_index, 1); // step2
        assert!(!run_state.is_complete());

        // Verify step1 result is available
        let item = run_state.items_state().item(0);
        assert!(item.is_completed(0));
        let result = item.get_step_result(0).unwrap();
        assert!(matches!(result, FlowResult::Success(_)));
    }

    #[test]
    fn test_from_recovery_data_mostly_complete() {
        // Recovery with step1 and step2 completed - step3 should be ready
        let run_id = Uuid::now_v7();
        let flow = create_chain_flow();
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let inputs = vec![ValueRef::new(json!({"x": 1}))];

        let completed_tasks = vec![
            (
                0u32,
                0usize,
                FlowResult::Success(ValueRef::new(json!({"result": 1}))),
            ),
            (
                0u32,
                1usize,
                FlowResult::Success(ValueRef::new(json!({"result": 2}))),
            ),
        ];

        let mut run_state = RunState::from_recovery_data(
            run_id,
            flow_id.clone(),
            flow,
            inputs,
            HashMap::new(),
            &completed_tasks,
            run_id,
            None,
        );

        let ready = run_state.initialize_all();

        // Only step3 should be ready
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].step_index, 2); // step3
        assert!(!run_state.is_complete());
    }

    #[test]
    fn test_from_recovery_data_fully_complete() {
        // Recovery with all steps completed - run should be complete
        let run_id = Uuid::now_v7();
        let flow = create_chain_flow();
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let inputs = vec![ValueRef::new(json!({"x": 1}))];

        let completed_tasks = vec![
            (
                0u32,
                0usize,
                FlowResult::Success(ValueRef::new(json!({"result": 1}))),
            ),
            (
                0u32,
                1usize,
                FlowResult::Success(ValueRef::new(json!({"result": 2}))),
            ),
            (
                0u32,
                2usize,
                FlowResult::Success(ValueRef::new(json!({"result": 3}))),
            ),
        ];

        let mut run_state = RunState::from_recovery_data(
            run_id,
            flow_id.clone(),
            flow,
            inputs,
            HashMap::new(),
            &completed_tasks,
            run_id,
            None,
        );

        let ready = run_state.initialize_all();

        // No tasks should be ready - all complete
        assert!(ready.is_empty());
        assert!(run_state.is_complete());
    }

    #[test]
    fn test_from_recovery_data_subflow() {
        // Recovery of a subflow run
        let run_id = Uuid::now_v7();
        let root_run_id = Uuid::now_v7();
        let parent_run_id = Uuid::now_v7();
        let flow = create_test_flow();
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let inputs = vec![ValueRef::new(json!({"x": 1}))];

        let run_state = RunState::from_recovery_data(
            run_id,
            flow_id.clone(),
            flow,
            inputs,
            HashMap::new(),
            &[],
            root_run_id,
            Some(parent_run_id),
        );

        assert_eq!(run_state.run_id(), run_id);
        assert_eq!(run_state.root_run_id(), root_run_id);
        assert_eq!(run_state.parent_run_id(), Some(parent_run_id));
    }

    #[test]
    fn test_from_recovery_data_multiple_items() {
        // Recovery with multiple items, different completion states
        let run_id = Uuid::now_v7();
        let flow = create_test_flow();
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let inputs = vec![
            ValueRef::new(json!({"x": 1})),
            ValueRef::new(json!({"x": 2})),
            ValueRef::new(json!({"x": 3})),
        ];

        // Item 0: completed, Item 1: not completed, Item 2: completed
        let completed_tasks = vec![
            (
                0u32,
                0usize,
                FlowResult::Success(ValueRef::new(json!({"result": 1}))),
            ),
            (
                2u32,
                0usize,
                FlowResult::Success(ValueRef::new(json!({"result": 3}))),
            ),
        ];

        let mut run_state = RunState::from_recovery_data(
            run_id,
            flow_id.clone(),
            flow,
            inputs,
            HashMap::new(),
            &completed_tasks,
            run_id,
            None,
        );

        let ready = run_state.initialize_all();

        // Only item 1 should have a ready task
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].item_index, 1);
        assert_eq!(ready[0].step_index, 0);
        assert!(!run_state.is_complete());
    }

    // =========================================================================
    // Unified apply_event Tests
    // =========================================================================

    #[test]
    fn test_apply_event_run_initialized() {
        // Test that apply_event with RunInitialized produces the same state as initialize_all
        let run_id = Uuid::now_v7();
        let flow = create_chain_flow();
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let inputs = vec![ValueRef::new(json!({"x": 1}))];

        // Execution path: create state, call initialize_all, record needed steps
        let mut exec_state = RunState::new(
            run_id,
            flow_id.clone(),
            flow.clone(),
            inputs.clone(),
            HashMap::new(),
        );
        let exec_ready = exec_state.initialize_all();
        let needed_steps = exec_state.items_state().needed_steps_for_journal();

        // Recovery path: create state, apply RunInitialized event
        let mut recovery_state = RunState::new(
            run_id,
            flow_id.clone(),
            flow.clone(),
            inputs.clone(),
            HashMap::new(),
        );
        let recovery_ready = recovery_state.apply_event(&JournalEvent::RunInitialized {
            needed_steps: needed_steps.clone(),
        });

        // Both paths should produce the same ready tasks
        assert_eq!(exec_ready.len(), recovery_ready.len());
        for (e, r) in exec_ready.iter().zip(recovery_ready.iter()) {
            assert_eq!(e.item_index, r.item_index);
            assert_eq!(e.step_index, r.step_index);
        }

        // Both paths should have the same needed steps
        let exec_needed = exec_state.items_state().item(0).needed_step_indices();
        let recovery_needed = recovery_state.items_state().item(0).needed_step_indices();
        assert_eq!(exec_needed, recovery_needed);
    }

    #[test]
    fn test_apply_event_task_completed() {
        // Test that apply_event with TaskCompleted matches complete_task_and_get_ready
        let run_id = Uuid::now_v7();
        let flow = create_chain_flow();
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let inputs = vec![ValueRef::new(json!({"x": 1}))];

        // Set up both states identically
        let mut exec_state = RunState::new(
            run_id,
            flow_id.clone(),
            flow.clone(),
            inputs.clone(),
            HashMap::new(),
        );
        exec_state.initialize_all();

        let mut recovery_state = RunState::new(
            run_id,
            flow_id.clone(),
            flow.clone(),
            inputs.clone(),
            HashMap::new(),
        );
        recovery_state.initialize_all();

        // Complete step 0 in both paths
        let task = crate::task::Task::new(run_id, 0, 0);
        let result = FlowResult::Success(ValueRef::new(json!({"result": 1})));

        // Execution path
        let exec_ready = exec_state
            .items_state_mut()
            .complete_task_and_get_ready(task, result.clone());

        // Recovery path via apply_event
        let recovery_ready = recovery_state.apply_event(&JournalEvent::TaskCompleted {
            item_index: 0,
            step_index: 0,
            result: result.clone(),
        });

        // Both should produce the same ready tasks
        assert_eq!(exec_ready.len(), recovery_ready.len());
        for (e, r) in exec_ready.iter().zip(recovery_ready.iter()) {
            assert_eq!(e.step_index, r.step_index);
        }

        // Both should have step 0 completed
        assert!(exec_state.items_state().item(0).is_completed(0));
        assert!(recovery_state.items_state().item(0).is_completed(0));

        // Both should have step 1 ready (since step 0 is complete)
        assert!(
            exec_state
                .items_state()
                .item(0)
                .schedulable_steps()
                .contains(1)
        );
        assert!(
            recovery_state
                .items_state()
                .item(0)
                .schedulable_steps()
                .contains(1)
        );
    }

    #[test]
    fn test_apply_events_full_execution_sequence() {
        // Verify that applying a full sequence of events produces the same final state
        // as running through the execution path
        let run_id = Uuid::now_v7();
        let flow = create_chain_flow(); // step1 -> step2 -> step3
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let inputs = vec![ValueRef::new(json!({"x": 1}))];

        // ===== Execution Path =====
        let mut exec_state = RunState::new(
            run_id,
            flow_id.clone(),
            flow.clone(),
            inputs.clone(),
            HashMap::new(),
        );

        // Initialize
        let initial_ready = exec_state.initialize_all();
        let needed_steps = exec_state.items_state().needed_steps_for_journal();
        assert_eq!(initial_ready.len(), 1); // step1 ready

        // Record events as we execute
        let mut events: Vec<JournalEvent> = vec![JournalEvent::RunInitialized {
            needed_steps: needed_steps.clone(),
        }];

        // Execute step1
        let task0 = crate::task::Task::new(run_id, 0, 0);
        let result0 = FlowResult::Success(ValueRef::new(json!({"step1": "done"})));
        let ready_after_0 = exec_state
            .items_state_mut()
            .complete_task_and_get_ready(task0, result0.clone());
        events.push(JournalEvent::TaskCompleted {
            item_index: 0,
            step_index: 0,
            result: result0.clone(),
        });
        assert_eq!(ready_after_0.len(), 1); // step2 ready

        // Execute step2
        let task1 = crate::task::Task::new(run_id, 0, 1);
        let result1 = FlowResult::Success(ValueRef::new(json!({"step2": "done"})));
        let ready_after_1 = exec_state
            .items_state_mut()
            .complete_task_and_get_ready(task1, result1.clone());
        events.push(JournalEvent::TaskCompleted {
            item_index: 0,
            step_index: 1,
            result: result1.clone(),
        });
        assert_eq!(ready_after_1.len(), 1); // step3 ready

        // Execute step3
        let task2 = crate::task::Task::new(run_id, 0, 2);
        let result2 = FlowResult::Success(ValueRef::new(json!({"step3": "done"})));
        let ready_after_2 = exec_state
            .items_state_mut()
            .complete_task_and_get_ready(task2, result2.clone());
        events.push(JournalEvent::TaskCompleted {
            item_index: 0,
            step_index: 2,
            result: result2.clone(),
        });
        assert!(ready_after_2.is_empty()); // no more tasks

        assert!(exec_state.is_complete());

        // ===== Recovery Path =====
        let mut recovery_state = RunState::new(
            run_id,
            flow_id.clone(),
            flow.clone(),
            inputs.clone(),
            HashMap::new(),
        );

        // Apply all events
        events.iter().for_each(|e| {
            recovery_state.apply_event(e);
        });

        // After all events, both states should be equivalent
        assert!(recovery_state.is_complete());
        // No schedulable tasks when complete
        assert!(
            recovery_state
                .items_state()
                .item(0)
                .schedulable_steps()
                .is_empty()
        );

        // Verify all steps are completed with correct results
        let exec_item = exec_state.items_state().item(0);
        let recovery_item = recovery_state.items_state().item(0);

        for step_idx in 0..3 {
            assert!(exec_item.is_completed(step_idx));
            assert!(recovery_item.is_completed(step_idx));

            let exec_result = exec_item.get_step_result(step_idx).unwrap();
            let recovery_result = recovery_item.get_step_result(step_idx).unwrap();

            // Results should match
            match (exec_result, recovery_result) {
                (FlowResult::Success(e), FlowResult::Success(r)) => {
                    assert_eq!(e.as_ref(), r.as_ref());
                }
                _ => panic!("Results should both be Success"),
            }
        }
    }

    #[test]
    fn test_apply_events_partial_recovery() {
        // Test recovery from a partial execution (crash after step1)
        let run_id = Uuid::now_v7();
        let flow = create_chain_flow(); // step1 -> step2 -> step3
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let inputs = vec![ValueRef::new(json!({"x": 1}))];

        // Simulate journal entries from a crashed execution that completed step1
        let events = vec![
            JournalEvent::RunInitialized {
                needed_steps: vec![stepflow_state::ItemSteps {
                    item_index: 0,
                    step_indices: vec![0, 1, 2],
                }],
            },
            JournalEvent::TaskCompleted {
                item_index: 0,
                step_index: 0,
                result: FlowResult::Success(ValueRef::new(json!({"step1": "done"}))),
            },
            // Crash here - step2 never started
        ];

        // Recover by applying events
        let mut recovery_state = RunState::new(
            run_id,
            flow_id.clone(),
            flow.clone(),
            inputs.clone(),
            HashMap::new(),
        );
        events.iter().for_each(|e| {
            recovery_state.apply_event(e);
        });

        // After recovery, step2 should be schedulable (step1 completed, step2 unblocked)
        let schedulable: Vec<_> = recovery_state
            .items_state()
            .item(0)
            .schedulable_steps()
            .iter()
            .collect();
        assert_eq!(schedulable.len(), 1);
        assert_eq!(schedulable[0], 1); // step index 1

        // Step1 should be completed
        let item = recovery_state.items_state().item(0);
        assert!(item.is_completed(0));
        assert!(!item.is_completed(1));
        assert!(!item.is_completed(2));

        // Run should not be complete
        assert!(!recovery_state.is_complete());
    }
}
