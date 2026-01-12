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

//! Task types for item-based execution.
//!
//! A [`Task`] represents a single unit of work: executing a specific step for a specific item
//! within a specific run. Tasks are identified by `(run_id, item_index, step_index)` tuples
//! and are the fundamental unit of scheduling in the execution engine.
//!
//! The `run_id` field enables multiple runs (including sub-flows) to execute concurrently
//! within the same executor, with their tasks interleaved in the scheduler.

use stepflow_core::FlowResult;
use uuid::Uuid;

use crate::step_runner::StepRunResult;

/// A single unit of work in the execution engine.
///
/// Each task represents executing one step for one item in a run (workflow execution).
/// Tasks are scheduled by the [`Scheduler`](crate::scheduler::Scheduler) and executed
/// by the executor. The `run_id` allows tasks from multiple runs (including sub-flows)
/// to coexist in the same scheduler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Task {
    /// The run this task belongs to (supports multiple concurrent runs including sub-flows).
    pub run_id: Uuid,
    /// Index of the item this task belongs to within the run.
    pub item_index: u32,
    /// Index of the step to execute within the item's flow.
    pub step_index: usize,
}

impl Task {
    /// Create a new task.
    pub fn new(run_id: Uuid, item_index: u32, step_index: usize) -> Self {
        Self {
            run_id,
            item_index,
            step_index,
        }
    }
}

/// The result of executing a task.
///
/// Combines the run ID, item index, and step execution result.
#[derive(Debug, Clone)]
pub struct TaskResult {
    /// The run this result belongs to.
    pub run_id: Uuid,
    /// Index of the item this result belongs to within the run.
    pub item_index: u32,
    /// The step execution result (includes step metadata and FlowResult).
    pub step: StepRunResult,
}

impl TaskResult {
    /// Create a new task result from a task and step result.
    pub fn new(task: Task, step: StepRunResult) -> Self {
        Self {
            run_id: task.run_id,
            item_index: task.item_index,
            step,
        }
    }

    /// Reconstruct the Task from this result.
    pub fn task(&self) -> Task {
        Task::new(self.run_id, self.item_index, self.step.step_index())
    }

    /// Get the step index.
    pub fn step_index(&self) -> usize {
        self.step.step_index()
    }

    /// Get a reference to the flow result.
    pub fn result(&self) -> &FlowResult {
        &self.step.result
    }

    /// Check if the result is a success.
    pub fn is_success(&self) -> bool {
        self.step.is_success()
    }

    /// Check if the result is a failure.
    pub fn is_failed(&self) -> bool {
        self.step.is_failed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stepflow_core::values::ValueRef;

    fn test_run_id() -> Uuid {
        Uuid::nil()
    }

    #[test]
    fn test_task_creation() {
        let run_id = test_run_id();
        let task = Task::new(run_id, 5, 10);
        assert_eq!(task.run_id, run_id);
        assert_eq!(task.item_index, 5);
        assert_eq!(task.step_index, 10);
    }

    #[test]
    fn test_task_equality() {
        let run_id = test_run_id();
        let task1 = Task::new(run_id, 1, 2);
        let task2 = Task::new(run_id, 1, 2);
        let task3 = Task::new(run_id, 1, 3);

        assert_eq!(task1, task2);
        assert_ne!(task1, task3);
    }

    #[test]
    fn test_task_equality_different_runs() {
        let run_id1 = Uuid::nil();
        let run_id2 = Uuid::from_u128(1);
        let task1 = Task::new(run_id1, 1, 2);
        let task2 = Task::new(run_id2, 1, 2);

        assert_ne!(task1, task2);
    }

    #[test]
    fn test_task_hash() {
        use std::collections::HashSet;

        let run_id = test_run_id();
        let mut set = HashSet::new();
        set.insert(Task::new(run_id, 1, 2));
        set.insert(Task::new(run_id, 1, 2)); // Duplicate
        set.insert(Task::new(run_id, 3, 4));

        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_task_result() {
        let run_id = test_run_id();
        let task = Task::new(run_id, 0, 1);
        let result = FlowResult::Success(ValueRef::new(serde_json::json!(42)));
        let step_result =
            StepRunResult::new(1, "test_step".to_string(), "/mock/test".to_string(), result);
        let task_result = TaskResult::new(task, step_result);

        assert_eq!(task_result.run_id, run_id);
        assert_eq!(task_result.item_index, 0);
        assert_eq!(task_result.step_index(), 1);
        assert_eq!(task_result.step.step_id(), "test_step");
        assert!(task_result.is_success());
        match task_result.result() {
            FlowResult::Success(v) => assert_eq!(v.as_ref(), &serde_json::json!(42)),
            _ => panic!("Expected Success"),
        }
    }
}
