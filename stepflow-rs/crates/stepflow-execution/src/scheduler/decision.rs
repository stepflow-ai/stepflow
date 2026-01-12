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

//! Scheduler decision types.

use crate::task::Task;
use nonempty::NonEmpty;

/// Decision returned by the scheduler.
///
/// The scheduler returns either `Idle` (no tasks ready) or `Execute` with
/// a guaranteed non-empty list of tasks to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchedulerDecision {
    /// No tasks are ready. The executor should wait for in-flight tasks to complete.
    Idle,
    /// Execute these tasks. The executor should run them (potentially in parallel).
    ///
    /// The task list is guaranteed to be non-empty.
    Execute(NonEmpty<Task>),
}

impl SchedulerDecision {
    /// Create an Idle decision.
    pub fn idle() -> Self {
        Self::Idle
    }

    /// Create an Execute decision with the given tasks.
    ///
    /// Returns `Idle` if the task list is empty.
    pub fn execute(tasks: Vec<Task>) -> Self {
        match NonEmpty::from_vec(tasks) {
            Some(non_empty) => Self::Execute(non_empty),
            None => Self::Idle,
        }
    }

    /// Check if this is an Idle decision (no tasks to execute).
    pub fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    /// Convert into the task list, if any.
    ///
    /// Consumes the decision and returns `Some(tasks)` for `Execute`,
    /// or `None` for `Idle`.
    pub fn into_tasks(self) -> Option<NonEmpty<Task>> {
        match self {
            Self::Idle => None,
            Self::Execute(tasks) => Some(tasks),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn test_run_id() -> Uuid {
        Uuid::nil()
    }

    #[test]
    fn test_scheduler_decision_idle() {
        let decision = SchedulerDecision::idle();
        assert!(decision.is_idle());
        assert!(decision.into_tasks().is_none());
    }

    #[test]
    fn test_scheduler_decision_execute() {
        let run_id = test_run_id();
        let decision =
            SchedulerDecision::execute(vec![Task::new(run_id, 0, 1), Task::new(run_id, 1, 2)]);
        assert!(!decision.is_idle());

        let tasks = decision.into_tasks().unwrap();
        assert_eq!(tasks.len(), 2);
    }

    #[test]
    fn test_scheduler_decision_empty_execute_becomes_idle() {
        let decision = SchedulerDecision::execute(vec![]);
        assert!(decision.is_idle());
    }
}
