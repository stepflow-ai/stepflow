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

//! Task scheduling for item-based execution.
//!
//! This module provides the [`Scheduler`] trait and implementations for
//! different scheduling strategies:
//!
//! - [`DepthFirstScheduler`]: Prioritizes completing items sequentially,
//!   minimizing time-to-first-result.
//! - [`BreadthFirstScheduler`]: Groups tasks by step across items,
//!   maximizing throughput and enabling batch component invocation.
//!
//! # Scheduling Model
//!
//! The scheduler works with the [`ItemsState`](crate::items::ItemsState) to
//! determine which tasks are ready to execute. When tasks complete, the
//! scheduler is notified so it can update its internal state and discover
//! new ready tasks.
//!
//! # Debug Mode
//!
//! Debug execution doesn't use a special scheduler. Instead, [`DebugExecutor`]
//! controls execution through *task addition* - steps are added to the needed
//! set incrementally rather than all at once. The underlying scheduler
//! (typically [`DepthFirstScheduler`]) handles ordering of ready tasks.

mod breadth_first;
mod decision;
mod depth_first;

pub use breadth_first::BreadthFirstScheduler;
pub use decision::SchedulerDecision;
pub use depth_first::DepthFirstScheduler;

// Re-export NonEmpty for convenience
pub use nonempty::NonEmpty;

use crate::task::Task;

/// Trait for task scheduling strategies.
///
/// A scheduler determines which tasks to execute next based on the current
/// state of execution. Different strategies optimize for different goals:
///
/// - **Depth-first**: Complete items sequentially for low latency to first result
/// - **Breadth-first**: Group tasks by step for throughput and batch optimization
///
/// # Executor Contract
///
/// The executor guarantees the following invariants, which scheduler implementations
/// can rely on:
///
/// 1. **Tasks are notified exactly once**: Each task is passed to `notify_new_tasks`
///    exactly once when it becomes ready. The scheduler does not need to check for
///    duplicates or already-active tasks.
///
/// 2. **Tasks are ready when notified**: Tasks passed to `notify_new_tasks` have all
///    dependencies satisfied and are ready to execute immediately.
///
/// 3. **Completions trigger notifications**: When a task completes and unblocks other
///    tasks, the executor calls `notify_new_tasks` with those newly-ready tasks.
///
/// # Thread Safety
///
/// Schedulers must be `Send + Sync` to support concurrent execution.
pub trait Scheduler: Send + Sync {
    /// Select the next tasks to execute.
    ///
    /// Returns up to `max_tasks` tasks that were previously notified via
    /// `notify_new_tasks`. Returns `Idle` when no tasks are available.
    ///
    /// The scheduler should return tasks in priority order according to its
    /// scheduling strategy (e.g., depth-first or breadth-first).
    fn select_next(&mut self, max_tasks: usize) -> SchedulerDecision;

    /// Notify the scheduler that a task has completed.
    ///
    /// Called after each task finishes execution. Implementations may use this
    /// to update internal state, though newly-ready tasks are provided separately
    /// via `notify_new_tasks`.
    fn task_completed(&mut self, task: Task);

    /// Notify the scheduler that new tasks are ready.
    ///
    /// Called when:
    /// - Initial tasks become ready at execution start
    /// - Dependencies are resolved after task completion
    /// - Tasks are explicitly added (debug mode)
    ///
    /// Per the executor contract, these tasks are guaranteed to be new (not
    /// previously notified) and ready (all dependencies satisfied).
    fn notify_new_tasks(&mut self, tasks: &[Task]);

    /// Reset the scheduler state.
    ///
    /// Called when starting a new execution to clear any accumulated state.
    fn reset(&mut self);
}
