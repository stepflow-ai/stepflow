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

mod error;
mod executor;
mod flow_executor;
mod flow_executor_builder;
mod recovery;
mod run_state;
mod scheduler;
mod state;
mod step_runner;
mod task;

#[cfg(test)]
pub(crate) mod testing;

// Main exports
pub use error::{ExecutionError, Result};
pub use executor::{get_run, submit_run, wait_for_completion};
pub use flow_executor::FlowExecutor;
pub use flow_executor_builder::FlowExecutorBuilder;

// State types
pub use run_state::RunState;
pub use state::{ItemState, ItemsState, StepIndex};

// Scheduling
pub use scheduler::{BreadthFirstScheduler, DepthFirstScheduler, Scheduler, SchedulerDecision};

// Task types
pub use task::{Task, TaskResult};

// Step execution
pub use step_runner::{StepMetadata, StepRunResult};

// Recovery
pub use recovery::{RecoveryResult, recover_orphaned_runs};
