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

//! Breadth-first scheduler implementation.
//!
//! This scheduler groups tasks by step across items, maximizing throughput
//! and enabling batch component invocation. It tries to execute the same
//! step for all items before moving to the next step.

use std::collections::BinaryHeap;

use crate::scheduler::{Scheduler, SchedulerDecision};
use crate::task::Task;

/// Wrapper for Task with breadth-first ordering (step_index, item_index).
///
/// Uses reversed comparison to convert BinaryHeap's max-heap into a min-heap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
struct BreadthFirstTask(Task);

impl PartialOrd for BreadthFirstTask {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BreadthFirstTask {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse ordering: smaller (step_index, item_index) should come first
        // BinaryHeap is max-heap, so we reverse the comparison
        (other.0.step_index, other.0.item_index).cmp(&(self.0.step_index, self.0.item_index))
    }
}

/// A scheduler that groups tasks by step across items.
///
/// BreadthFirstScheduler maximizes throughput by grouping tasks that
/// execute the same step. This enables future optimization where
/// component calls can be batched (e.g., one API call for multiple items).
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
/// Breadth-first ordering (assuming no parallelism):
/// ```text
/// item0/step0 -> item1/step0 -> item0/step1 -> item1/step1 -> item0/step2 -> item1/step2
/// ```
///
/// Note: The exact order depends on which tasks become ready. This scheduler
/// prioritizes grouping tasks by step_index, but respects dependency order.
#[derive(Debug, Default)]
pub struct BreadthFirstScheduler {
    /// Heap of ready tasks, ordered by (step_index, item_index) ascending.
    ready_heap: BinaryHeap<BreadthFirstTask>,
}

impl BreadthFirstScheduler {
    /// Create a new breadth-first scheduler.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Scheduler for BreadthFirstScheduler {
    fn select_next(&mut self, max_tasks: usize) -> SchedulerDecision {
        let mut tasks = Vec::with_capacity(max_tasks.min(self.ready_heap.len()));
        while tasks.len() < max_tasks {
            if let Some(BreadthFirstTask(task)) = self.ready_heap.pop() {
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
            self.ready_heap.push(BreadthFirstTask(task));
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

    #[test]
    fn test_breadth_first_ordering() {
        let mut scheduler = BreadthFirstScheduler::new();

        // Notify with tasks in arbitrary order
        scheduler.notify_new_tasks(&[
            Task::new(1, 0),
            Task::new(0, 1),
            Task::new(0, 0),
            Task::new(1, 1),
        ]);

        // Get all tasks
        let tasks: Vec<_> = scheduler
            .select_next(10)
            .into_tasks()
            .unwrap()
            .into_iter()
            .collect();

        // Should be sorted by (step_index, item_index)
        assert_eq!(tasks.len(), 4);
        assert_eq!(tasks[0], Task::new(0, 0)); // step 0, item 0
        assert_eq!(tasks[1], Task::new(1, 0)); // step 0, item 1
        assert_eq!(tasks[2], Task::new(0, 1)); // step 1, item 0
        assert_eq!(tasks[3], Task::new(1, 1)); // step 1, item 1
    }

    #[test]
    fn test_breadth_first_groups_by_step() {
        let mut scheduler = BreadthFirstScheduler::new();

        // Notify with tasks for step 0 from 3 items
        scheduler.notify_new_tasks(&[Task::new(0, 0), Task::new(1, 0), Task::new(2, 0)]);

        // Request all tasks
        let tasks: Vec<_> = scheduler
            .select_next(10)
            .into_tasks()
            .unwrap()
            .into_iter()
            .collect();

        // All tasks should be for step 0
        assert_eq!(tasks.len(), 3);
        for task in &tasks {
            assert_eq!(task.step_index, 0);
        }

        // Items should be in order
        assert_eq!(tasks[0].item_index, 0);
        assert_eq!(tasks[1].item_index, 1);
        assert_eq!(tasks[2].item_index, 2);
    }

    #[test]
    fn test_breadth_first_with_limit() {
        let mut scheduler = BreadthFirstScheduler::new();

        // Notify with 6 tasks
        scheduler.notify_new_tasks(&[
            Task::new(0, 0),
            Task::new(0, 1),
            Task::new(0, 2),
            Task::new(1, 0),
            Task::new(1, 1),
            Task::new(1, 2),
        ]);

        // Request only 2 tasks
        let tasks: Vec<_> = scheduler
            .select_next(2)
            .into_tasks()
            .unwrap()
            .into_iter()
            .collect();
        assert_eq!(tasks.len(), 2);

        // Both should be from step 0 (breadth-first)
        for task in &tasks {
            assert_eq!(task.step_index, 0);
        }
    }

    #[test]
    fn test_notify_new_tasks() {
        let mut scheduler = BreadthFirstScheduler::new();

        // Notify with out-of-order tasks
        scheduler.notify_new_tasks(&[Task::new(0, 1), Task::new(1, 0), Task::new(0, 0)]);

        // Heap should contain all tasks; verify by popping in order
        let mut tasks = Vec::new();
        while let Some(BreadthFirstTask(task)) = scheduler.ready_heap.pop() {
            tasks.push(task);
        }
        assert_eq!(tasks[0], Task::new(0, 0)); // step 0, item 0
        assert_eq!(tasks[1], Task::new(1, 0)); // step 0, item 1
        assert_eq!(tasks[2], Task::new(0, 1)); // step 1, item 0
    }

    #[test]
    fn test_incremental_notification() {
        // Tests the executor contract: tasks are notified as they become ready
        let mut scheduler = BreadthFirstScheduler::new();

        // Initial task
        scheduler.notify_new_tasks(&[Task::new(0, 0)]);

        // Get first task
        let task = scheduler.select_next(1).into_tasks().unwrap().head;
        assert_eq!(task, Task::new(0, 0));

        // Simulate task completion - executor notifies newly ready tasks
        scheduler.task_completed(task);
        scheduler.notify_new_tasks(&[Task::new(0, 1)]);

        // Get next task
        let task = scheduler.select_next(1).into_tasks().unwrap().head;
        assert_eq!(task, Task::new(0, 1));
    }

    #[test]
    fn test_empty_returns_idle() {
        let mut scheduler = BreadthFirstScheduler::new();
        assert!(scheduler.select_next(10).is_idle());
    }

    #[test]
    fn test_reset() {
        let mut scheduler = BreadthFirstScheduler::new();

        scheduler.notify_new_tasks(&[Task::new(0, 0), Task::new(1, 0)]);
        scheduler.reset();

        assert!(scheduler.ready_heap.is_empty());
        assert!(scheduler.select_next(10).is_idle());
    }

    // Integration test that mirrors real executor behavior
    #[test]
    fn test_integration_with_state() {
        use crate::state::ItemsState;
        use stepflow_core::ValueExpr;

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
            flow,
            ValueRef::new(json!({})),
            std::collections::HashMap::new(),
        );

        let mut scheduler = BreadthFirstScheduler::new();

        // Executor pattern: initialize_item returns ready tasks
        let initial_tasks = state.initialize_item(0);
        scheduler.notify_new_tasks(&initial_tasks);

        // Only step1 should be ready (step2 depends on it)
        assert_eq!(initial_tasks.len(), 1);
        assert_eq!(initial_tasks[0], Task::new(0, 0));

        // Get first task
        let task = scheduler.select_next(1).into_tasks().unwrap().head;
        assert_eq!(task, Task::new(0, 0));
        state.mark_executing(task);

        // Complete and get newly ready
        let new_ready = state.complete_task_and_get_ready(
            task,
            stepflow_core::FlowResult::Success(ValueRef::new(json!(null))),
        );
        scheduler.task_completed(task);
        scheduler.notify_new_tasks(&new_ready);

        // step2 should now be ready
        assert_eq!(new_ready.len(), 1);
        assert_eq!(new_ready[0], Task::new(0, 1));

        // Scheduler should return step2
        let next = scheduler.select_next(1).into_tasks().unwrap().head;
        assert_eq!(next, Task::new(0, 1));
    }
}
