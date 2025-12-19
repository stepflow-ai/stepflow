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

use std::collections::HashMap;

use bit_set::BitSet;
use stepflow_core::{
    FlowResult,
    values::{Secrets, StepContext, ValueRef},
    workflow::VariableSchema,
};

/// Step index mapping for execution tracking.
///
/// This provides efficient lookups between step IDs and indices.
#[derive(Debug)]
pub(crate) struct StepIndex {
    /// Number of steps in the workflow.
    num_steps: usize,
    /// Map from step name to index (for name -> index lookups).
    step_name_to_index: HashMap<String, usize>,
}

impl StepIndex {
    /// Create a step index from a flow's steps.
    pub fn from_flow(flow: &stepflow_core::workflow::Flow) -> Self {
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

/// Tracks workflow execution state for lazy step evaluation.
///
/// The tracker dynamically discovers which steps need to execute based on
/// runtime expression evaluation. When evaluating expressions like `$step`
/// the tracker records which steps are referenced and marks them as needed.
/// Steps that are never needed (eg., in untaken branches) are never executed.
///
/// The tracker also manages execution coordination: tracking which steps have
/// completed and which are blocked waiting on other steps to complete.
///
/// waiting_on and waiters are used to record when expressions like `$if` or
/// `$coalesce` are waiting for some results before they can be resolved. Once
/// the steps they were waiting on are computed, the expression will be
/// re-evaluated to see if it is  ready for resolution or new needed steps are
/// discovered (and started).
pub(crate) struct ExecutionTracker {
    /// Step index mapping for ID <-> index lookups.
    step_index: StepIndex,
    /// For each step, whether it is currently executing.
    executing: BitSet,
    /// For each step, whether it has completed execution.
    completed: BitSet,
    /// Cached results for completed steps.
    results: Vec<Option<FlowResult>>,
    /// Steps that are dynamically determined to be needed.
    needed: BitSet,
    /// For each step, the set of steps it's waiting on for re-evaluation.
    /// When all steps in waiting_on[i] are completed, step i should be re-evaluated.
    waiting_on: Vec<BitSet>,
    /// Reverse mapping: for each step, which steps are waiting on it.
    /// Used to efficiently notify waiters when a step completes.
    waiters: Vec<BitSet>,
    /// Workflow input value for evaluating `$input` references.
    input: Option<ValueRef>,
    /// Variable values for evaluating `$variable` references.
    variables: HashMap<String, ValueRef>,
    /// Variable schema for defaults and secret redaction.
    variable_schema: Option<VariableSchema>,
}

impl ExecutionTracker {
    /// Create a new execution tracker for a workflow.
    ///
    /// - `flow`: The workflow definition
    /// - `input`: The workflow input value (for `$input` references)
    /// - `variables`: Variable values (for `$variable` references)
    pub fn new(
        flow: &stepflow_core::workflow::Flow,
        input: ValueRef,
        variables: HashMap<String, ValueRef>,
    ) -> Self {
        let step_index = StepIndex::from_flow(flow);
        let num_steps = step_index.num_steps();
        Self {
            step_index,
            executing: BitSet::with_capacity(num_steps),
            completed: BitSet::with_capacity(num_steps),
            results: vec![None; num_steps],
            needed: BitSet::with_capacity(num_steps),
            waiting_on: vec![BitSet::new(); num_steps],
            waiters: vec![BitSet::new(); num_steps],
            input: Some(input),
            variables,
            variable_schema: flow.variables().cloned(),
        }
    }

    /// Add a step to the needed set (without evaluating dependencies).
    #[cfg(test)]
    pub fn add_needed(&mut self, step: usize) {
        self.needed.insert(step);
    }

    /// Add a step as needed and recursively discover all transitively needed steps.
    ///
    /// This evaluates each newly needed step's input expression to determine
    /// what other steps it depends on, recursively adding those as well.
    /// Returns the set of newly discovered needed step indices.
    ///
    /// If the step is already needed, this re-evaluates its dependencies
    /// (useful after a dependency completes and waiting_on may have changed).
    pub fn add_or_update_needed(
        &mut self,
        step_idx: usize,
        flow: &stepflow_core::workflow::Flow,
    ) -> BitSet {
        let mut newly_needed = BitSet::new();
        self.add_needed_recursive(step_idx, flow, &mut newly_needed);
        newly_needed
    }

    fn add_needed_recursive(
        &mut self,
        step_idx: usize,
        flow: &stepflow_core::workflow::Flow,
        newly_needed: &mut BitSet,
    ) {
        // Track if this is a new addition
        let is_new = !self.needed.contains(step_idx);

        // Add to needed set (idempotent for already-needed steps)
        self.needed.insert(step_idx);
        if is_new {
            newly_needed.insert(step_idx);
        }

        // Evaluate what this step needs (re-evaluate in case deps have changed)
        let step = flow.step(step_idx);
        let deps = step.input.needed_steps(self);

        if deps.is_empty() {
            // Step is ready immediately
            self.clear_waiting(step_idx);
        } else {
            // Step needs to wait on deps
            self.set_waiting(step_idx, deps.clone());

            // Recurse into deps that aren't completed
            for dep_idx in deps.iter() {
                if self.is_completed(dep_idx) {
                    continue;
                }
                // Only recurse for deps that aren't already needed
                // (already-needed deps have their waiting set up correctly)
                if !self.needed.contains(dep_idx) {
                    self.add_needed_recursive(dep_idx, flow, newly_needed);
                }
            }
        }
    }

    /// Set what a step is waiting on for re-evaluation.
    ///
    /// When all steps in `waiting` complete, this step should be re-evaluated
    /// to determine if it's ready or needs more dependencies.
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

    /// Clear a step's waiting set (e.g., when it becomes ready).
    pub fn clear_waiting(&mut self, step: usize) {
        for old_wait in self.waiting_on[step].iter() {
            self.waiters[old_wait].remove(step);
        }
        self.waiting_on[step].clear();
    }

    /// Mark a step as completed with its result.
    ///
    /// Returns the set of steps that became newly unblocked (their waiting_on
    /// set is now empty after removing this completed step).
    pub fn complete_step(&mut self, step: usize, result: FlowResult) -> BitSet {
        // Clear executing flag
        self.executing.remove(step);

        // Record completion
        if !self.completed.insert(step) {
            // Already completed - shouldn't happen, but handle gracefully
            log::warn!("Step {step} already completed");
            return BitSet::new();
        }

        self.results[step] = Some(result);

        // Get steps that were waiting on this one
        let waiters = std::mem::take(&mut self.waiters[step]);

        // Update each waiter's waiting_on set to remove this completed step
        // Track which waiters became newly unblocked (waiting_on now empty)
        let mut newly_unblocked = BitSet::new();
        for waiter in waiters.iter() {
            self.waiting_on[waiter].remove(step);
            if self.waiting_on[waiter].is_empty() {
                newly_unblocked.insert(waiter);
            }
        }

        newly_unblocked
    }

    /// Get the set of steps that are ready to execute.
    ///
    /// A step is ready if:
    /// - It is needed
    /// - It is not already executing
    /// - It is not completed
    /// - It has no waiting dependencies (waiting_on is empty)
    pub fn ready_steps(&self) -> BitSet {
        let mut ready = self.needed.clone();
        ready.difference_with(&self.executing);
        ready.difference_with(&self.completed);

        // Only include steps with empty waiting_on sets
        ready
            .iter()
            .filter(|&step| self.waiting_on[step].is_empty())
            .collect()
    }

    /// Mark a step as currently executing.
    ///
    /// This prevents the step from appearing in `ready_steps()` until it completes.
    pub fn start_step(&mut self, step: usize) {
        self.executing.insert(step);
    }

    /// Check if all needed steps have completed.
    pub fn all_needed_completed(&self) -> bool {
        self.needed.is_subset(&self.completed)
    }

    /// Check if a step has been marked as needed.
    #[cfg(test)]
    pub fn is_needed(&self, step: usize) -> bool {
        self.needed.contains(step)
    }
}

impl StepContext for ExecutionTracker {
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
        self.input.as_ref()
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
    use stepflow_core::values::ValueRef;
    use stepflow_core::workflow::{Flow, FlowBuilder, StepBuilder};

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
        FlowResult::Success(ValueRef::new(serde_json::json!(null)))
    }

    /// Create a simple flow with the given step names (no dependencies between them).
    fn create_flow(step_names: &[&str]) -> stepflow_core::workflow::Flow {
        let steps = step_names
            .iter()
            .map(|name| StepBuilder::mock_step(*name).build())
            .collect::<Vec<_>>();
        FlowBuilder::test_flow().steps(steps).build()
    }

    /// Create a tracker with empty input and no variables (for tests).
    fn create_tracker(flow: &stepflow_core::workflow::Flow) -> ExecutionTracker {
        ExecutionTracker::new(flow, ValueRef::new(serde_json::Value::Null), HashMap::new())
    }

    /// Create a tracker with all steps marked as needed.
    fn create_tracker_with_all_needed(flow: &stepflow_core::workflow::Flow) -> ExecutionTracker {
        let mut tracker = create_tracker(flow);
        let num_steps = flow.steps().len();
        let all_steps: BitSet = (0..num_steps).collect();
        tracker.needed = all_steps;
        tracker
    }

    #[test]
    fn test_ready_steps_basic() {
        let flow = create_flow(&["step1", "step2"]);
        let mut tracker = create_tracker_with_all_needed(&flow);

        // Both steps should be ready (no dependencies, all needed)
        assert_bitset_eq(&tracker.ready_steps(), &[0, 1]);

        // Complete step1 - only step2 should be ready now
        tracker.complete_step(0, success_result());
        assert_bitset_eq(&tracker.ready_steps(), &[1]);

        // Complete step2 - no steps ready
        tracker.complete_step(1, success_result());
        assert_bitset_eq(&tracker.ready_steps(), &[]);
    }

    #[test]
    fn test_waiting_on_mechanism() {
        let flow = create_flow(&["step1", "step2", "step3"]);
        let mut tracker = create_tracker_with_all_needed(&flow);

        // step3 is waiting on step1 and step2
        let mut waiting = BitSet::new();
        waiting.insert(0);
        waiting.insert(1);
        tracker.set_waiting(2, waiting);

        // Only step1 and step2 should be ready (step3 is waiting)
        assert_bitset_eq(&tracker.ready_steps(), &[0, 1]);

        // Complete step1 - step3 is NOT yet unblocked (still waiting on step2)
        let newly_unblocked = tracker.complete_step(0, success_result());
        assert_bitset_eq(&newly_unblocked, &[]); // step3 still has deps

        // step3 is still waiting on step2
        assert_bitset_eq(&tracker.ready_steps(), &[1]);

        // Complete step2 - now step3 becomes unblocked
        let newly_unblocked = tracker.complete_step(1, success_result());
        assert_bitset_eq(&newly_unblocked, &[2]); // step3 is now unblocked

        // Now step3 should be ready (waiting_on was cleared automatically)
        assert_bitset_eq(&tracker.ready_steps(), &[2]);
    }

    #[test]
    fn test_step_context_implementation() {
        let flow = create_flow(&["step1", "step2"]);
        let mut tracker = create_tracker(&flow);

        // Test step_index (via StepContext trait)
        assert_eq!(StepContext::step_index(&tracker, "step1"), Some(0));
        assert_eq!(StepContext::step_index(&tracker, "step2"), Some(1));
        assert_eq!(StepContext::step_index(&tracker, "unknown"), None);

        // Test is_completed before completion
        assert!(!tracker.is_completed(0));
        assert!(!tracker.is_completed(1));

        // Complete step1
        tracker.needed.insert(0);
        tracker.complete_step(0, FlowResult::Success(ValueRef::new(serde_json::json!(42))));

        // Test is_completed after completion
        assert!(tracker.is_completed(0));
        assert!(!tracker.is_completed(1));

        // Test get_result
        let result = tracker.get_result(0);
        assert!(result.is_some());
        match result.unwrap() {
            FlowResult::Success(v) => assert_eq!(v.as_ref(), &serde_json::json!(42)),
            _ => panic!("Expected Success"),
        }
        assert!(tracker.get_result(1).is_none());
    }

    #[test]
    fn test_all_needed_completed() {
        let flow = create_flow(&["step1", "step2", "step3"]);
        let mut tracker = create_tracker(&flow);

        // Mark only step1 and step2 as needed
        tracker.add_needed(0);
        tracker.add_needed(1);

        assert!(!tracker.all_needed_completed());

        tracker.complete_step(0, success_result());
        assert!(!tracker.all_needed_completed());

        tracker.complete_step(1, success_result());
        assert!(tracker.all_needed_completed());

        // step3 not needed, so still complete
        assert!(tracker.all_needed_completed());
    }

    #[test]
    fn test_empty_workflow() {
        let flow = create_flow(&[]);
        let tracker = create_tracker(&flow);

        // No steps means nothing ready
        assert_bitset_eq(&tracker.ready_steps(), &[]);
    }

    #[test]
    fn test_step_index_lookups() {
        let flow = create_flow(&["step1", "step2"]);
        let step_index = StepIndex::from_flow(&flow);

        assert_eq!(step_index.step_index("step1"), Some(0));
        assert_eq!(step_index.step_index("step2"), Some(1));
        assert_eq!(step_index.step_index("unknown"), None);
    }

    /// Initialize a tracker using dynamic evaluation, similar to what the executor does.
    /// This evaluates the output and must_execute steps to determine what's needed,
    /// then sets up waiting_on for each step based on their input expressions.
    fn initialize_tracker_dynamic(tracker: &mut ExecutionTracker, flow: &Flow) {
        // 1. Evaluate output's needed_steps and add directly
        let output_needs = flow.output().needed_steps(tracker);
        for idx in output_needs.iter() {
            tracker.add_needed(idx);
        }

        // 2. Add must_execute steps
        for (idx, step) in flow.steps().iter().enumerate() {
            if step.must_execute() {
                tracker.add_needed(idx);
            }
        }

        // 3. Evaluate each needed step's input to set up waiting_on
        // Iterate until no more changes (handles transitive deps)
        loop {
            let needed: Vec<usize> = (0..flow.steps().len())
                .filter(|&idx| tracker.is_needed(idx) && !tracker.is_completed(idx))
                .collect();

            let mut changed = false;
            for step_idx in needed {
                let step = flow.step(step_idx);
                let all_needs = step.input.needed_steps(tracker);

                if !all_needs.is_empty() {
                    // Filter to only incomplete deps
                    let waiting: BitSet = all_needs
                        .iter()
                        .filter(|&idx| !tracker.is_completed(idx))
                        .collect();

                    if !waiting.is_empty() && tracker.waiting_on[step_idx].is_empty() {
                        tracker.set_waiting(step_idx, waiting.clone());
                        // Add newly discovered deps
                        for dep in waiting.iter() {
                            if !tracker.is_needed(dep) {
                                tracker.add_needed(dep);
                                changed = true;
                            }
                        }
                    }
                }
            }

            if !changed {
                break;
            }
        }
    }

    #[test]
    fn test_must_execute_steps_are_tracked() {
        // Create a flow where step3 has must_execute but is not referenced by output
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
                StepBuilder::mock_step("step3")
                    .input(ValueExpr::Input {
                        input: Default::default(),
                    })
                    .must_execute(true)
                    .build(),
            ])
            .output(ValueExpr::Step {
                step: "step2".to_string(),
                path: Default::default(),
            })
            .build();

        let mut tracker = create_tracker(&flow);
        initialize_tracker_dynamic(&mut tracker, &flow);

        // Initially step1 and step3 should be runnable (no dependencies)
        let ready = tracker.ready_steps();
        assert!(ready.contains(0), "step1 should be initially runnable");
        assert!(ready.contains(2), "step3 should be initially runnable");
        assert!(!ready.contains(1), "step2 should be blocked on step1");

        // Required steps (step2 for output + step3 for must_execute) are NOT completed yet
        assert!(
            !tracker.all_needed_completed(),
            "step2 and step3 (needed) have not completed yet"
        );

        // Complete step1
        let newly_unblocked = tracker.complete_step(0, success_result());
        assert!(
            newly_unblocked.contains(1),
            "step2 should be unblocked after step1"
        );

        // Still not all needed steps completed
        assert!(
            !tracker.all_needed_completed(),
            "step2 and step3 (needed) have not completed yet"
        );

        // Complete step2 (output dependency satisfied, but step3 still needed)
        tracker.complete_step(1, success_result());

        // Still not all needed steps completed (step3 must_execute still needed)
        assert!(
            !tracker.all_needed_completed(),
            "step3 (must_execute) has not completed yet"
        );

        // Complete step3 (must_execute step)
        tracker.complete_step(2, success_result());

        // Now all needed steps are completed (output deps + must_execute)
        assert!(
            tracker.all_needed_completed(),
            "All needed steps (step2 for output, step3 for must_execute) have completed"
        );
    }

    #[test]
    fn test_only_required_steps_execute() {
        // Create a flow with an unreferenced step that should NOT execute
        // step1 -> step2 (output depends on step2)
        // step3 (not referenced, not must_execute, should NOT run)
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
                StepBuilder::mock_step("step3")
                    .input(ValueExpr::Input {
                        input: Default::default(),
                    })
                    .build(),
            ])
            .output(ValueExpr::Step {
                step: "step2".to_string(),
                path: Default::default(),
            })
            .build();

        let mut tracker = create_tracker(&flow);
        initialize_tracker_dynamic(&mut tracker, &flow);

        // Initially only step1 should be runnable (step3 is not needed)
        let ready = tracker.ready_steps();
        assert!(ready.contains(0), "step1 should be runnable");
        assert!(
            !ready.contains(2),
            "step3 should NOT be runnable (not needed)"
        );

        // Complete step1
        let newly_unblocked = tracker.complete_step(0, success_result());
        assert!(
            newly_unblocked.contains(1),
            "step2 should be newly runnable"
        );
        assert!(
            !newly_unblocked.contains(2),
            "step3 should NOT be newly runnable (not needed)"
        );

        // Complete step2
        tracker.complete_step(1, success_result());

        // Now all needed steps are completed
        assert!(tracker.all_needed_completed(), "All needed steps completed");

        // Verify step3 is not needed
        assert!(
            !tracker.is_needed(2),
            "step3 should never be needed (not required)"
        );
    }

    #[test]
    fn test_must_execute_without_dependencies() {
        // Create a flow where step2 has must_execute but no dependencies and isn't referenced
        let flow = FlowBuilder::test_flow()
            .steps(vec![
                StepBuilder::mock_step("step1")
                    .input(ValueExpr::Input {
                        input: Default::default(),
                    })
                    .build(),
                StepBuilder::mock_step("step2")
                    .input(ValueExpr::literal(json!({"value": 42})))
                    .must_execute(true)
                    .build(),
            ])
            .output(ValueExpr::Step {
                step: "step1".to_string(),
                path: Default::default(),
            })
            .build();

        let mut tracker = create_tracker(&flow);
        initialize_tracker_dynamic(&mut tracker, &flow);

        // Both step1 and step2 should be initially runnable
        let ready = tracker.ready_steps();
        assert!(ready.contains(0), "step1 should be runnable");
        assert!(ready.contains(1), "step2 should be runnable");

        // Needed steps (step1 for output + step2 for must_execute) not completed yet
        assert!(
            !tracker.all_needed_completed(),
            "step1 and step2 (needed) have not completed yet"
        );

        // Complete step1 (satisfies output dependency, but step2 still needed)
        tracker.complete_step(0, success_result());

        // Needed step2 (must_execute) still not completed
        assert!(
            !tracker.all_needed_completed(),
            "step2 (must_execute) has not completed yet"
        );

        // Complete step2
        tracker.complete_step(1, success_result());

        // Now all needed steps are completed (output deps + must_execute)
        assert!(
            tracker.all_needed_completed(),
            "All needed steps (step1 for output, step2 for must_execute) have completed"
        );
    }

    #[test]
    fn test_executing_steps_excluded_from_ready() {
        // Regression test: ensure that once a step is started (marked as executing),
        // it does not appear in ready_steps() again. This prevents the same step
        // from being executed multiple times in parallel.
        let flow = create_flow(&["step1", "step2"]);
        let mut tracker = create_tracker_with_all_needed(&flow);

        // Both steps should initially be ready
        assert_bitset_eq(&tracker.ready_steps(), &[0, 1]);

        // Start step1 - it should no longer appear in ready_steps
        tracker.start_step(0);
        assert_bitset_eq(&tracker.ready_steps(), &[1]);

        // Calling ready_steps multiple times should consistently exclude step1
        assert_bitset_eq(&tracker.ready_steps(), &[1]);
        assert_bitset_eq(&tracker.ready_steps(), &[1]);

        // Start step2 - now no steps should be ready
        tracker.start_step(1);
        assert_bitset_eq(&tracker.ready_steps(), &[]);

        // Complete step1 - it should NOT reappear (it's completed, not just not-executing)
        tracker.complete_step(0, success_result());
        assert_bitset_eq(&tracker.ready_steps(), &[]);

        // Complete step2
        tracker.complete_step(1, success_result());
        assert_bitset_eq(&tracker.ready_steps(), &[]);
    }
}
