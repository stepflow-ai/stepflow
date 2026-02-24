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

//! Builder for creating FlowExecutor instances.

use std::collections::HashMap;
use std::sync::Arc;

use error_stack::ResultExt as _;
use stepflow_plugin::{ExecutionConfig, StepflowEnvironment, subflow_channel};
use stepflow_state::{CheckpointStoreExt as _, CreateRunParams, MetadataStoreExt as _};

use crate::checkpointer::Checkpointer;
use crate::flow_executor::FlowExecutor;
use crate::run_state::RunState;
use crate::scheduler::Scheduler;
use crate::{ExecutionError, Result};

/// Builder for [`FlowExecutor`].
///
/// Creates a FlowExecutor from a RunState, which can be either freshly created
/// for new runs or reconstructed from journal replay for recovery.
///
/// # Example (new run)
///
/// ```ignore
/// // Create RunState with flow, inputs, variables
/// let run_state = RunState::new(run_id, flow_id, flow, inputs, variables);
///
/// // Apply overrides if needed
/// let flow = apply_overrides(flow, &overrides)?;
///
/// // Create RunState
/// let run_state = RunState::new(run_id, flow_id, flow, inputs, variables);
///
/// // Build executor
/// let executor = FlowExecutorBuilder::new(env, run_state)
///     .scheduler(Box::new(DepthFirstScheduler::new()))
///     .build()
///     .await?;
/// ```
///
/// # Example (recovery)
///
/// ```ignore
/// // Replay journal to reconstruct state
/// let mut run_state = RunState::new(...);
/// journal_events.iter().for_each(|e| { run_state.apply_event(e); });
///
/// // Build executor with recovered state
/// let executor = FlowExecutorBuilder::new(env, run_state)
///     .skip_validation()
///     .build()
///     .await?;
/// ```
pub struct FlowExecutorBuilder {
    env: Arc<StepflowEnvironment>,
    run_state: RunState,
    scheduler: Option<Box<dyn Scheduler>>,
    max_concurrency: usize,
    skip_validation: bool,
    /// Additional run states to include (e.g., recovered subflow RunStates).
    additional_runs: HashMap<uuid::Uuid, RunState>,
    /// Recovered subflow mappings for deduplication during re-execution.
    recovered_subflows: HashMap<(uuid::Uuid, u32, usize, uuid::Uuid), uuid::Uuid>,
}

impl FlowExecutorBuilder {
    /// Create a new builder with a RunState.
    ///
    /// The RunState can be freshly created for new runs or reconstructed from
    /// journal replay for recovery. The state store is obtained from the environment.
    pub fn new(env: Arc<StepflowEnvironment>, run_state: RunState) -> Self {
        Self {
            env,
            run_state,
            scheduler: None,
            max_concurrency: 10,
            skip_validation: false,
            additional_runs: HashMap::new(),
            recovered_subflows: HashMap::new(),
        }
    }

    /// Skip workflow validation during build.
    ///
    /// Use this when resuming an execution where the flow was already validated,
    /// or when validation was performed externally.
    pub fn skip_validation(mut self) -> Self {
        self.skip_validation = true;
        self
    }

    /// Set the scheduler (default: DepthFirstScheduler).
    pub fn scheduler(mut self, scheduler: Box<dyn Scheduler>) -> Self {
        self.scheduler = Some(scheduler);
        self
    }

    /// Set maximum concurrency (default: 10).
    pub fn max_concurrency(mut self, max: usize) -> Self {
        self.max_concurrency = max;
        self
    }

    /// Add recovered subflow states and deduplication mappings.
    ///
    /// During recovery, subflow `RunState` objects are reconstructed from journal
    /// replay and injected here. When parent steps re-execute and re-submit
    /// subflows, the executor matches by `(parent_run_id, item_index, step_index,
    /// subflow_key)` and returns the existing run_id instead of creating a duplicate.
    pub fn with_recovered_subflows(
        mut self,
        additional_runs: HashMap<uuid::Uuid, RunState>,
        recovered_subflows: HashMap<(uuid::Uuid, u32, usize, uuid::Uuid), uuid::Uuid>,
    ) -> Self {
        self.additional_runs = additional_runs;
        self.recovered_subflows = recovered_subflows;
        self
    }

    /// Build the executor.
    ///
    /// This validates the workflow (unless `skip_validation()` was called) and
    /// ensures a run record exists in the state store (idempotent).
    pub async fn build(self) -> Result<FlowExecutor> {
        let run_id = self.run_state.run_id();
        let flow = self.run_state.flow();
        let state_store = self.env.metadata_store().clone();

        // Validate workflow unless skipped
        if !self.skip_validation {
            let diagnostics =
                stepflow_analysis::validate(&flow).change_context(ExecutionError::AnalysisError)?;

            if diagnostics.has_fatal() {
                let fatal = diagnostics.num_fatal;
                let error = diagnostics.num_error;
                return Err(error_stack::report!(ExecutionError::AnalysisError)
                    .attach_printable(format!(
                        "Workflow validation failed with {fatal} fatal and {error} error diagnostics"
                    )));
            }
        }

        // Ensure run record exists (idempotent - no-op if already created)
        let mut run_params = CreateRunParams::new(
            run_id,
            self.run_state.flow_id().clone(),
            self.run_state.inputs(),
        );
        run_params.workflow_name = flow.name().map(|s| s.to_string());
        state_store
            .create_run(run_params)
            .await
            .change_context(ExecutionError::StateError)?;

        // Default scheduler
        let scheduler = self
            .scheduler
            .unwrap_or_else(|| Box::new(crate::scheduler::DepthFirstScheduler::new()));

        // Create submit channel for subflow submission
        // Buffer size matches max_concurrency to avoid blocking during high concurrency
        let (submit_sender, submit_receiver) = subflow_channel(self.max_concurrency, run_id);

        log::debug!(
            "Building FlowExecutor for run {} ({} items, {} tasks ready)",
            run_id,
            self.run_state.item_count(),
            self.run_state.items_state().get_ready_tasks().len()
        );

        let mut runs = HashMap::new();
        runs.insert(run_id, self.run_state);
        // Merge in any recovered subflow RunStates
        runs.extend(self.additional_runs);

        // Create checkpointer from environment configuration
        let checkpoint_interval = self
            .env
            .get::<ExecutionConfig>()
            .map(|c| c.checkpoint_interval)
            .unwrap_or(0);
        let checkpoint_store = self.env.checkpoint_store().clone();
        let checkpointer = Checkpointer::new(checkpoint_store, run_id, checkpoint_interval);

        Ok(FlowExecutor::new_from_builder(
            self.env,
            run_id,
            runs,
            scheduler,
            self.max_concurrency,
            state_store,
            submit_sender,
            submit_receiver,
            self.recovered_subflows,
            checkpointer,
        ))
    }
}
