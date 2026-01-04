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

//! Workflow execution engine for Stepflow.
//!
//! This crate provides the core execution infrastructure:
//!
//! - [`FlowExecutor`] - batch/single workflow execution with scheduler
//! - [`DebugExecutor`] - step-by-step debugging with queue/eval API
//! - [`StepflowExecutor`] - main entry point for workflow execution
//!
//! ## Execution State
//!
//! - [`ItemState`] - per-item execution state (dependency tracking, results)
//! - [`ItemsState`] - cross-item coordinator for batch execution
//!
//! ## Scheduling
//!
//! - [`Scheduler`] - trait for custom execution strategies
//! - [`DepthFirstScheduler`] - complete items sequentially (time-to-first-result)
//! - [`BreadthFirstScheduler`] - group by step (throughput, batch invocation)
//!
//! ## Debug Mode
//!
//! Debug execution differs from batch mode in *when* steps are added to the
//! needed set, not in how they are executed. [`DebugExecutor`] composes a
//! [`FlowExecutor`] and controls execution through task addition - steps are
//! added incrementally via `queue_step()` rather than all at once.

mod debug_executor;
mod debug_history;
mod error;
mod executor;
mod flow_executor;
mod scheduler;
mod state;
mod step_runner;
mod task;

#[cfg(test)]
pub(crate) mod testing;

// Main exports
pub use debug_executor::{DebugExecutor, StepInspection};
pub use debug_history::{DebugEvent, DebugHistory, PendingAction};
pub use error::{ExecutionError, Result};
pub use executor::StepflowExecutor;
pub use flow_executor::{FlowExecutor, FlowExecutorBuilder};

// State types
pub use state::{ItemState, ItemsState, StepIndex};

// Scheduling
pub use scheduler::{
    BreadthFirstScheduler, DepthFirstScheduler, NonEmpty, Scheduler, SchedulerDecision,
};

// Task types
pub use task::{Task, TaskResult};

// Step execution
pub use step_runner::{StepMetadata, StepRunResult};
