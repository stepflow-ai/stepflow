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

//! Per-item execution state.
//!
//! [`ItemState`] tracks the execution state of a single item in a batch, including
//! which steps have completed, which are waiting on dependencies, and cached results.
//! Each item has its own flow and step index, supporting heterogeneous flows in a batch
//! (e.g., for nested flow evaluation).

use std::collections::HashMap;
use std::sync::Arc;

use bit_set::BitSet;
use stepflow_core::{
    FlowResult,
    status::StepStatus,
    values::{Secrets, StepContext, ValueRef},
    workflow::{Flow, VariableSchema},
};
use stepflow_dtos::StepStatusInfo;

/// Step index mapping for execution tracking.
///
/// This provides efficient lookups between step IDs and indices.
#[derive(Debug)]
pub struct StepIndex {
    /// Number of steps in the workflow.
    num_steps: usize,
    /// Map from step name to index (for name -> index lookups).
    step_name_to_index: HashMap<String, usize>,
}

impl StepIndex {
    /// Create a step index from a flow's steps.
    pub fn from_flow(flow: &Flow) -> Self {
        let steps = flow.steps();
        let num_steps = steps.len();
        let step_name_to_index: HashMap<String, usize> = steps
            .iter()
            .enumerate()
            .map(|(i, step)| (step.id.clone(), i))
            .collect();

        Self {
            num_steps,
            step_name_to_index,
        }
    }

    /// Get the number of steps.
    pub fn num_steps(&self) -> usize {
        self.num_steps
    }

    /// Get the index of a step by its name.
    pub fn step_index(&self, name: &str) -> Option<usize> {
        self.step_name_to_index.get(name).copied()
    }
}

/// Execution state for a single item in a batch.
///
/// Tracks which steps have completed, which are waiting on dependencies,
/// and caches step results. Each item can have its own flow, supporting
/// nested flow evaluation where sub-flows are added as new items.
///
/// Note: This struct tracks workflow graph state (what CAN run based on
/// dependencies), not runtime state (what IS running). The executor/scheduler
/// separately tracks which tasks are currently in-flight to prevent double-scheduling.
pub struct ItemState {
    /// The flow definition for this item.
    flow: Arc<Flow>,
    /// Step index mapping for ID <-> index lookups.
    step_index: Arc<StepIndex>,
    /// For each step, whether it has completed execution.
    completed: BitSet,
    /// Cached results for completed steps.
    results: Vec<Option<FlowResult>>,
    /// Steps that are dynamically determined to be needed.
    needed: BitSet,
    /// For each step, the set of steps it's waiting on for re-evaluation.
    waiting_on: Vec<BitSet>,
    /// Reverse mapping: for each step, which steps are waiting on it.
    waiters: Vec<BitSet>,
    /// Workflow input value for evaluating `$input` references.
    input: ValueRef,
    /// Variable values for evaluating `$variable` references.
    /// Shared across all items in a batch.
    variables: Arc<HashMap<String, ValueRef>>,
    /// Variable schema for defaults and secret redaction.
    variable_schema: Option<VariableSchema>,
}

impl ItemState {
    /// Create a new item state for executing a flow with the given input.
    pub fn new(
        flow: Arc<Flow>,
        input: ValueRef,
        variables: Arc<HashMap<String, ValueRef>>,
    ) -> Self {
        let step_index = Arc::new(StepIndex::from_flow(&flow));
        let num_steps = step_index.num_steps();
        let variable_schema = flow.variables();

        Self {
            flow,
            step_index,
            completed: BitSet::with_capacity(num_steps),
            results: vec![None; num_steps],
            needed: BitSet::with_capacity(num_steps),
            waiting_on: vec![BitSet::new(); num_steps],
            waiters: vec![BitSet::new(); num_steps],
            input,
            variables,
            variable_schema,
        }
    }

    /// Get the flow for this item.
    pub fn flow(&self) -> &Arc<Flow> {
        &self.flow
    }

    /// Get the step index for this item.
    pub fn step_index_map(&self) -> &Arc<StepIndex> {
        &self.step_index
    }

    /// Get the number of steps in this item's flow.
    pub fn num_steps(&self) -> usize {
        self.step_index.num_steps()
    }

    /// Add a step to the needed set.
    #[cfg(test)]
    fn mark_needed(&mut self, step_index: usize) {
        self.needed.insert(step_index);
    }

    /// Add a step as needed and transitively discover all needed steps.
    ///
    /// Returns the set of newly discovered needed step indices.
    pub fn add_or_update_needed(&mut self, step_idx: usize) -> BitSet {
        let mut newly_needed = BitSet::new();
        let mut worklist = vec![step_idx];

        while let Some(idx) = worklist.pop() {
            // Mark as needed
            let is_new = !self.needed.contains(idx);
            self.needed.insert(idx);
            if is_new {
                newly_needed.insert(idx);
            }

            // Evaluate what this step needs
            let step = self.flow.step(idx);
            let mut pending_deps = step.input.needed_steps(self);

            // Only wait on deps that aren't already completed.
            // This is important for recovery where some steps are marked completed
            // before initialization runs.
            pending_deps.difference_with(&self.completed);

            if pending_deps.is_empty() {
                self.clear_waiting(idx);
            } else {
                self.set_waiting(idx, pending_deps.clone());

                // Queue unvisited deps for processing
                for dep_idx in pending_deps.iter() {
                    if !self.needed.contains(dep_idx) {
                        worklist.push(dep_idx);
                    }
                }
            }
        }

        newly_needed
    }

    /// Set what a step is waiting on for re-evaluation.
    pub fn set_waiting(&mut self, step: usize, waiting: BitSet) {
        // Clear old waiters mapping
        for old_wait in self.waiting_on[step].iter() {
            self.waiters[old_wait].remove(step);
        }

        // Set new waiters mapping
        for wait in waiting.iter() {
            self.waiters[wait].insert(step);
        }

        self.waiting_on[step] = waiting;
    }

    /// Clear a step's waiting set.
    pub fn clear_waiting(&mut self, step: usize) {
        for old_wait in self.waiting_on[step].iter() {
            self.waiters[old_wait].remove(step);
        }
        self.waiting_on[step].clear();
    }

    /// Mark a step as completed with its result.
    ///
    /// Returns the set of steps that became newly unblocked.
    pub fn mark_completed(&mut self, step: usize, result: FlowResult) -> BitSet {
        if !self.completed.insert(step) {
            log::warn!("Step {step} already completed");
            return BitSet::new();
        }

        self.results[step] = Some(result);

        // Get steps that were waiting on this one
        let waiters = std::mem::take(&mut self.waiters[step]);

        // Update each waiter's waiting_on set
        let mut newly_unblocked = BitSet::new();
        for waiter in waiters.iter() {
            self.waiting_on[waiter].remove(step);
            if self.waiting_on[waiter].is_empty() {
                newly_unblocked.insert(waiter);
            }
        }

        newly_unblocked
    }

    /// Get the set of steps that are schedulable for execution.
    ///
    /// A step is schedulable if it is needed, not completed, and all of its
    /// needed dependencies are available -- it is not waiting on anything.
    ///
    /// Note: This does not track in-flight tasks. The executor/scheduler
    /// must separately track which tasks are currently executing to prevent
    /// double-scheduling.
    pub fn schedulable_steps(&self) -> BitSet {
        let mut schedulable = self.needed.clone();
        schedulable.difference_with(&self.completed);

        schedulable
            .iter()
            .filter(|&step| self.waiting_on[step].is_empty())
            .collect()
    }

    /// Check if all needed steps have completed.
    pub fn is_complete(&self) -> bool {
        self.needed.is_subset(&self.completed)
    }

    /// Check if the item execution has failed.
    ///
    /// An item is considered failed if any of its completed steps have a Failed result.
    pub fn is_failed(&self) -> bool {
        self.results
            .iter()
            .flatten()
            .any(|r| matches!(r, FlowResult::Failed(_)))
    }

    /// Check if a step has been marked as needed.
    pub fn is_needed(&self, step: usize) -> bool {
        self.needed.contains(step)
    }

    /// Get all needed step indices as a vector.
    ///
    /// This is used for journalling to record which steps are needed for this item.
    pub fn needed_step_indices(&self) -> Vec<usize> {
        self.needed.iter().collect()
    }

    /// Check if a step has completed execution.
    pub fn is_completed(&self, step: usize) -> bool {
        self.completed.contains(step)
    }

    /// Get the result for a completed step.
    pub fn get_step_result(&self, step: usize) -> Option<&FlowResult> {
        self.results.get(step).and_then(|r| r.as_ref())
    }

    /// Get a reference to the input value.
    pub fn input(&self) -> &ValueRef {
        &self.input
    }

    /// Get step status information for all needed steps.
    ///
    /// Returns step status info for each step that was needed for this item.
    /// This is used when recording item results to include step status summary.
    pub fn get_step_statuses(&self) -> Vec<StepStatusInfo> {
        let steps = self.flow.steps();
        self.needed
            .iter()
            .map(|step_idx| {
                let step = &steps[step_idx];
                let status = if self.completed.contains(step_idx) {
                    // Step completed - check if it succeeded or failed
                    match self.results.get(step_idx).and_then(|r| r.as_ref()) {
                        Some(FlowResult::Success(_)) => StepStatus::Completed,
                        Some(FlowResult::Failed(_)) => StepStatus::Failed,
                        None => StepStatus::Completed, // Completed but result not stored (shouldn't happen)
                    }
                } else {
                    // Step not completed - it's either blocked or runnable
                    if self.waiting_on[step_idx].is_empty() {
                        StepStatus::Runnable
                    } else {
                        StepStatus::Blocked
                    }
                };
                StepStatusInfo {
                    step_id: step.id.clone(),
                    status,
                }
            })
            .collect()
    }

    // =========================================================================
    // Recovery Methods
    // =========================================================================

    /// Apply a task completion from journal replay.
    ///
    /// This marks a step as completed with its result, without updating
    /// dependency tracking. This is used during recovery where we first
    /// replay all completions, then re-initialize to properly compute
    /// which steps are now unblocked.
    ///
    /// Unlike `mark_completed`, this doesn't return newly unblocked steps
    /// since dependency tracking happens after all completions are applied.
    pub fn apply_completed(&mut self, step_index: usize, result: FlowResult) {
        self.completed.insert(step_index);
        self.results[step_index] = Some(result);
    }
}

impl StepContext for ItemState {
    fn step_index(&self, step_id: &str) -> Option<usize> {
        self.step_index.step_index(step_id)
    }

    fn is_completed(&self, step_index: usize) -> bool {
        self.completed.contains(step_index)
    }

    fn get_result(&self, step_index: usize) -> Option<&FlowResult> {
        self.results.get(step_index).and_then(|r| r.as_ref())
    }

    fn get_input(&self) -> Option<&ValueRef> {
        Some(&self.input)
    }

    fn get_variable(&self, name: &str) -> Option<ValueRef> {
        // First try the provided variables
        if let Some(value) = self.variables.get(name) {
            return Some(value.clone());
        }

        // Fall back to schema default if available
        if let Some(schema) = &self.variable_schema {
            return schema.default_value(name);
        }

        None
    }

    fn get_variable_secrets(&self, name: &str) -> Secrets {
        self.variable_schema
            .as_ref()
            .map(|s| s.secrets().field(name).clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use stepflow_core::ValueExpr;
    use stepflow_core::workflow::{FlowBuilder, StepBuilder};

    fn assert_bitset_eq(actual: &BitSet, expected: &[usize]) {
        let expected_set: BitSet = expected.iter().cloned().collect();
        assert_eq!(
            *actual,
            expected_set,
            "Expected {:?}, got {:?}",
            expected,
            actual.iter().collect::<Vec<_>>()
        );
    }

    fn success_result() -> FlowResult {
        FlowResult::Success(ValueRef::new(json!(null)))
    }

    fn create_flow(step_names: &[&str]) -> Flow {
        let steps = step_names
            .iter()
            .map(|name| StepBuilder::mock_step(*name).build())
            .collect::<Vec<_>>();
        FlowBuilder::test_flow().steps(steps).build()
    }

    fn create_item_state(flow: Arc<Flow>) -> ItemState {
        ItemState::new(flow, ValueRef::new(json!(null)), Arc::new(HashMap::new()))
    }

    fn create_item_state_with_all_needed(flow: Arc<Flow>) -> ItemState {
        let num_steps = flow.steps().len();
        let mut state = create_item_state(flow);
        for i in 0..num_steps {
            state.mark_needed(i);
        }
        state
    }

    #[test]
    fn test_schedulable_steps_basic() {
        let flow = Arc::new(create_flow(&["step1", "step2"]));
        let mut state = create_item_state_with_all_needed(flow);

        // Both steps should be ready
        assert_bitset_eq(&state.schedulable_steps(), &[0, 1]);

        // Complete step1
        state.mark_completed(0, success_result());
        assert_bitset_eq(&state.schedulable_steps(), &[1]);

        // Complete step2
        state.mark_completed(1, success_result());
        assert_bitset_eq(&state.schedulable_steps(), &[]);
    }

    #[test]
    fn test_waiting_on_mechanism() {
        let flow = Arc::new(create_flow(&["step1", "step2", "step3"]));
        let mut state = create_item_state_with_all_needed(flow);

        // step3 is waiting on step1 and step2
        let mut waiting = BitSet::new();
        waiting.insert(0);
        waiting.insert(1);
        state.set_waiting(2, waiting);

        // Only step1 and step2 should be ready
        assert_bitset_eq(&state.schedulable_steps(), &[0, 1]);

        // Complete step1 - step3 still waiting on step2
        let newly_unblocked = state.mark_completed(0, success_result());
        assert_bitset_eq(&newly_unblocked, &[]);

        // Complete step2 - step3 now unblocked
        let newly_unblocked = state.mark_completed(1, success_result());
        assert_bitset_eq(&newly_unblocked, &[2]);

        // step3 should now be ready
        assert_bitset_eq(&state.schedulable_steps(), &[2]);
    }

    #[test]
    fn test_is_complete() {
        let flow = Arc::new(create_flow(&["step1", "step2"]));
        let mut state = create_item_state(flow);

        // No steps needed = complete
        assert!(state.is_complete());

        // Mark step1 as needed
        state.mark_needed(0);
        assert!(!state.is_complete());

        // Complete step1
        state.mark_completed(0, success_result());
        assert!(state.is_complete());

        // Mark step2 as needed
        state.mark_needed(1);
        assert!(!state.is_complete());

        // Complete step2
        state.mark_completed(1, success_result());
        assert!(state.is_complete());
    }

    #[test]
    fn test_step_context_implementation() {
        let flow = Arc::new(create_flow(&["step1", "step2"]));
        let mut state = create_item_state(flow);

        // Test step_index lookup
        assert_eq!(StepContext::step_index(&state, "step1"), Some(0));
        assert_eq!(StepContext::step_index(&state, "step2"), Some(1));
        assert_eq!(StepContext::step_index(&state, "unknown"), None);

        // Test is_completed
        assert!(!StepContext::is_completed(&state, 0));

        state.mark_needed(0);
        state.mark_completed(0, FlowResult::Success(ValueRef::new(json!(42))));

        assert!(StepContext::is_completed(&state, 0));

        // Test get_result
        let result = StepContext::get_result(&state, 0);
        assert!(result.is_some());
        match result.unwrap() {
            FlowResult::Success(v) => assert_eq!(v.as_ref(), &json!(42)),
            _ => panic!("Expected Success"),
        }
    }

    #[test]
    fn test_variables() {
        let flow = create_flow(&["step1"]);
        let mut variables = HashMap::new();
        variables.insert("api_key".to_string(), ValueRef::new(json!("secret123")));

        let state = ItemState::new(
            Arc::new(flow),
            ValueRef::new(json!(null)),
            Arc::new(variables),
        );

        // Test get_variable
        let var = state.get_variable("api_key");
        assert!(var.is_some());
        assert_eq!(var.unwrap().as_ref(), &json!("secret123"));

        // Unknown variable
        assert!(state.get_variable("unknown").is_none());
    }

    #[test]
    fn test_add_or_update_needed_with_dependencies() {
        // Create a flow with dependencies: step2 depends on step1
        let flow = FlowBuilder::test_flow()
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
            .build();

        let mut state = create_item_state(Arc::new(flow));

        // Add step2 as needed - should discover step1 as a dependency
        let newly_needed = state.add_or_update_needed(1);

        // Both steps should now be needed
        assert!(state.is_needed(0));
        assert!(state.is_needed(1));

        // newly_needed should include both
        assert!(newly_needed.contains(0));
        assert!(newly_needed.contains(1));

        // step1 should be ready (no deps), step2 should be waiting
        assert_bitset_eq(&state.schedulable_steps(), &[0]);
    }
}
