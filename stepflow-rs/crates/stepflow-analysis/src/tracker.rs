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

// Licensed to the Apache Software Foundation (ASF) under one or more contributor license agreements.
// See the NOTICE file distributed with this work for additional information regarding copyright
// ownership.  The ASF licenses this file to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance with the License.  You may obtain a
// copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied.  See the License for the specific language governing permissions and limitations under
// the License.

use std::{collections::HashMap, sync::Arc};

use bit_set::BitSet;

/// Builds a dependency graph for a workflow.
pub struct DependenciesBuilder {
    steps: usize,
    step_names: Vec<String>,
    step_names_to_index: HashMap<String, usize>,
    /// For each step, a bitset of the steps that depend on it.
    step_dependents: Vec<BitSet>,
    /// For each step, a bitset of the steps that it depends on.
    step_dependencies: Vec<BitSet>,
}

impl DependenciesBuilder {
    /// Create a new builder for a workflow with the given number of steps.
    pub fn new(steps: usize) -> Self {
        Self {
            steps,
            step_names: Vec::with_capacity(steps),
            step_names_to_index: HashMap::with_capacity(steps),
            step_dependents: Vec::with_capacity(steps),
            step_dependencies: Vec::with_capacity(steps),
        }
    }

    /// Add a step to the workflow.
    ///
    /// The dependencies are the names of the steps that must be completed before this step can be run.
    pub fn add_step<S: AsRef<str>, I: IntoIterator<Item = S>>(
        &mut self,
        name: impl Into<String>,
        dependencies: I,
    ) {
        let name = name.into();
        let index = self.step_names_to_index.len();
        debug_assert!(index < self.steps);
        self.step_names.push(name.clone());
        self.step_names_to_index.insert(name, index);

        let mut dependency_set = BitSet::with_capacity(self.step_names_to_index.len());
        for dependency in dependencies {
            let dependency = self
                .step_names_to_index
                .get(dependency.as_ref())
                .expect("Unknown dependency");

            if dependency_set.insert(*dependency) {
                self.step_dependents[*dependency].insert(index);
            }
        }
        self.step_dependencies.push(dependency_set);
        self.step_dependents.push(BitSet::with_capacity(self.steps))
    }

    /// Finish building the dependency graph.
    pub fn finish(self) -> Arc<Dependencies> {
        Arc::new(Dependencies {
            steps: self.steps,
            step_names: self.step_names,
            step_dependents: self.step_dependents,
            step_dependencies: self.step_dependencies,
        })
    }
}

/// Information about dependencies in a workflow.
#[derive(Debug)]
pub struct Dependencies {
    steps: usize,
    /// Step names (for debugging).
    step_names: Vec<String>,
    /// For each step, a bitset of the steps that depend on it.
    step_dependents: Vec<BitSet>,
    /// For each step, a bitset of the steps that it depends on.
    step_dependencies: Vec<BitSet>,
}

impl Dependencies {}

pub struct DependencyTracker {
    dependencies: Arc<Dependencies>,
    /// For each step, the count of remaining dependencies.
    blocking: Vec<usize>,
    /// For each step, whether it has been completed.
    completed: BitSet,
}

impl DependencyTracker {
    pub fn new(dependencies: Arc<Dependencies>) -> Self {
        let blocking = dependencies
            .step_dependencies
            .iter()
            .map(|d| d.len())
            .collect();
        let completed = BitSet::with_capacity(dependencies.steps);
        Self {
            dependencies,
            blocking,
            completed,
        }
    }

    /// Return the name of the given step.
    pub fn step_name(&self, step: usize) -> &str {
        &self.dependencies.step_names[step]
    }

    /// Return the set of all steps that are currently runnable.
    pub fn unblocked_steps(&self) -> BitSet {
        let mut unblocked: BitSet = self
            .blocking
            .iter()
            .enumerate()
            .filter(|(_, blocking)| **blocking == 0)
            .map(|(step, _)| step)
            .collect();
        unblocked.difference_with(&self.completed);
        unblocked
    }

    /// Mark the given step as completed.
    ///
    /// Return a set of newly runnable steps.
    pub fn complete_step(&mut self, step: usize) -> BitSet {
        // Record completion. If already completed, return empty set
        if !self.completed.insert(step) {
            return BitSet::new();
        }

        let mut unblocked = BitSet::with_capacity(self.dependencies.steps);
        for dependent in self.dependencies.step_dependents[step].iter() {
            self.blocking[dependent] -= 1;
            if self.blocking[dependent] == 0 {
                // This step was previously blocking the dependent. Therefore,
                // the dependent was not runnable and should not be completed.
                debug_assert!(!self.completed.contains(dependent));
                unblocked.insert(dependent);
            }
        }
        unblocked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_simple_chain() {
        let mut builder = DependenciesBuilder::new(3);
        builder.add_step("step1", Vec::<&str>::new());
        builder.add_step("step2", vec!["step1"]);
        builder.add_step("step3", vec!["step2"]);

        let deps = builder.finish();
        let mut tracker = DependencyTracker::new(deps);

        // Initially only step1 should be runnable
        assert_bitset_eq(&tracker.unblocked_steps(), &[0]);

        // Complete step1 -> only step2 becomes runnable
        let newly_unblocked = tracker.complete_step(0);
        assert_bitset_eq(&newly_unblocked, &[1]);
        assert_bitset_eq(&tracker.unblocked_steps(), &[1]);

        // Complete step2 -> only step3 becomes runnable
        let newly_unblocked = tracker.complete_step(1);
        assert_bitset_eq(&newly_unblocked, &[2]);
        assert_bitset_eq(&tracker.unblocked_steps(), &[2]);

        // Complete step3 -> no new steps
        let newly_unblocked = tracker.complete_step(2);
        assert_bitset_eq(&newly_unblocked, &[]);
        assert_bitset_eq(&tracker.unblocked_steps(), &[]);
    }

    #[test]
    fn test_parallel_execution() {
        let mut builder = DependenciesBuilder::new(4);
        builder.add_step("step1", Vec::<&str>::new());
        builder.add_step("step2", Vec::<&str>::new());
        builder.add_step("step3", vec!["step1", "step2"]);
        builder.add_step("step4", vec!["step1"]);

        let deps = builder.finish();
        let mut tracker = DependencyTracker::new(deps);

        // Initially step1 and step2 should be runnable, not step3 or step4
        assert_bitset_eq(&tracker.unblocked_steps(), &[0, 1]);

        // Complete step1 -> only step4 becomes newly runnable
        let newly_unblocked = tracker.complete_step(0);
        assert_bitset_eq(&newly_unblocked, &[3]);
        assert_bitset_eq(&tracker.unblocked_steps(), &[1, 3]);

        // Complete step2 -> only step3 becomes newly runnable
        let newly_unblocked = tracker.complete_step(1);
        assert_bitset_eq(&newly_unblocked, &[2]);
        assert_bitset_eq(&tracker.unblocked_steps(), &[2, 3]);

        // Complete step3 -> no new steps
        let newly_unblocked = tracker.complete_step(2);
        assert_bitset_eq(&newly_unblocked, &[]);
        assert_bitset_eq(&tracker.unblocked_steps(), &[3]);

        // Complete step4 -> no new steps
        let newly_unblocked = tracker.complete_step(3);
        assert_bitset_eq(&newly_unblocked, &[]);
        assert_bitset_eq(&tracker.unblocked_steps(), &[]);
    }

    #[test]
    fn test_diamond_dependency() {
        // step1 -> step2, step3 -> step4
        let mut builder = DependenciesBuilder::new(4);
        builder.add_step("step1", Vec::<&str>::new());
        builder.add_step("step2", vec!["step1"]);
        builder.add_step("step3", vec!["step1"]);
        builder.add_step("step4", vec!["step2", "step3"]);

        let deps = builder.finish();
        let mut tracker = DependencyTracker::new(deps);

        // Only step1 runnable initially
        assert_bitset_eq(&tracker.unblocked_steps(), &[0]);

        // Complete step1 -> step2 and step3 become runnable, not step4
        let newly_unblocked = tracker.complete_step(0);
        assert_bitset_eq(&newly_unblocked, &[1, 2]);
        assert_bitset_eq(&tracker.unblocked_steps(), &[1, 2]);

        // Complete step2 -> no new steps (step4 still needs step3)
        let newly_unblocked = tracker.complete_step(1);
        assert_bitset_eq(&newly_unblocked, &[]);
        assert_bitset_eq(&tracker.unblocked_steps(), &[2]);

        // Complete step3 -> step4 becomes runnable
        let newly_unblocked = tracker.complete_step(2);
        assert_bitset_eq(&newly_unblocked, &[3]);
        assert_bitset_eq(&tracker.unblocked_steps(), &[3]);
    }

    #[test]
    fn test_no_dependencies() {
        let mut builder = DependenciesBuilder::new(3);
        builder.add_step("step1", Vec::<&str>::new());
        builder.add_step("step2", Vec::<&str>::new());
        builder.add_step("step3", Vec::<&str>::new());

        let deps = builder.finish();
        let tracker = DependencyTracker::new(deps);

        // All steps runnable initially, none missing
        assert_bitset_eq(&tracker.unblocked_steps(), &[0, 1, 2]);
    }

    #[test]
    fn test_single_step() {
        let mut builder = DependenciesBuilder::new(1);
        builder.add_step("only_step", Vec::<&str>::new());

        let deps = builder.finish();
        let mut tracker = DependencyTracker::new(deps);

        // Only step should be runnable
        assert_bitset_eq(&tracker.unblocked_steps(), &[0]);

        // After completion, nothing should be runnable
        let newly_unblocked = tracker.complete_step(0);
        assert_bitset_eq(&newly_unblocked, &[]);
        assert_bitset_eq(&tracker.unblocked_steps(), &[]);
    }

    #[test]
    fn test_multiple_completion_same_step() {
        let mut builder = DependenciesBuilder::new(2);
        builder.add_step("step1", Vec::<&str>::new());
        builder.add_step("step2", vec!["step1"]);

        let deps = builder.finish();
        let mut tracker = DependencyTracker::new(deps);

        // Complete step1 first time
        let newly_unblocked = tracker.complete_step(0);
        assert_bitset_eq(&newly_unblocked, &[1]);

        // Complete step1 again - should return empty set
        let newly_unblocked = tracker.complete_step(0);
        assert_bitset_eq(&newly_unblocked, &[]);

        // step2 should still be runnable
        assert_bitset_eq(&tracker.unblocked_steps(), &[1]);
    }

    #[test]
    #[should_panic(expected = "Unknown dependency")]
    fn test_unknown_dependency() {
        let mut builder = DependenciesBuilder::new(2);
        builder.add_step("step1", Vec::<&str>::new());
        builder.add_step("step2", vec!["unknown_step"]);
    }

    #[test]
    fn test_empty_workflow() {
        let builder = DependenciesBuilder::new(0);
        let deps = builder.finish();
        let tracker = DependencyTracker::new(deps);

        // No steps means nothing runnable
        assert_bitset_eq(&tracker.unblocked_steps(), &[]);
    }
}
