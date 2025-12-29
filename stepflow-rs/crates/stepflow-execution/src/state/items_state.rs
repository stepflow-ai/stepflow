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

//! Cross-item execution coordinator.
//!
//! [`ItemsState`] manages execution state across multiple items, coordinating
//! task discovery and completion. It supports dynamic growth for nested flow
//! evaluation where sub-flows are added as new items during execution.

use std::collections::HashMap;
use std::sync::Arc;

use stepflow_core::FlowResult;
use stepflow_core::values::ValueRef;
use stepflow_core::workflow::Flow;

use super::item_state::ItemState;
use crate::task::Task;

/// Coordinates execution state across multiple items.
///
/// ItemsState manages a collection of [`ItemState`] instances, one per item
/// in the batch. It provides methods for discovering ready tasks across all
/// items and recording task completions.
///
/// The state can grow dynamically via [`add_item`](Self::add_item), which
/// enables nested flow evaluation where sub-flows are added as new items.
pub struct ItemsState {
    /// Per-item execution state. Can grow dynamically.
    items: Vec<ItemState>,
    /// Shared variables across all items (e.g., API keys, environment config).
    variables: Arc<HashMap<String, ValueRef>>,
    /// Count of incomplete items (for O(1) is_all_complete check).
    /// Decremented when an item transitions from incomplete to complete.
    incomplete_count: usize,
}

impl ItemsState {
    /// Create a new ItemsState with the given items.
    ///
    /// All items share the same variables.
    pub fn new(variables: HashMap<String, ValueRef>) -> Self {
        Self {
            items: Vec::new(),
            variables: Arc::new(variables),
            incomplete_count: 0,
        }
    }

    /// Create an ItemsState with a single item.
    pub fn single(flow: Arc<Flow>, input: ValueRef, variables: HashMap<String, ValueRef>) -> Self {
        let mut state = Self::new(variables);
        state.add_item(flow, input);
        state
    }

    /// Create an ItemsState with multiple items using the same flow.
    pub fn batch(
        flow: Arc<Flow>,
        inputs: Vec<ValueRef>,
        variables: HashMap<String, ValueRef>,
    ) -> Self {
        let mut state = Self::new(variables);
        for input in inputs {
            state.add_item(flow.clone(), input);
        }
        state
    }

    /// Add a new item to the batch.
    ///
    /// Returns the index of the newly added item.
    /// This enables dynamic growth for nested flow evaluation.
    ///
    /// Note: Newly added items are "trivially complete" (no needed steps).
    /// The incomplete count is updated when `initialize_item` marks steps as needed.
    pub fn add_item(&mut self, flow: Arc<Flow>, input: ValueRef) -> u32 {
        let item_index = self.items.len() as u32;
        let item_state = ItemState::new(flow, input, self.variables.clone());
        self.items.push(item_state);
        item_index
    }

    /// Get the number of items.
    pub fn item_count(&self) -> u32 {
        self.items.len() as u32
    }

    /// Get read access to an item's state.
    ///
    /// # Panics
    ///
    /// Panics if `item_index` is out of bounds.
    pub fn item(&self, item_index: u32) -> &ItemState {
        &self.items[item_index as usize]
    }

    /// Get mutable access to an item's state.
    ///
    /// # Panics
    ///
    /// Panics if `item_index` is out of bounds.
    pub fn item_mut(&mut self, item_index: u32) -> &mut ItemState {
        &mut self.items[item_index as usize]
    }

    /// Get all tasks that are ready to execute across all items.
    pub fn all_ready_tasks(&self) -> Vec<Task> {
        let mut tasks = Vec::new();
        for (item_index, item) in self.items.iter().enumerate() {
            for step_index in item.ready_steps().iter() {
                tasks.push(Task::new(item_index as u32, step_index));
            }
        }
        tasks
    }

    /// Get ready tasks for a specific step across all items.
    ///
    /// Useful for breadth-first scheduling where we want to batch
    /// component calls for the same step.
    #[cfg(test)]
    pub fn ready_tasks_for_step(&self, step_index: usize) -> Vec<Task> {
        let mut tasks = Vec::new();
        for (item_index, item) in self.items.iter().enumerate() {
            let ready = item.ready_steps();
            if ready.contains(step_index) {
                tasks.push(Task::new(item_index as u32, step_index));
            }
        }
        tasks
    }

    /// Mark a task as currently executing.
    pub fn mark_executing(&mut self, task: Task) {
        self.item_mut(task.item_index)
            .mark_executing(task.step_index);
    }

    /// Mark a step as needed for an item, updating the incomplete counter.
    ///
    /// This is the preferred way to mark steps as needed (vs direct `mark_needed` on item)
    /// because it properly tracks the incomplete count for O(1) completion checks.
    #[cfg(test)]
    pub fn mark_step_needed(&mut self, item_index: u32, step_index: usize) {
        let item = self.item_mut(item_index);
        let was_complete = item.is_complete();
        item.mark_needed(step_index);
        if was_complete && !item.is_complete() {
            self.incomplete_count += 1;
        }
    }

    /// Complete a task and record its result.
    ///
    /// Returns the set of newly unblocked step indices for this item.
    pub fn complete_task(&mut self, task: Task, result: FlowResult) -> Vec<usize> {
        let item = self.item_mut(task.item_index);
        let was_complete = item.is_complete();
        let newly_unblocked = item.mark_completed(task.step_index, result);
        let is_complete = item.is_complete();

        // Update incomplete count if item just became complete
        if !was_complete && is_complete {
            self.incomplete_count -= 1;
        }

        newly_unblocked.iter().collect()
    }

    /// Complete a task and return the tasks that are now ready to execute.
    ///
    /// This is a convenience method that combines:
    /// 1. Recording the task completion in state
    /// 2. Re-evaluating newly unblocked steps to discover dependencies
    /// 3. Filtering to find which unblocked steps are now ready
    ///
    /// Returns the list of tasks that are now ready to execute.
    pub fn complete_task_and_get_ready(&mut self, task: Task, result: FlowResult) -> Vec<Task> {
        // Step 1: Complete the task
        let newly_unblocked = self.complete_task(task, result);

        // Step 2: Re-evaluate newly unblocked steps to discover dependencies
        let item = self.item_mut(task.item_index);
        for step_idx in &newly_unblocked {
            item.add_or_update_needed(*step_idx);
        }

        // Step 3: Get newly ready tasks
        let item = self.item(task.item_index);
        newly_unblocked
            .into_iter()
            .filter(|step_idx| item.ready_steps().contains(*step_idx))
            .map(|step_idx| Task::new(task.item_index, step_idx))
            .collect()
    }

    /// Check if all items have completed execution.
    ///
    /// This is O(1) using a counter, not O(n) iterating items.
    pub fn is_all_complete(&self) -> bool {
        self.incomplete_count == 0
    }

    /// Check if a specific item has completed execution.
    pub fn is_item_complete(&self, item_index: u32) -> bool {
        self.item(item_index).is_complete()
    }

    /// Check if there are any ready tasks across all items.
    ///
    /// Used for deadlock detection: if not complete and no ready tasks
    /// (and nothing in-flight), we have a deadlock.
    pub fn has_ready_tasks(&self) -> bool {
        self.items.iter().any(|item| !item.ready_steps().is_empty())
    }

    /// Get the result of a completed item.
    ///
    /// Returns the final output of the item's flow by evaluating the flow's
    /// output expression, or None if not complete.
    pub fn get_item_result(&self, item_index: u32) -> Option<FlowResult> {
        let item = self.item(item_index);
        if !item.is_complete() {
            return None;
        }
        // Evaluate the flow's output expression using the item as context
        Some(item.flow().output().resolve(item))
    }

    /// Initialize needed steps for an item based on output and must_execute.
    ///
    /// This discovers which steps need to execute by evaluating the flow's
    /// output expression and marking must_execute steps. Updates the incomplete
    /// counter if the item becomes incomplete (has needed steps).
    pub fn initialize_item(&mut self, item_index: u32) -> Vec<Task> {
        let item = self.item_mut(item_index);
        let was_complete = item.is_complete();
        let flow = item.flow().clone();

        // Evaluate output's needed_steps
        let output_needs = flow.output().needed_steps(item);
        for idx in output_needs.iter() {
            item.add_or_update_needed(idx);
        }

        // Add must_execute steps
        for (idx, step) in flow.steps().iter().enumerate() {
            if step.must_execute() {
                item.add_or_update_needed(idx);
            }
        }

        // Update incomplete count if item became incomplete
        if was_complete && !item.is_complete() {
            self.incomplete_count += 1;
        }

        // Return ready tasks for this item
        self.item(item_index)
            .ready_steps()
            .iter()
            .map(|step_index| Task::new(item_index, step_index))
            .collect()
    }

    /// Initialize all items and return all ready tasks.
    pub fn initialize_all(&mut self) -> Vec<Task> {
        let mut tasks = Vec::new();
        for item_index in 0..self.item_count() {
            tasks.extend(self.initialize_item(item_index));
        }
        tasks
    }

    /// Add a step to the needed set for an item.
    ///
    /// This is for debug mode's "task addition control" where steps are
    /// added incrementally rather than all at once via `initialize_item`.
    ///
    /// Properly tracks the incomplete count for O(1) completion checks.
    /// Returns the newly needed step indices (including dependencies).
    pub fn add_needed(&mut self, item_index: u32, step_index: usize) -> Vec<usize> {
        let item = self.item_mut(item_index);
        let was_complete = item.is_complete();

        // Add the step and discover dependencies
        let newly_needed = item.add_or_update_needed(step_index);

        // Update incomplete count if item became incomplete
        if was_complete && !item.is_complete() {
            self.incomplete_count += 1;
        }

        newly_needed.iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use stepflow_core::ValueExpr;
    use stepflow_core::values::StepContext as _;
    use stepflow_core::workflow::{FlowBuilder, StepBuilder};

    fn success_result() -> FlowResult {
        FlowResult::Success(ValueRef::new(json!(null)))
    }

    fn create_simple_flow(step_names: &[&str]) -> Flow {
        let steps = step_names
            .iter()
            .map(|name| StepBuilder::mock_step(*name).build())
            .collect::<Vec<_>>();
        FlowBuilder::test_flow().steps(steps).build()
    }

    #[test]
    fn test_single_item() {
        let flow = Arc::new(create_simple_flow(&["step1", "step2"]));
        let mut state = ItemsState::single(flow, ValueRef::new(json!({})), HashMap::new());

        assert_eq!(state.item_count(), 1);

        // Before marking steps as needed, state is trivially complete
        assert!(state.is_all_complete());

        // Mark steps as needed (using mark_step_needed for proper counter tracking)
        state.mark_step_needed(0, 0);
        state.mark_step_needed(0, 1);

        // Now it's not complete
        assert!(!state.is_all_complete());
    }

    #[test]
    fn test_batch_items() {
        let flow = Arc::new(create_simple_flow(&["step1"]));
        let inputs = vec![
            ValueRef::new(json!({"x": 1})),
            ValueRef::new(json!({"x": 2})),
            ValueRef::new(json!({"x": 3})),
        ];
        let state = ItemsState::batch(flow, inputs, HashMap::new());

        assert_eq!(state.item_count(), 3);
    }

    #[test]
    fn test_add_item_dynamically() {
        let mut state = ItemsState::new(HashMap::new());
        assert_eq!(state.item_count(), 0);

        let flow1 = Arc::new(create_simple_flow(&["step1"]));
        let idx1 = state.add_item(flow1.clone(), ValueRef::new(json!(1)));
        assert_eq!(idx1, 0);
        assert_eq!(state.item_count(), 1);

        let flow2 = Arc::new(create_simple_flow(&["a", "b", "c"]));
        let idx2 = state.add_item(flow2, ValueRef::new(json!(2)));
        assert_eq!(idx2, 1);
        assert_eq!(state.item_count(), 2);

        // Items can have different flows
        assert_eq!(state.item(0).num_steps(), 1);
        assert_eq!(state.item(1).num_steps(), 3);
    }

    #[test]
    fn test_all_ready_tasks() {
        let flow = Arc::new(create_simple_flow(&["step1", "step2"]));
        let mut state = ItemsState::batch(
            flow,
            vec![ValueRef::new(json!(1)), ValueRef::new(json!(2))],
            HashMap::new(),
        );

        // Mark all steps as needed for both items
        for item_index in 0..2 {
            let item = state.item_mut(item_index);
            item.mark_needed(0);
            item.mark_needed(1);
        }

        let ready = state.all_ready_tasks();
        // 2 items x 2 steps = 4 tasks
        assert_eq!(ready.len(), 4);

        // Should contain tasks for both items and both steps
        assert!(ready.contains(&Task::new(0, 0)));
        assert!(ready.contains(&Task::new(0, 1)));
        assert!(ready.contains(&Task::new(1, 0)));
        assert!(ready.contains(&Task::new(1, 1)));
    }

    #[test]
    fn test_ready_tasks_for_step() {
        let flow = Arc::new(create_simple_flow(&["step1", "step2"]));
        let mut state = ItemsState::batch(
            flow,
            vec![ValueRef::new(json!(1)), ValueRef::new(json!(2))],
            HashMap::new(),
        );

        // Mark step 0 as needed for both items
        for item_index in 0..2 {
            state.item_mut(item_index).mark_needed(0);
        }

        let ready_step0 = state.ready_tasks_for_step(0);
        assert_eq!(ready_step0.len(), 2);
        assert!(ready_step0.contains(&Task::new(0, 0)));
        assert!(ready_step0.contains(&Task::new(1, 0)));

        let ready_step1 = state.ready_tasks_for_step(1);
        assert!(ready_step1.is_empty());
    }

    #[test]
    fn test_complete_task() {
        let flow = Arc::new(create_simple_flow(&["step1", "step2"]));
        let mut state = ItemsState::single(flow, ValueRef::new(json!({})), HashMap::new());

        {
            let item = state.item_mut(0);
            item.mark_needed(0);
            item.mark_needed(1);
            // step2 waits on step1
            let mut waiting = bit_set::BitSet::new();
            waiting.insert(0);
            item.set_waiting(1, waiting);
        }

        // Only step1 should be ready
        let ready = state.all_ready_tasks();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0], Task::new(0, 0));

        // Complete step1
        state.mark_executing(Task::new(0, 0));
        let newly_unblocked = state.complete_task(Task::new(0, 0), success_result());
        assert_eq!(newly_unblocked, vec![1]); // step2 unblocked

        // Now step2 should be ready
        let ready = state.all_ready_tasks();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0], Task::new(0, 1));
    }

    #[test]
    fn test_is_all_complete() {
        let flow = Arc::new(create_simple_flow(&["step1"]));
        let mut state = ItemsState::batch(
            flow,
            vec![ValueRef::new(json!(1)), ValueRef::new(json!(2))],
            HashMap::new(),
        );

        // Initially complete (no steps needed)
        assert!(state.is_all_complete());

        // Mark step as needed for item 0 (using mark_step_needed for proper counter tracking)
        state.mark_step_needed(0, 0);
        assert!(!state.is_all_complete());

        // Complete item 0
        state.complete_task(Task::new(0, 0), success_result());
        assert!(state.is_all_complete());

        // Mark step as needed for item 1 (using mark_step_needed for proper counter tracking)
        state.mark_step_needed(1, 0);
        assert!(!state.is_all_complete());

        // Complete item 1
        state.complete_task(Task::new(1, 0), success_result());
        assert!(state.is_all_complete());
    }

    #[test]
    fn test_shared_variables() {
        let mut variables = HashMap::new();
        variables.insert("api_key".to_string(), ValueRef::new(json!("secret")));

        let flow = Arc::new(create_simple_flow(&["step1"]));
        let state = ItemsState::batch(
            flow,
            vec![ValueRef::new(json!(1)), ValueRef::new(json!(2))],
            variables,
        );

        // Both items should see the same variables
        assert_eq!(
            state.item(0).get_variable("api_key").unwrap().as_ref(),
            &json!("secret")
        );
        assert_eq!(
            state.item(1).get_variable("api_key").unwrap().as_ref(),
            &json!("secret")
        );
    }

    #[test]
    fn test_initialize_item() {
        // Create a flow where output depends on step2, which depends on step1
        let flow = Arc::new(
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
                ])
                .output(ValueExpr::Step {
                    step: "step2".to_string(),
                    path: Default::default(),
                })
                .build(),
        );

        let mut state = ItemsState::single(flow, ValueRef::new(json!({})), HashMap::new());

        // Initialize - should discover step1 and step2 as needed
        let ready_tasks = state.initialize_item(0);

        // Only step1 should be ready (step2 depends on it)
        assert_eq!(ready_tasks.len(), 1);
        assert_eq!(ready_tasks[0], Task::new(0, 0));

        // Both steps should be needed
        let item = state.item(0);
        assert!(item.is_needed(0));
        assert!(item.is_needed(1));
    }

    #[test]
    fn test_get_item_result_evaluates_output_expression() {
        use stepflow_core::values::JsonPath;

        // Create a flow where output references step1 with a JSONPath
        let flow = Arc::new(
            FlowBuilder::test_flow()
                .steps(vec![
                    StepBuilder::mock_step("step1")
                        .input(ValueExpr::Input {
                            input: Default::default(),
                        })
                        .build(),
                    StepBuilder::mock_step("step2")
                        .input(ValueExpr::Input {
                            input: Default::default(),
                        })
                        .build(),
                ])
                .output(ValueExpr::Step {
                    step: "step1".to_string(),
                    path: JsonPath::from("$.result"),
                })
                .build(),
        );

        let mut state = ItemsState::single(
            flow,
            ValueRef::new(json!({"input": "data"})),
            HashMap::new(),
        );

        // Mark steps as needed (use mark_step_needed for proper counter tracking)
        state.mark_step_needed(0, 0);
        state.mark_step_needed(0, 1);

        // Item is not complete yet
        assert!(state.get_item_result(0).is_none());

        // Complete step1 with a result containing nested data
        state.complete_task(
            Task::new(0, 0),
            FlowResult::Success(ValueRef::new(
                json!({"result": "hello world", "extra": "ignored"}),
            )),
        );

        // Still not complete (step2 not done)
        assert!(state.get_item_result(0).is_none());

        // Complete step2 with different data
        state.complete_task(
            Task::new(0, 1),
            FlowResult::Success(ValueRef::new(json!({"result": "step2 output"}))),
        );

        // Now it's complete - get_item_result should evaluate output expression
        // which references step1's result.result field
        let result = state.get_item_result(0).expect("Should be complete");
        match result {
            FlowResult::Success(value) => {
                assert_eq!(value.as_ref(), &json!("hello world"));
            }
            _ => panic!("Expected success, got {:?}", result),
        }
    }

    #[test]
    fn test_get_item_result_with_input_expression() {
        use stepflow_core::values::JsonPath;

        // Create a flow where output is $input (passes through the input)
        let flow = Arc::new(
            FlowBuilder::test_flow()
                .steps(vec![
                    StepBuilder::mock_step("step1")
                        .input(ValueExpr::Input {
                            input: Default::default(),
                        })
                        .build(),
                ])
                .output(ValueExpr::Input {
                    input: JsonPath::from("$.message"),
                })
                .build(),
        );

        let mut state = ItemsState::single(
            flow,
            ValueRef::new(json!({"message": "hello from input"})),
            HashMap::new(),
        );

        // Mark step as needed (use mark_step_needed for proper counter tracking)
        state.mark_step_needed(0, 0);

        // Complete step1
        state.complete_task(
            Task::new(0, 0),
            FlowResult::Success(ValueRef::new(json!(null))),
        );

        // Output should be from the input path
        let result = state.get_item_result(0).expect("Should be complete");
        match result {
            FlowResult::Success(value) => {
                assert_eq!(value.as_ref(), &json!("hello from input"));
            }
            _ => panic!("Expected success, got {:?}", result),
        }
    }
}
