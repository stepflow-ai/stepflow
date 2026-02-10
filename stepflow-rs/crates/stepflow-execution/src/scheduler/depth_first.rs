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

//! Depth-first scheduler implementation.
//!
//! This scheduler prioritizes completing items sequentially, minimizing
//! time-to-first-result. It tries to complete item 0 before starting
//! substantive work on item 1, etc.

use std::collections::BinaryHeap;

use crate::scheduler::{Scheduler, SchedulerDecision};
use crate::task::Task;

/// Wrapper for Task with depth-first ordering (item_index, step_index).
///
/// Uses `Reverse` to convert BinaryHeap's max-heap into a min-heap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
struct DepthFirstTask(Task);

impl PartialOrd for DepthFirstTask {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DepthFirstTask {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse ordering: smaller (item_index, step_index, run_id) should come first
        // BinaryHeap is max-heap, so we reverse the comparison
        // Note: run_id is compared last so tasks from the same item are grouped first
        (other.0.item_index, other.0.step_index, other.0.run_id).cmp(&(
            self.0.item_index,
            self.0.step_index,
            self.0.run_id,
        ))
    }
}

/// A scheduler that prioritizes completing items sequentially.
///
/// DepthFirstScheduler minimizes time-to-first-result by focusing on
/// completing item 0 before item 1, and so on. Within an item, it
/// executes ready steps as soon as possible.
///
/// Uses a binary heap for O(log n) insertion and O(log n) extraction.
///
/// # Example
///
/// For a batch with 2 items and 3 steps each:
/// ```text
/// Item 0: step0 -> step1 -> step2
/// Item 1: step0 -> step1 -> step2
/// ```
///
/// Depth-first ordering (assuming no parallelism):
/// ```text
/// item0/step0 -> item0/step1 -> item0/step2 -> item1/step0 -> item1/step1 -> item1/step2
/// ```
#[derive(Debug, Default)]
pub struct DepthFirstScheduler {
    /// Heap of ready tasks, ordered by (item_index, step_index) ascending.
    ready_heap: BinaryHeap<DepthFirstTask>,
}

impl DepthFirstScheduler {
    /// Create a new depth-first scheduler.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Scheduler for DepthFirstScheduler {
    fn select_next(&mut self, max_tasks: usize) -> SchedulerDecision {
        let mut tasks = Vec::with_capacity(max_tasks.min(self.ready_heap.len()));
        while tasks.len() < max_tasks {
            if let Some(DepthFirstTask(task)) = self.ready_heap.pop() {
                tasks.push(task);
            } else {
                break;
            }
        }
        SchedulerDecision::execute(tasks)
    }

    fn task_completed(&mut self, _task: Task) {
        // No-op: newly ready tasks are provided via notify_new_tasks
    }

    fn notify_new_tasks(&mut self, tasks: &[Task]) {
        for &task in tasks {
            self.ready_heap.push(DepthFirstTask(task));
        }
    }

    fn reset(&mut self) {
        self.ready_heap.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;
    use stepflow_core::values::ValueRef;
    use stepflow_core::workflow::{FlowBuilder, StepBuilder};
    use uuid::Uuid;

    fn test_run_id() -> Uuid {
        Uuid::nil()
    }

    #[test]
    fn test_depth_first_ordering() {
        let run_id = test_run_id();
        let mut scheduler = DepthFirstScheduler::new();

        // Notify with tasks in arbitrary order
        scheduler.notify_new_tasks(&[
            Task::new(run_id, 1, 0),
            Task::new(run_id, 0, 1),
            Task::new(run_id, 0, 0),
            Task::new(run_id, 1, 1),
        ]);

        // Get all tasks
        let tasks: Vec<_> = scheduler
            .select_next(10)
            .into_tasks()
            .unwrap()
            .into_iter()
            .collect();

        // Should be sorted by (item_index, step_index)
        assert_eq!(tasks.len(), 4);
        assert_eq!(tasks[0], Task::new(run_id, 0, 0));
        assert_eq!(tasks[1], Task::new(run_id, 0, 1));
        assert_eq!(tasks[2], Task::new(run_id, 1, 0));
        assert_eq!(tasks[3], Task::new(run_id, 1, 1));
    }

    #[test]
    fn test_depth_first_with_limit() {
        let run_id = test_run_id();
        let mut scheduler = DepthFirstScheduler::new();

        // Notify with 6 tasks
        scheduler.notify_new_tasks(&[
            Task::new(run_id, 0, 0),
            Task::new(run_id, 0, 1),
            Task::new(run_id, 0, 2),
            Task::new(run_id, 1, 0),
            Task::new(run_id, 1, 1),
            Task::new(run_id, 1, 2),
        ]);

        // Request only 2 tasks
        let tasks: Vec<_> = scheduler
            .select_next(2)
            .into_tasks()
            .unwrap()
            .into_iter()
            .collect();
        assert_eq!(tasks.len(), 2);

        // Both should be from item 0 (depth-first)
        for task in &tasks {
            assert_eq!(task.item_index, 0);
        }
    }

    #[test]
    fn test_notify_new_tasks() {
        let run_id = test_run_id();
        let mut scheduler = DepthFirstScheduler::new();

        // Notify with out-of-order tasks
        scheduler.notify_new_tasks(&[
            Task::new(run_id, 1, 0),
            Task::new(run_id, 0, 1),
            Task::new(run_id, 0, 0),
        ]);

        // Heap should contain all tasks; verify by popping in order
        let mut tasks = Vec::new();
        while let Some(DepthFirstTask(task)) = scheduler.ready_heap.pop() {
            tasks.push(task);
        }
        assert_eq!(tasks[0], Task::new(run_id, 0, 0));
        assert_eq!(tasks[1], Task::new(run_id, 0, 1));
        assert_eq!(tasks[2], Task::new(run_id, 1, 0));
    }

    #[test]
    fn test_incremental_notification() {
        let run_id = test_run_id();
        // Tests the executor contract: tasks are notified as they become ready
        let mut scheduler = DepthFirstScheduler::new();

        // Initial task
        scheduler.notify_new_tasks(&[Task::new(run_id, 0, 0)]);

        // Get first task
        let task = scheduler.select_next(1).into_tasks().unwrap().head;
        assert_eq!(task, Task::new(run_id, 0, 0));

        // Simulate task completion - executor notifies newly ready tasks
        scheduler.task_completed(task);
        scheduler.notify_new_tasks(&[Task::new(run_id, 0, 1)]);

        // Get next task
        let task = scheduler.select_next(1).into_tasks().unwrap().head;
        assert_eq!(task, Task::new(run_id, 0, 1));
    }

    #[test]
    fn test_empty_returns_idle() {
        let mut scheduler = DepthFirstScheduler::new();
        assert!(scheduler.select_next(10).is_idle());
    }

    #[test]
    fn test_reset() {
        let run_id = test_run_id();
        let mut scheduler = DepthFirstScheduler::new();

        scheduler.notify_new_tasks(&[Task::new(run_id, 0, 0), Task::new(run_id, 1, 0)]);
        scheduler.reset();

        assert!(scheduler.ready_heap.is_empty());
        assert!(scheduler.select_next(10).is_idle());
    }

    // Integration test that mirrors real executor behavior
    #[test]
    fn test_integration_with_state() {
        use crate::state::ItemsState;
        use stepflow_core::ValueExpr;

        let run_id = test_run_id();

        // Create a flow where step2 depends on step1
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

        let mut state = ItemsState::single(
            run_id,
            flow,
            ValueRef::new(json!({})),
            std::collections::HashMap::new(),
        );
        let initial_tasks = state.initialize_item(0);

        let mut scheduler = DepthFirstScheduler::new();
        scheduler.notify_new_tasks(&initial_tasks);

        // Only step1 should be ready (step2 depends on it)
        assert_eq!(initial_tasks.len(), 1);
        assert_eq!(initial_tasks[0], Task::new(run_id, 0, 0));

        // Get first task
        let task = scheduler.select_next(1).into_tasks().unwrap().head;
        assert_eq!(task, Task::new(run_id, 0, 0));

        // Complete and get newly ready
        let new_ready = state.complete_task_and_get_ready(
            task,
            stepflow_core::FlowResult::Success(ValueRef::new(json!(null))),
        );
        scheduler.task_completed(task);
        scheduler.notify_new_tasks(&new_ready);

        // step2 should now be ready
        assert_eq!(new_ready.len(), 1);
        assert_eq!(new_ready[0], Task::new(run_id, 0, 1));

        // Scheduler should return step2
        let next = scheduler.select_next(1).into_tasks().unwrap().head;
        assert_eq!(next, Task::new(run_id, 0, 1));
    }
}
