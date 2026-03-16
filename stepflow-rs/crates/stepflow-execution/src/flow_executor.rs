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

//! Flow executor for unified single and multi-item workflow execution.
//!
//! [`FlowExecutor`] provides the main execution engine for running workflows
//! with one or more items. It coordinates task execution across items using
//! a pluggable [`Scheduler`] and manages concurrency, state tracking, and
//! result recording.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use error_stack::ResultExt as _;
use futures::stream::{FuturesUnordered, StreamExt as _};
use stepflow_core::FlowResult;
use stepflow_core::workflow::StepId;
use stepflow_observability::RunInfoGuard;
use stepflow_state::{
    CreateRunParams, ExecutionJournal, ExecutionJournalExt as _, JournalEvent,
    LeaseManagerExt as _, MetadataStore, OrchestratorIdExt as _, RunTaskAttempts, TaskAttempt,
};
use uuid::Uuid;

use crate::checkpointer::Checkpointer;
use crate::run_state::RunState;
use crate::scheduler::Scheduler;
use crate::state::ItemsState;
use crate::step_runner::{StepRunResult, StepRunner};
use crate::task::{Task, TaskResult};
use crate::{ExecutionError, Result};
use stepflow_plugin::{
    Plugin as _, PluginRouterExt as _, SubflowReceiver, SubflowRequest, SubflowSubmitter,
};

/// Executor for running workflows with one or more items.
///
/// FlowExecutor coordinates execution across multiple items, using a scheduler
/// to determine task ordering and respecting concurrency limits. It supports
/// both run-to-completion and streaming execution modes.
///
/// Use [`crate::FlowExecutorBuilder`] to create instances.
///
/// # Example
///
/// ```ignore
/// let flow = Arc::new(flow);
/// let inputs = vec![input1, input2, input3];
/// let run_state = RunState::new(run_id, flow_id, flow, inputs, variables);
///
/// let mut executor = FlowExecutorBuilder::new(env, run_state)
///     .max_concurrency(10)
///     .build()
///     .await?;
///
/// executor.execute_to_completion().await?;
/// let results = env.metadata_store()
///     .get_item_results(run_id, ResultOrder::ByIndex)
///     .await?;
/// ```
pub struct FlowExecutor {
    /// Reference to the StepflowEnvironment for plugin access.
    env: Arc<stepflow_plugin::StepflowEnvironment>,
    /// Root run ID for this execution tree.
    /// This is the top-level run ID; sub-flows have their own run IDs.
    root_run_id: Uuid,
    /// Per-run execution state, keyed by run ID.
    /// Includes both the root run and any sub-flow runs.
    runs: HashMap<Uuid, RunState>,
    /// Scheduler for task ordering across all runs.
    scheduler: Box<dyn Scheduler>,
    /// Maximum concurrent tasks.
    max_in_flight: usize,
    /// Metadata store for persisting results.
    metadata_store: Arc<dyn MetadataStore>,
    /// Journal for appending execution events.
    journal: Arc<dyn ExecutionJournal>,
    /// Sender for submitting sub-flows to this executor.
    /// Used to create `RunContext` instances with subflow submission capability.
    submit_sender: SubflowSubmitter,
    /// Receiver for sub-flow submission requests.
    /// Processed in the execution loop alongside task completions.
    submit_receiver: SubflowReceiver,
    /// Recovered subflow mappings from journal replay.
    /// Maps (parent_run_id, item_index, step_index, subflow_key) → subflow_run_id.
    /// When a parent step re-executes after recovery and submits the "same" subflow,
    /// the dedup check in `handle_submit_request` returns the existing run_id.
    recovered_subflows: HashMap<(Uuid, u32, usize, Uuid), Uuid>,
    /// Recovered run IDs that still need a `StepsNeeded` event written.
    /// Populated during recovery for subflows in the crash window between
    /// `SubRunCreated` and `StepsNeeded`. Empty for non-recovery runs.
    runs_needing_step_updates: HashSet<Uuid>,
    /// All recovered subflow run IDs. Used to distinguish recovered subflows
    /// (which may already have `StepsNeeded` journaled) from new runs (which
    /// always need it written).
    recovered_run_ids: HashSet<Uuid>,
    /// Periodic checkpoint creator for execution state.
    checkpointer: Checkpointer,
}

impl FlowExecutor {
    /// Create a new FlowExecutor from builder components.
    ///
    /// This is used by [`FlowExecutorBuilder`] to construct the executor.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_from_builder(
        env: Arc<stepflow_plugin::StepflowEnvironment>,
        root_run_id: Uuid,
        runs: HashMap<Uuid, RunState>,
        scheduler: Box<dyn Scheduler>,
        max_in_flight: usize,
        metadata_store: Arc<dyn MetadataStore>,
        submit_sender: SubflowSubmitter,
        submit_receiver: SubflowReceiver,
        recovered_subflows: HashMap<(Uuid, u32, usize, Uuid), Uuid>,
        runs_needing_step_updates: HashSet<Uuid>,
        checkpointer: Checkpointer,
    ) -> Self {
        let journal = env.execution_journal().clone();
        // Derive the set of all recovered subflow run IDs from the dedup map.
        let recovered_run_ids: HashSet<Uuid> = recovered_subflows.values().copied().collect();
        Self {
            env,
            root_run_id,
            runs,
            scheduler,
            max_in_flight,
            metadata_store,
            journal,
            submit_sender,
            submit_receiver,
            recovered_subflows,
            runs_needing_step_updates,
            recovered_run_ids,
            checkpointer,
        }
    }

    /// Get the root run ID for this execution tree.
    pub fn root_run_id(&self) -> Uuid {
        self.root_run_id
    }

    /// Spawn this executor in the background and track it in active executions.
    ///
    /// This consumes the executor, spawns it as a tokio task, and registers it
    /// with the active executions tracker for lifecycle management. The tracker
    /// is automatically cleaned up when execution completes.
    ///
    /// Errors during execution are logged but do not propagate.
    ///
    /// # Arguments
    /// * `active_executions` - The tracker to register with
    pub fn spawn(mut self, active_executions: &stepflow_state::ActiveExecutions) {
        use stepflow_observability::fastrace::prelude::*;
        let run_id = self.root_run_id;

        let run_span_context =
            SpanContext::new(TraceId(Uuid::now_v7().as_u128()), SpanId::default());

        let run_span = Span::root("run_execution", run_span_context)
            .with_property(|| ("run_id", run_id.to_string()))
            .with_property(|| ("max_in_flight", self.max_in_flight.to_string()));

        // Spawn the executor with tracing
        let future = async move {
            if let Err(e) = self.execute_to_completion().await {
                log::error!("Run {} failed: {:?}", run_id, e);
            }

            // Release lease after completion (best-effort, root runs only)
            if let Some(orch_id) = self.env.orchestrator_id()
                && let Err(e) = self
                    .env
                    .lease_manager()
                    .release_lease(run_id, orch_id.clone())
                    .await
            {
                log::warn!("Failed to release lease for run {run_id}: {e:?}");
            }
        }
        .in_span(run_span);

        active_executions.spawn(run_id, future);
    }

    /// Get a reference to a run state by run ID.
    fn run_state(&self, run_id: Uuid) -> Option<&RunState> {
        self.runs.get(&run_id)
    }

    /// Get a mutable reference to a run state by run ID.
    fn run_state_mut(&mut self, run_id: Uuid) -> Option<&mut RunState> {
        self.runs.get_mut(&run_id)
    }

    /// Get the root run state.
    fn root_run_state(&self) -> &RunState {
        self.runs.get(&self.root_run_id).expect("root run exists")
    }

    /// Write a journal entry durably and record it for checkpointing.
    ///
    /// Returns the sequence number assigned to the entry, which callers can
    /// use as a `created_at_seqno` when creating run metadata records.
    async fn write_journal(
        &mut self,
        event: JournalEvent,
    ) -> Result<stepflow_state::SequenceNumber> {
        let sequence = self
            .journal
            .write(self.root_run_id, event)
            .await
            .change_context(ExecutionError::JournalError)?;
        self.checkpointer.record_entry(sequence);
        Ok(sequence)
    }

    /// Write per-item `StepsNeeded` journal events for a run.
    ///
    /// Each item gets its own event because conditional output expressions
    /// (`$if`) can cause different items to need different steps based on
    /// their input.
    async fn write_steps_needed_events(&mut self, run_id: Uuid, item_count: u32) -> Result<()> {
        for item_index in 0..item_count {
            let run_state = self.runs.get(&run_id).expect("run should exist");
            let step_indices = run_state
                .items_state()
                .item(item_index)
                .needed_step_indices();
            if !step_indices.is_empty() {
                self.write_journal(JournalEvent::StepsNeeded {
                    run_id,
                    item_index,
                    step_indices,
                })
                .await?;
            }
        }
        Ok(())
    }

    /// Write a step status update to the metadata store.
    ///
    /// This is called after journal writes to keep the metadata store in sync
    /// with step transitions. Failures propagate as execution errors.
    async fn update_step_status(
        &self,
        run_id: Uuid,
        item_index: u32,
        step_index: usize,
        status: stepflow_core::status::StepStatus,
        result: Option<FlowResult>,
        journal_seqno: stepflow_state::SequenceNumber,
    ) -> Result<()> {
        let run_state = self.run_state(run_id).ok_or_else(|| {
            error_stack::report!(ExecutionError::internal(format!(
                "run state not found for {run_id}"
            )))
        })?;
        let flow = run_state.flow();
        let step = &flow.steps[step_index];
        let step_id = &step.id;
        let component = step.component.path();

        self.metadata_store
            .update_step_status(
                run_id,
                item_index as usize,
                step_id,
                step_index,
                status,
                Some(component),
                result,
                journal_seqno,
            )
            .await
            .change_context(ExecutionError::MetadataStoreError)?;

        Ok(())
    }

    /// Get the subflow submitter for this executor.
    ///
    /// This can be used to submit subflows that will be processed by this executor.
    /// Primarily useful for testing the subflow submission mechanism.
    #[cfg(test)]
    pub fn submit_sender(&self) -> &SubflowSubmitter {
        &self.submit_sender
    }

    /// Internal execution loop that processes tasks until idle or fuel is exhausted.
    ///
    /// This is an internal method. Use [`execute_to_completion`] for the public API.
    ///
    /// # Arguments
    ///
    /// * `fuel` - Maximum number of tasks to complete before returning.
    ///   - `None`: Run until scheduler returns Idle and all in-flight complete
    ///   - `Some(n)`: Complete at most `n` tasks, then return
    ///
    /// Results are recorded to the state store as tasks complete.
    ///
    /// The loop uses `tokio::select!` to handle both task completions and subflow
    /// submissions concurrently. Subflow submissions create new `RunState` entries
    /// and add their tasks to the scheduler.
    pub(crate) async fn run_internal(
        &mut self,
        fuel: Option<std::num::NonZeroUsize>,
    ) -> Result<()> {
        let mut in_flight: FuturesUnordered<futures::future::BoxFuture<'static, TaskResult>> =
            FuturesUnordered::new();
        let mut remaining = fuel.map(|f| f.get());

        loop {
            // Check if we've exhausted fuel
            if remaining == Some(0) {
                // Wait for any in-flight tasks to complete before returning
                while let Some(task_result) = in_flight.next().await {
                    if let Some(retry_future) = self.complete_task(task_result).await? {
                        in_flight.push(retry_future);
                    }
                    self.checkpointer
                        .maybe_checkpoint(&self.runs, &self.recovered_subflows)
                        .await?;
                }
                return Ok(());
            }

            // Determine how many more tasks we can start.
            //
            // IMPORTANT: We always allow at least 1 task to start to avoid deadlock
            // when parent tasks are waiting for subflows to complete. If we ever computed
            // `max_to_start` as 0 here, then in a situation where only subflow work can
            // make progress, and parent tasks have saturated the `max_in_flight` then
            // the subflow tasks would never be scheduled and the parent would wait forever.
            //
            // The `.max(1)` below is a deliberate safeguard against that
            // deadlock scenario and should not be removed without revisiting
            // the concurrency/deadlock analysis.
            //
            // Future improvements could exclude "blocked" parent tasks from being counted
            // as in flight, or ensure that at least one task from a given subflow is
            // running, while not admitting arbitrarily many new tasks.
            let max_to_start = {
                let max_to_start = self.max_in_flight.saturating_sub(in_flight.len()).max(1);
                if let Some(r) = remaining {
                    max_to_start.min(r)
                } else {
                    max_to_start
                }
            };

            log::debug!(
                "Loop iteration: in_flight={}, max_in_flight={}, max_to_start={}",
                in_flight.len(),
                self.max_in_flight,
                max_to_start,
            );

            // Try to start new tasks
            if max_to_start > 0
                && let Some(tasks) = self.scheduler.select_next(max_to_start).into_tasks()
            {
                // Build TaskAttempt records and increment attempt counters before
                // spawning. Tasks may belong to different runs (parent and subflows),
                // so we group attempts by run_id into a single TasksStarted event
                // with RunTaskAttempts entries per run.
                let mut attempts_by_run: HashMap<uuid::Uuid, Vec<TaskAttempt>> = HashMap::new();

                for task in tasks.iter() {
                    let run_state = self
                        .run_state_mut(task.run_id)
                        .expect("run should exist for scheduled task");
                    let item = run_state.items_state_mut().item_mut(task.item_index);
                    let attempt = item.record_attempt(task.step_index);

                    attempts_by_run
                        .entry(task.run_id)
                        .or_default()
                        .push(TaskAttempt::new(task.item_index, task.step_index, attempt));
                }

                // Write a single TasksStarted event — this must be durable before we spawn
                // the task futures, so that recovery knows these tasks were attempted.
                let runs: Vec<RunTaskAttempts> = attempts_by_run
                    .into_iter()
                    .map(|(run_id, tasks)| RunTaskAttempts { run_id, tasks })
                    .collect();
                let seqno = self
                    .write_journal(JournalEvent::TasksStarted { runs })
                    .await?;

                // Sync step statuses to metadata: mark started steps as Running
                for task in tasks.iter() {
                    self.update_step_status(
                        task.run_id,
                        task.item_index,
                        task.step_index,
                        stepflow_core::status::StepStatus::Running,
                        None,
                        seqno,
                    )
                    .await?;
                }

                for task in tasks.into_iter() {
                    let future = self.prepare_task_future(task)?;
                    in_flight.push(future);
                }
            }

            // If no tasks in flight and no subflows could be submitted, we're done
            if in_flight.is_empty() {
                return Ok(());
            }

            // Wait for either a task to complete or a subflow submission
            tokio::select! {
                // Handle task completion
                Some(task_result) = in_flight.next() => {
                    if let Some(retry_future) = self.complete_task(task_result).await? {
                        in_flight.push(retry_future);
                    } else if let Some(r) = &mut remaining {
                        // Only count fuel for actual completions, not retries
                        *r = r.saturating_sub(1);
                    }
                    self.checkpointer.maybe_checkpoint(&self.runs, &self.recovered_subflows).await?;
                }
                // Handle subflow submission
                Some(submit_request) = self.submit_receiver.recv() => {
                    self.handle_submit_request(submit_request).await?;
                }
            }
        }
    }

    /// Handle a subflow submission request.
    ///
    /// Creates a new `RunState` for the subflow, initializes its items,
    /// and sends the response with the run ID and completion channel.
    ///
    async fn handle_submit_request(&mut self, request: SubflowRequest) -> Result<()> {
        let run_id = request.run_id;
        let parent_run_id = request.parent_run_id;
        let input_count = request.inputs.len();

        // Dedup check for recovered subflows: if a pre-crash subflow matches this
        // (parent_run_id, item_index, step_index, subflow_key), return its run_id.
        // The parent step's wait_for_completion will pick up the existing subflow
        // (completed subflows resolve immediately from the metadata store).
        //
        // This is the only dedup mechanism needed — run_ids are fresh Uuid::now_v7()
        // values per submit() call, so duplicate run_ids cannot arrive through the channel.
        let lookup_key = (
            parent_run_id,
            request.item_index,
            request.step_index,
            request.subflow_key,
        );
        if let Some(recovered_run_id) = self.recovered_subflows.remove(&lookup_key) {
            log::info!(
                "Matched recovered subflow: key=({}, {}, {}, {}) -> run_id={}",
                parent_run_id,
                request.item_index,
                request.step_index,
                request.subflow_key,
                recovered_run_id
            );
            let _ = request.response_tx.send(recovered_run_id);
            return Ok(());
        }

        // Clone variables before moving into RunState (needed for journal event)
        let variables = request.variables.clone();

        // Create the subflow's RunState
        let run_state = RunState::new_subflow(
            run_id,
            request.flow_id.clone(),
            self.root_run_id,
            parent_run_id,
            request.flow.clone(),
            request.inputs.clone(),
            request.variables,
        );

        // Store the run state
        self.runs.insert(run_id, run_state);

        // Journal FIRST: Record sub-run creation and parent association atomically.
        // This single event provides everything needed for recovery: the sub-run's
        // creation data AND the dedup mapping (parent_run_id, item_index, step_index,
        // subflow_key). All events for the execution tree share the same journal
        // (keyed by root_run_id).
        //
        // The journal write must happen before the metadata store write so the
        // journal remains the authoritative source of truth. See executor.rs
        // submit_run for the crash-window analysis.
        let created_at_seqno = self
            .write_journal(JournalEvent::SubRunCreated {
                run_id,
                flow_id: request.flow_id.clone(),
                inputs: request.inputs.clone(),
                variables,
                parent_run_id,
                item_index: request.item_index,
                step_index: request.step_index,
                subflow_key: request.subflow_key,
            })
            .await?;

        // Metadata store: Create run record with the journal offset.
        let mut run_params = CreateRunParams::new_subflow(
            run_id,
            request.flow_id,
            request.inputs,
            self.root_run_id,
            parent_run_id,
            created_at_seqno,
        );
        run_params.workflow_name = request.flow.name().map(|s| s.to_string());
        run_params.orchestrator_id = self.env.orchestrator_id().map(|id| id.as_str().to_string());
        if let Err(e) = self.metadata_store.create_run(run_params).await {
            log::error!(
                "Failed to create subflow run record for {}: {:?}",
                run_id,
                e
            );
        }
        log::debug!(
            "Subflow run record created: run_id={}, items={}",
            run_id,
            input_count
        );

        // Initialize the run and get ready tasks
        let (initial_tasks, is_complete, item_count) = {
            let run_state = self.run_state_mut(run_id).expect("run should exist");
            let initial_tasks = run_state.initialize_all();
            let is_complete = run_state.is_complete();
            let item_count = run_state.items_state().item_count();
            (initial_tasks, is_complete, item_count)
        };

        // Journal: Record subflow initialization (on the subflow run)
        self.write_steps_needed_events(run_id, item_count).await?;

        log::debug!(
            "Subflow initialized: run_id={}, initial_tasks={}",
            run_id,
            initial_tasks.len()
        );

        if !initial_tasks.is_empty() {
            self.scheduler.notify_new_tasks(&initial_tasks);
        } else if is_complete {
            // Subflow is complete with no tasks. This can happen for:
            // 1. Truly empty subflow (0 items) - no results to record
            // 2. Subflow with items but 0 steps - each item has an output to record
            if item_count == 0 {
                // Truly empty subflow: journal first, then update metadata.
                log::debug!(
                    "Truly empty subflow (0 items), marking as completed: run_id={}",
                    run_id
                );

                // Journal FIRST: record completion for recovery.
                let finished_seqno = self
                    .write_journal(JournalEvent::RunCompleted {
                        run_id,
                        status: stepflow_core::status::ExecutionStatus::Completed,
                    })
                    .await?;

                if let Err(e) = self
                    .metadata_store
                    .update_run_status(
                        run_id,
                        stepflow_core::status::ExecutionStatus::Completed,
                        Some(finished_seqno),
                    )
                    .await
                {
                    log::error!(
                        "Failed to update empty subflow status for run {}: {:?}",
                        run_id,
                        e
                    );
                }
                self.runs.remove(&run_id);
            } else {
                // Subflow with items but 0 steps: record item results (state store notifies on completion)
                log::debug!(
                    "Subflow complete with {} items but no steps, recording results: run_id={}",
                    item_count,
                    run_id
                );
                for item_index in 0..item_count {
                    let result = self.resolve_item_output(run_id, item_index);
                    // No steps executed, so step_statuses is empty
                    if let Err(e) = self
                        .metadata_store
                        .record_item_result(run_id, item_index as usize, result, Vec::new())
                        .await
                    {
                        log::error!(
                            "Failed to record empty subflow item result for run {} item {}: {:?}",
                            run_id,
                            item_index,
                            e
                        );
                    }
                }

                // Journal FIRST, then update metadata: subflow completed with items but 0 steps.
                let finished_seqno = self
                    .write_journal(JournalEvent::RunCompleted {
                        run_id,
                        status: stepflow_core::status::ExecutionStatus::Completed,
                    })
                    .await?;

                if let Err(e) = self
                    .metadata_store
                    .update_run_status(
                        run_id,
                        stepflow_core::status::ExecutionStatus::Completed,
                        Some(finished_seqno),
                    )
                    .await
                {
                    log::error!(
                        "Failed to update zero-step subflow status for run {}: {:?}",
                        run_id,
                        e
                    );
                }
                self.runs.remove(&run_id);
            }
        }

        // Acknowledge the submission by sending back the run_id.
        // The submitter will use state_store.wait_for_completion(run_id) to wait.
        let _ = request.response_tx.send(run_id);

        log::debug!(
            "Subflow submitted: run_id={}, parent_run_id={}, root_run_id={}",
            run_id,
            parent_run_id,
            self.root_run_id
        );

        Ok(())
    }

    /// Execute all items to completion.
    ///
    /// This method drives execution until all items complete, then:
    /// 1. Records item results to the state store
    /// 2. Updates the run status (Completed or Failed)
    ///
    /// Use `state_store.get_item_results(run_id, order)` after completion
    /// to retrieve the results.
    ///
    /// Tasks are executed concurrently up to `max_in_flight` at a time.
    async fn execute_to_completion(&mut self) -> Result<()> {
        let run_id = self.root_run_id;
        let flow_id = self.root_run_state().flow_id().clone();

        // Set run_id in diagnostic context
        let _run_guard = RunInfoGuard::new(flow_id.to_string(), run_id.to_string());

        log::info!(
            "Starting items execution: run_id={}, item_count={}, max_in_flight={}",
            run_id,
            self.root_run_state().item_count(),
            self.max_in_flight
        );

        // Initialize all items and get initial ready tasks.
        // We reset the scheduler but then re-add tasks from ALL runs (not just root),
        // so subflows that were submitted before execute_to_completion are preserved.
        self.scheduler.reset();

        // Initialize and collect tasks from ALL runs
        let run_ids: Vec<Uuid> = self.runs.keys().copied().collect();
        let mut initial_tasks = Vec::new();
        for rid in run_ids {
            let item_count = if let Some(run_state) = self.runs.get_mut(&rid) {
                initial_tasks.extend(run_state.initialize_all());
                run_state.items_state().item_count()
            } else {
                continue;
            };

            // For recovered subflows, only write StepsNeeded if the journal
            // doesn't already contain it (crash window between SubRunCreated
            // and StepsNeeded). Non-recovered runs always need it written.
            if self.recovered_run_ids.contains(&rid)
                && !self.runs_needing_step_updates.contains(&rid)
            {
                continue;
            }

            self.write_steps_needed_events(rid, item_count).await?;
        }

        self.scheduler.notify_new_tasks(&initial_tasks);

        // Run until complete or deadlock
        self.run_internal(None).await?;

        // Check for deadlock: run() returned but not complete
        if self.root_run_state().items_state().incomplete() > 0 {
            return Err(error_stack::report!(ExecutionError::Deadlock)
                .attach_printable("No tasks ready and none in flight"));
        }

        // Record item results and determine final status
        let mut has_failures = false;
        let item_count = self.root_run_state().item_count();
        for item_index in 0..item_count {
            let result = self.resolve_item_output(run_id, item_index);
            if matches!(&result, FlowResult::Failed(_)) {
                has_failures = true;
            }
            let step_statuses = self
                .root_run_state()
                .items_state()
                .get_item_step_statuses(item_index);
            if let Err(e) = self
                .metadata_store
                .record_item_result(run_id, item_index as usize, result, step_statuses)
                .await
            {
                log::error!(
                    "Failed to record item result for run {} item {}: {:?}",
                    run_id,
                    item_index,
                    e
                );
            }
        }

        // Update run status
        let final_status = if has_failures {
            stepflow_core::status::ExecutionStatus::Failed
        } else {
            stepflow_core::status::ExecutionStatus::Completed
        };

        // Journal FIRST: Record run completion
        let finished_seqno = self
            .write_journal(JournalEvent::RunCompleted {
                run_id,
                status: final_status,
            })
            .await?;

        if let Err(e) = self
            .metadata_store
            .update_run_status(run_id, final_status, Some(finished_seqno))
            .await
        {
            log::error!("Failed to update run status for {}: {:?}", run_id, e);
        }
        // State store's update_run_status will notify waiters via RunCompletionNotifier

        log::info!(
            "Items execution completed: run_id={}, item_count={}, status={:?}",
            run_id,
            item_count,
            final_status
        );

        // Clean up checkpoints now that the run is complete.
        self.checkpointer.cleanup().await;

        Ok(())
    }

    /// Prepare a task for execution and return an owned future.
    ///
    /// This uses [`StepRunner`] to handle plugin lookup, context creation,
    /// and step execution. The future is fully owned and can be polled
    /// without borrowing self.
    fn prepare_task_future(
        &self,
        task: Task,
    ) -> Result<futures::future::BoxFuture<'static, TaskResult>> {
        use futures::future::FutureExt as _;
        use stepflow_plugin::RunContext;

        // Look up the run state for this task
        let run_state = self.run_state(task.run_id).ok_or_else(|| {
            error_stack::report!(ExecutionError::Deadlock)
                .attach_printable(format!("Unknown run_id {} in task", task.run_id))
        })?;

        // Get flow and resolve step input while we have access to state
        let item = run_state.items_state().item(task.item_index);
        let flow = item.flow().clone();
        let step = flow.step(task.step_index);
        let step_input = step.input.resolve(item);
        let attempt = item.attempt_count(task.step_index);

        // Create StepId for error handling (before moving flow into runner)
        let step_id = StepId::for_step(flow.clone(), task.step_index);
        let component = step.component.to_string();

        // Get the flow_id for this run
        let flow_id = run_state.flow_id().clone();

        // Create RunContext with subflow submitter scoped to this task.
        // The submitter carries (parent_run_id, item_index, step_index) so that
        // subflow submissions include full parent task context for recovery.
        let submitter = self
            .submit_sender
            .for_task(task.run_id, task.item_index, task.step_index);
        let run_context = Arc::new(
            RunContext::new(task.run_id, flow, flow_id, self.env.clone()).with_submitter(submitter),
        );

        // Generate a unique task ID for TaskRegistry tracking
        let task_id = uuid::Uuid::now_v7().to_string();

        // Create step runner with all execution context
        let runner = StepRunner::new(task.step_index, task_id, step_input, run_context, attempt);

        // Create an owned future that runs the step and handles errors
        let future = async move {
            let step_result = match runner.run().await {
                Ok(result) => result,
                Err(error) => {
                    // Infrastructure error - log and convert to FlowResult::Failed
                    log::error!("Internal error executing step '{}': {:?}", step_id, error);
                    let flow_error =
                        stepflow_core::FlowError::from_error_stack(error.attach_printable(
                            format!("Internal error executing step '{}'", step_id),
                        ));
                    StepRunResult::new(step_id, component, FlowResult::Failed(flow_error))
                }
            };
            TaskResult::new(task, step_result)
        };

        Ok(future.boxed())
    }

    /// Complete a task and update state.
    ///
    /// Returns `Some(future)` if the task should be retried (push into `in_flight`),
    /// or `None` if the task completed normally.
    async fn complete_task(
        &mut self,
        task_result: TaskResult,
    ) -> Result<Option<futures::future::BoxFuture<'static, TaskResult>>> {
        let task = task_result.task();
        let run_id = task.run_id;
        let component_path = task_result.step.component().to_string();

        // Record step execution metric
        let outcome = if task_result.is_success() {
            "success"
        } else {
            "failed"
        };
        stepflow_observability::metrics::record_step_execution(&component_path, outcome);

        // ---------------------------------------------------------------
        // Retry decision: check if we should retry instead of completing
        // ---------------------------------------------------------------
        if task_result.step.is_failed()
            && let Some(retry_future) = self.maybe_retry_task(&task_result, &component_path).await?
        {
            return Ok(Some(retry_future));
        }

        // ---------------------------------------------------------------
        // Normal completion path
        // ---------------------------------------------------------------
        let result = task_result.step.result.clone();

        // Update state and get newly ready tasks
        let new_tasks = if let Some(run_state) = self.run_state_mut(run_id) {
            run_state
                .items_state_mut()
                .complete_task_and_get_ready(task, result.clone())
        } else {
            log::warn!("complete_task called for unknown run_id: {}", run_id);
            Vec::new()
        };

        log::info!(
            "Task completed: run_id={}, item={}, step={}, result={}, new_tasks={}",
            run_id,
            task.item_index,
            task.step_index,
            if matches!(&result, stepflow_core::FlowResult::Failed(_)) {
                "failed"
            } else {
                "ok"
            },
            new_tasks.len()
        );

        // Journal: Record task completion
        let completed_seqno = self
            .write_journal(JournalEvent::TaskCompleted {
                run_id,
                item_index: task.item_index,
                step_index: task.step_index,
                result: result.clone(),
            })
            .await?;

        // Sync step status to metadata: mark step as Completed or Failed
        let step_status = if matches!(&result, FlowResult::Failed(_)) {
            stepflow_core::status::StepStatus::Failed
        } else {
            stepflow_core::status::StepStatus::Completed
        };
        self.update_step_status(
            run_id,
            task.item_index,
            task.step_index,
            step_status,
            Some(result.clone()),
            completed_seqno,
        )
        .await?;

        // Journal: Record newly unblocked steps (grouped by item)
        // Group new_tasks by item_index for efficient journalling
        let mut unblocked_by_item: HashMap<u32, Vec<usize>> = HashMap::new();
        for new_task in &new_tasks {
            unblocked_by_item
                .entry(new_task.item_index)
                .or_default()
                .push(new_task.step_index);
        }
        for (item_index, step_indices) in &unblocked_by_item {
            let seqno = self
                .write_journal(JournalEvent::StepsUnblocked {
                    run_id,
                    item_index: *item_index,
                    step_indices: step_indices.clone(),
                })
                .await?;

            // Sync step statuses to metadata: mark unblocked steps as Runnable
            for &step_index in step_indices {
                self.update_step_status(
                    run_id,
                    *item_index,
                    step_index,
                    stepflow_core::status::StepStatus::Runnable,
                    None,
                    seqno,
                )
                .await?;
            }
        }

        // Notify scheduler
        self.scheduler.task_completed(task);
        if !new_tasks.is_empty() {
            self.scheduler.notify_new_tasks(&new_tasks);
        }

        // Check if this run is now complete
        if let Some(run_state) = self.run_state(run_id)
            && run_state.is_complete()
        {
            // Check if all items succeeded or any failed
            let items_state = run_state.items_state();
            let has_failures =
                (0..items_state.item_count()).any(|i| items_state.item(i).is_failed());

            // For subflows (non-root runs), record item results to state store immediately.
            // Root runs are finalized in execute_to_completion.
            // The state store will notify waiters when results are recorded.
            if run_id != self.root_run_id {
                let final_status = if has_failures {
                    stepflow_core::status::ExecutionStatus::Failed
                } else {
                    stepflow_core::status::ExecutionStatus::Completed
                };

                // Collect item results while we have immutable access to the run state.
                // These are used after the journal write (which requires &mut self).
                let item_results: Vec<_> = (0..items_state.item_count())
                    .map(|item_index| {
                        let result = self.resolve_item_output(run_id, item_index);
                        let step_statuses = items_state.get_item_step_statuses(item_index);
                        (item_index, result, step_statuses)
                    })
                    .collect();

                log::info!(
                    "Subflow run complete: run_id={}, status={:?}, root_run_id={}",
                    run_id,
                    final_status,
                    self.root_run_id
                );

                // Journal FIRST: record subflow completion for recovery.
                // The journal is the source of truth — if we crash after this
                // write but before updating the metadata store, recovery will
                // detect the discrepancy and sync the metadata store.
                let finished_seqno = self
                    .write_journal(JournalEvent::RunCompleted {
                        run_id,
                        status: final_status,
                    })
                    .await?;

                // Metadata store: record item results and update status.
                // This notifies any waiters (e.g., the parent step) that the
                // subflow has completed.
                for (item_index, result, step_statuses) in item_results {
                    if let Err(e) = self
                        .metadata_store
                        .record_item_result(run_id, item_index as usize, result, step_statuses)
                        .await
                    {
                        log::error!(
                            "Failed to record subflow item result for run {} item {}: {:?}",
                            run_id,
                            item_index,
                            e
                        );
                    }
                }

                if let Err(e) = self
                    .metadata_store
                    .update_run_status(run_id, final_status, Some(finished_seqno))
                    .await
                {
                    log::error!(
                        "Failed to update subflow run status for {}: {:?}",
                        run_id,
                        e
                    );
                }

                // Evict completed subflow from in-memory state.
                // Results are persisted to the metadata store; RunState is redundant.
                self.runs.remove(&run_id);
                log::debug!(
                    "Evicted completed subflow run_id={}, remaining_runs={}",
                    run_id,
                    self.runs.len()
                );
            }
            // Root runs are handled in execute_to_completion
        }

        Ok(None)
    }

    /// Evaluate whether a failed task should be retried and, if so, build the retry future.
    ///
    /// Returns `Some(future)` if retrying, `None` if the task should complete normally.
    async fn maybe_retry_task(
        &mut self,
        task_result: &TaskResult,
        component_path: &str,
    ) -> Result<Option<futures::future::BoxFuture<'static, TaskResult>>> {
        use futures::future::FutureExt as _;

        let task = task_result.task();
        let run_id = task.run_id;
        let is_transport = task_result.step.result.is_transport_error();
        let is_component_execution = task_result.step.result.is_component_execution_error();

        // 1. Read limits (immutable borrows — no conflict with runs)
        let transport_max = self.retry_config().transport_max_retries;
        let component_max = self.get_component_max_retries(task);

        // 2. Check budget against item state
        let run_state = self
            .run_state_mut(run_id)
            .expect("run should exist for retry");
        let item = run_state.items_state_mut().item_mut(task.item_index);
        let retry = item.retry_state_mut(task.step_index);

        let should_retry = if is_transport {
            retry.transport_retries < transport_max
        } else if is_component_execution {
            // Only component execution errors (-32100 to -32199) are eligible
            // for onError retry. Protocol, orchestrator, and JSON-RPC errors
            // indicate structural problems that won't resolve on retry.
            if let Some(max) = component_max {
                retry.component_retries < max
            } else {
                false
            }
        } else {
            false
        };

        if !should_retry {
            // Budget exhausted or not a retriable error — record metric if budget existed
            let had_budget = if is_transport {
                transport_max > 0
            } else {
                component_max.is_some()
            };
            if had_budget {
                let reason = if is_transport {
                    "transport_error"
                } else {
                    "component_error"
                };
                stepflow_observability::metrics::record_step_retries_exhausted(
                    reason,
                    component_path,
                );
            }
            return Ok(None);
        }

        // ---------------------------------------------------------------
        // Retry path
        // ---------------------------------------------------------------
        let reason = if is_transport {
            "transport_error"
        } else {
            "component_error"
        };

        // 3. Increment per-reason counter in item state
        let retry_count = if is_transport {
            retry.transport_retries += 1;
            retry.transport_retries
        } else {
            retry.component_retries += 1;
            retry.component_retries
        };

        // Shared backoff for all retries (transport and component)
        let delay = self.retry_config().delay(retry_count);

        // 2. For transport errors, prepare the plugin for retry (e.g. restart subprocess)
        if is_transport
            && let Ok((plugin, _)) = self.lookup_plugin_for_task(task)
            && let Err(e) = plugin.prepare_for_retry().await
        {
            log::warn!(
                "prepare_for_retry failed for step '{}': {:?}",
                task_result.step.step_name(),
                e
            );
            // Continue with retry anyway — the execution attempt will fail
            // and be handled by the next retry cycle
        }

        // 3. Increment attempt counter in item state
        let new_attempt = {
            let run_state = self
                .run_state_mut(run_id)
                .expect("run should exist for retry");
            let item = run_state.items_state_mut().item_mut(task.item_index);
            item.record_attempt(task.step_index)
        };

        // 4. Journal the new attempt
        let seqno = self
            .write_journal(JournalEvent::TasksStarted {
                runs: vec![RunTaskAttempts {
                    run_id,
                    tasks: vec![TaskAttempt::new(
                        task.item_index,
                        task.step_index,
                        new_attempt,
                    )],
                }],
            })
            .await?;

        // Sync step status to metadata: mark retried step as Running again
        self.update_step_status(
            run_id,
            task.item_index,
            task.step_index,
            stepflow_core::status::StepStatus::Running,
            None,
            seqno,
        )
        .await?;

        // 5. Record retry metric
        stepflow_observability::metrics::record_step_retry(reason, component_path);

        log::info!(
            "Retrying step '{}' (reason: {}, attempt: {}, delay: {:?})",
            task_result.step.step_name(),
            reason,
            new_attempt,
            delay
        );

        // 6. Build retry future: sleep then re-execute
        let task_future = self.prepare_task_future(task)?;
        let component_for_metric = component_path.to_string();
        let retry_future = async move {
            stepflow_observability::metrics::record_pending_execution_start(&component_for_metric);
            tokio::time::sleep(delay).await;
            stepflow_observability::metrics::record_pending_execution_end(&component_for_metric);
            task_future.await
        };

        Ok(Some(retry_future.boxed()))
    }

    /// Get the transport retry config from the environment.
    fn retry_config(&self) -> stepflow_core::RetryConfig {
        self.env
            .get::<stepflow_core::RetryConfig>()
            .expect("RetryConfig must be in environment")
    }

    /// Look up the component max_retries from the step's ErrorAction, if it's a Retry action.
    fn get_component_max_retries(&self, task: Task) -> Option<u32> {
        let run_state = self.run_state(task.run_id)?;
        let item = run_state.items_state().item(task.item_index);
        let step = item.flow().step(task.step_index);
        step.on_error_or_default().max_retries()
    }

    /// Look up the plugin for a task's step component, using the step's resolved input for routing.
    fn lookup_plugin_for_task(
        &self,
        task: Task,
    ) -> Result<(std::sync::Arc<stepflow_plugin::DynPlugin<'static>>, String)> {
        let run_state = self.run_state(task.run_id).ok_or_else(|| {
            error_stack::report!(ExecutionError::Deadlock)
                .attach_printable(format!("Unknown run_id {} for retry lookup", task.run_id))
        })?;
        let item = run_state.items_state().item(task.item_index);
        let step = item.flow().step(task.step_index);

        // Resolve step input for routing (dependencies are complete at this point)
        let input = match step.input.resolve(item) {
            FlowResult::Success(v) => v,
            FlowResult::Failed(_) => stepflow_core::values::ValueRef::new(serde_json::Value::Null),
        };

        self.env
            .get_plugin_and_component(&step.component, input)
            .change_context(ExecutionError::RouterError)
    }

    /// Resolve the output for a completed item.
    fn resolve_item_output(&self, run_id: Uuid, item_index: u32) -> FlowResult {
        let run_state = self
            .run_state(run_id)
            .expect("run_id should exist when resolving output");
        let item = run_state.items_state().item(item_index);
        let flow = item.flow();
        flow.output().resolve(item)
    }

    /// Get the run ID for this execution (alias for root_run_id).
    pub fn run_id(&self) -> Uuid {
        self.root_run_id
    }

    /// Get read access to the root execution state.
    pub fn state(&self) -> &ItemsState {
        self.root_run_state().items_state()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow_executor_builder::FlowExecutorBuilder;
    use crate::scheduler::{BreadthFirstScheduler, DepthFirstScheduler};
    use crate::testing::{MockExecutorBuilder, create_executor_with_behaviors, create_linear_flow};
    use serde_json::json;
    use stepflow_core::status::ExecutionStatus;
    use stepflow_core::values::ValueRef;
    use stepflow_core::workflow::Flow;
    use stepflow_core::workflow::StepBuilder;
    use stepflow_core::{BlobId, ValueExpr};
    use stepflow_dtos::ResultOrder;
    use stepflow_state::MetadataStoreExt as _;

    /// Helper to create a RunState for tests with a single input.
    fn create_run_state(flow: Arc<Flow>, flow_id: BlobId, input: ValueRef) -> RunState {
        let run_id = Uuid::now_v7();
        RunState::new(run_id, flow_id, flow, vec![input], HashMap::new())
    }

    /// Helper to create a RunState for tests with multiple inputs.
    fn create_run_state_batch(flow: Arc<Flow>, flow_id: BlobId, inputs: Vec<ValueRef>) -> RunState {
        let run_id = Uuid::now_v7();
        RunState::new(run_id, flow_id, flow, inputs, HashMap::new())
    }

    /// Helper to create a RunState for tests with variables.
    fn create_run_state_with_vars(
        flow: Arc<Flow>,
        flow_id: BlobId,
        input: ValueRef,
        variables: HashMap<String, ValueRef>,
    ) -> RunState {
        let run_id = Uuid::now_v7();
        RunState::new(run_id, flow_id, flow, vec![input], variables)
    }

    #[tokio::test]
    async fn test_single_item_execution() {
        let flow = Arc::new(create_linear_flow(2));
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let input = ValueRef::new(json!({"x": 1}));

        let executor = MockExecutorBuilder::new().build().await;
        let state_store = executor.metadata_store().clone();
        let run_state = create_run_state(flow.clone(), flow_id, input);
        let run_id = run_state.run_id();

        let mut items_executor = FlowExecutorBuilder::new(executor, run_state)
            .scheduler(Box::new(DepthFirstScheduler::new()))
            .build()
            .await
            .unwrap();
        items_executor.execute_to_completion().await.unwrap();
        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(matches!(results[0].result, Some(FlowResult::Success(_))));
    }

    #[tokio::test]
    async fn test_batch_execution() {
        let flow = Arc::new(create_linear_flow(2));
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let inputs = vec![
            ValueRef::new(json!({"x": 1})),
            ValueRef::new(json!({"x": 2})),
            ValueRef::new(json!({"x": 3})),
        ];

        let executor = MockExecutorBuilder::new().build().await;
        let state_store = executor.metadata_store().clone();
        let run_state = create_run_state_batch(flow.clone(), flow_id, inputs);
        let run_id = run_state.run_id();

        let mut items_executor = FlowExecutorBuilder::new(executor, run_state)
            .scheduler(Box::new(DepthFirstScheduler::new()))
            .build()
            .await
            .unwrap();
        items_executor.execute_to_completion().await.unwrap();
        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        assert_eq!(results.len(), 3);
        for result in &results {
            assert!(matches!(result.result, Some(FlowResult::Success(_))));
        }
    }

    #[tokio::test]
    async fn test_depth_first_vs_breadth_first() {
        // This test verifies that both schedulers produce correct results
        // (ordering differences are tested in scheduler tests)
        let flow = Arc::new(create_linear_flow(2));
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let inputs = vec![
            ValueRef::new(json!({"x": 1})),
            ValueRef::new(json!({"x": 2})),
        ];

        // Test with depth-first
        let executor_df = MockExecutorBuilder::new().build().await;
        let state_store_df = executor_df.metadata_store();
        let run_state_df = create_run_state_batch(flow.clone(), flow_id.clone(), inputs.clone());
        let run_id_df = run_state_df.run_id();
        let mut items_executor_df = FlowExecutorBuilder::new(executor_df.clone(), run_state_df)
            .scheduler(Box::new(DepthFirstScheduler::new()))
            .build()
            .await
            .unwrap();

        items_executor_df.execute_to_completion().await.unwrap();
        let results_df = state_store_df
            .get_item_results(run_id_df, ResultOrder::ByIndex)
            .await
            .unwrap();

        // Test with breadth-first
        let executor_bf = MockExecutorBuilder::new().build().await;
        let state_store_bf = executor_bf.metadata_store();
        let run_state_bf = create_run_state_batch(flow.clone(), flow_id, inputs);
        let run_id_bf = run_state_bf.run_id();
        let mut items_executor_bf = FlowExecutorBuilder::new(executor_bf.clone(), run_state_bf)
            .scheduler(Box::new(BreadthFirstScheduler::new()))
            .build()
            .await
            .unwrap();

        items_executor_bf.execute_to_completion().await.unwrap();
        let results_bf = state_store_bf
            .get_item_results(run_id_bf, ResultOrder::ByIndex)
            .await
            .unwrap();

        // Both should produce same number of results
        assert_eq!(results_df.len(), results_bf.len());
        assert_eq!(results_df.len(), 2);
    }

    #[tokio::test]
    async fn test_error_isolation_between_items() {
        // Test that when one item fails, other items still succeed
        let flow = Arc::new(create_linear_flow(1));
        let flow_id = BlobId::from_flow(&flow).unwrap();

        // Create behaviors: x=1 fails, x=2 and x=3 succeed
        let behaviors = vec![
            (
                json!({"x": 1}),
                FlowResult::Failed(stepflow_core::FlowError::new(500, "Item 1 failed")),
            ),
            (
                json!({"x": 2}),
                FlowResult::Success(ValueRef::new(json!({"result": "ok"}))),
            ),
            (
                json!({"x": 3}),
                FlowResult::Success(ValueRef::new(json!({"result": "ok"}))),
            ),
        ];

        let executor = create_executor_with_behaviors(behaviors).await;
        let state_store = executor.metadata_store().clone();

        let inputs = vec![
            ValueRef::new(json!({"x": 1})), // Will fail
            ValueRef::new(json!({"x": 2})), // Will succeed
            ValueRef::new(json!({"x": 3})), // Will succeed
        ];

        let run_state = create_run_state_batch(flow.clone(), flow_id, inputs);
        let run_id = run_state.run_id();
        let mut items_executor = FlowExecutorBuilder::new(executor, run_state)
            .scheduler(Box::new(DepthFirstScheduler::new()))
            .build()
            .await
            .unwrap();

        items_executor.execute_to_completion().await.unwrap();
        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        // All items should have results
        assert_eq!(results.len(), 3);

        // Item 0 should fail
        assert!(
            matches!(&results[0].result, Some(FlowResult::Failed(e)) if e.message.contains("Item 1 failed"))
        );

        // Items 1 and 2 should succeed
        assert!(matches!(&results[1].result, Some(FlowResult::Success(_))));
        assert!(matches!(&results[2].result, Some(FlowResult::Success(_))));
    }

    #[tokio::test]
    async fn test_error_isolation_with_breadth_first() {
        // Same test but with breadth-first scheduler to ensure isolation works regardless of order
        let flow = Arc::new(create_linear_flow(1));
        let flow_id = BlobId::from_flow(&flow).unwrap();

        let behaviors = vec![
            (
                json!({"x": 1}),
                FlowResult::Success(ValueRef::new(json!({"result": "ok"}))),
            ),
            (
                json!({"x": 2}),
                FlowResult::Failed(stepflow_core::FlowError::new(500, "Middle item failed")),
            ),
            (
                json!({"x": 3}),
                FlowResult::Success(ValueRef::new(json!({"result": "ok"}))),
            ),
        ];

        let executor = create_executor_with_behaviors(behaviors).await;
        let state_store = executor.metadata_store().clone();

        let inputs = vec![
            ValueRef::new(json!({"x": 1})), // Will succeed
            ValueRef::new(json!({"x": 2})), // Will fail
            ValueRef::new(json!({"x": 3})), // Will succeed
        ];

        let run_state = create_run_state_batch(flow.clone(), flow_id, inputs);
        let run_id = run_state.run_id();
        let mut items_executor = FlowExecutorBuilder::new(executor, run_state)
            .scheduler(Box::new(BreadthFirstScheduler::new()))
            .build()
            .await
            .unwrap();
        items_executor.execute_to_completion().await.unwrap();
        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        assert_eq!(results.len(), 3);
        assert!(matches!(&results[0].result, Some(FlowResult::Success(_))));
        assert!(matches!(&results[1].result, Some(FlowResult::Failed(_))));
        assert!(matches!(&results[2].result, Some(FlowResult::Success(_))));
    }

    #[tokio::test]
    async fn test_concurrent_execution_respects_max_in_flight() {
        // Test that execution works with different max_in_flight settings
        let flow = Arc::new(create_linear_flow(1));
        let flow_id = BlobId::from_flow(&flow).unwrap();

        // Create many items
        let behaviors: Vec<_> = (1..=10)
            .map(|i| {
                (
                    json!({"x": i}),
                    FlowResult::Success(ValueRef::new(json!({"result": i}))),
                )
            })
            .collect();

        let executor = create_executor_with_behaviors(behaviors).await;
        let state_store = executor.metadata_store().clone();

        let inputs: Vec<_> = (1..=10).map(|i| ValueRef::new(json!({"x": i}))).collect();

        // Execute with max_in_flight = 5
        let run_state = create_run_state_batch(flow.clone(), flow_id, inputs);
        let run_id = run_state.run_id();
        let mut items_executor = FlowExecutorBuilder::new(executor, run_state)
            .max_concurrency(5)
            .scheduler(Box::new(BreadthFirstScheduler::new()))
            .build()
            .await
            .unwrap();
        items_executor.execute_to_completion().await.unwrap();
        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        // All items should complete successfully
        assert_eq!(results.len(), 10);
        for result in &results {
            assert!(matches!(result.result, Some(FlowResult::Success(_))));
        }
    }

    #[tokio::test]
    async fn test_concurrent_execution_with_max_in_flight_one() {
        // Test that execution still works with max_in_flight = 1 (sequential)
        let flow = Arc::new(create_linear_flow(1));
        let flow_id = BlobId::from_flow(&flow).unwrap();

        let behaviors: Vec<_> = (1..=5)
            .map(|i| {
                (
                    json!({"x": i}),
                    FlowResult::Success(ValueRef::new(json!({"result": i}))),
                )
            })
            .collect();

        let executor = create_executor_with_behaviors(behaviors).await;
        let state_store = executor.metadata_store().clone();

        let inputs: Vec<_> = (1..=5).map(|i| ValueRef::new(json!({"x": i}))).collect();

        // Execute with max_in_flight = 1 (effectively sequential)
        let run_state = create_run_state_batch(flow.clone(), flow_id, inputs);
        let run_id = run_state.run_id();
        let mut items_executor = FlowExecutorBuilder::new(executor, run_state)
            .max_concurrency(1)
            .scheduler(Box::new(DepthFirstScheduler::new()))
            .build()
            .await
            .unwrap();
        items_executor.execute_to_completion().await.unwrap();
        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        // All items should complete successfully
        assert_eq!(results.len(), 5);
        for result in &results {
            assert!(matches!(result.result, Some(FlowResult::Success(_))));
        }
    }

    #[tokio::test]
    async fn test_empty_inputs() {
        // Test with no inputs - should return empty results
        let flow = Arc::new(create_linear_flow(1));
        let flow_id = BlobId::from_flow(&flow).unwrap();

        let executor = MockExecutorBuilder::new().build().await;
        let state_store = executor.metadata_store().clone();
        let run_state = create_run_state_batch(flow.clone(), flow_id, vec![]);
        let run_id = run_state.run_id();

        let mut items_executor = FlowExecutorBuilder::new(executor, run_state)
            .build()
            .await
            .unwrap();

        items_executor.execute_to_completion().await.unwrap();
        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_all_items_fail() {
        // Test that all items failing doesn't cause panic
        let flow = Arc::new(create_linear_flow(1));
        let flow_id = BlobId::from_flow(&flow).unwrap();

        // Create behaviors where all items fail
        let behaviors = vec![
            (
                json!({"x": 1}),
                FlowResult::Failed(stepflow_core::FlowError::new(500, "Error 1")),
            ),
            (
                json!({"x": 2}),
                FlowResult::Failed(stepflow_core::FlowError::new(500, "Error 2")),
            ),
            (
                json!({"x": 3}),
                FlowResult::Failed(stepflow_core::FlowError::new(500, "Error 3")),
            ),
        ];

        let executor = create_executor_with_behaviors(behaviors).await;
        let state_store = executor.metadata_store().clone();

        let inputs = vec![
            ValueRef::new(json!({"x": 1})),
            ValueRef::new(json!({"x": 2})),
            ValueRef::new(json!({"x": 3})),
        ];

        let run_state = create_run_state_batch(flow.clone(), flow_id, inputs);
        let run_id = run_state.run_id();
        let mut items_executor = FlowExecutorBuilder::new(executor, run_state)
            .build()
            .await
            .unwrap();

        items_executor.execute_to_completion().await.unwrap();
        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        // All items should have failed results
        assert_eq!(results.len(), 3);
        for result in &results {
            assert!(matches!(result.result, Some(FlowResult::Failed(_))));
        }
    }

    #[tokio::test]
    async fn test_single_step_flow() {
        // Test minimal flow with just one step
        let flow = Arc::new(create_linear_flow(1));
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let input = ValueRef::new(json!({"x": 1}));

        let executor = MockExecutorBuilder::new().build().await;
        let state_store = executor.metadata_store().clone();
        let run_state = create_run_state(flow.clone(), flow_id, input);
        let run_id = run_state.run_id();

        let mut items_executor = FlowExecutorBuilder::new(executor, run_state)
            .build()
            .await
            .unwrap();

        items_executor.execute_to_completion().await.unwrap();
        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(matches!(results[0].result, Some(FlowResult::Success(_))));
    }

    #[tokio::test]
    async fn test_run_with_fuel_limit() {
        // Test that fuel limits partial execution
        // Use chain flow so steps execute sequentially (each depends on previous)
        use crate::testing::create_chain_flow;

        let flow = Arc::new(create_chain_flow(3));
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let input = ValueRef::new(json!({"x": 1}));

        let executor = MockExecutorBuilder::new().build().await;
        let run_state = create_run_state(flow.clone(), flow_id, input);

        let mut items_executor = FlowExecutorBuilder::new(executor, run_state)
            .scheduler(Box::new(DepthFirstScheduler::new()))
            .build()
            .await
            .unwrap();

        // Initialize and get initial tasks
        items_executor.scheduler.reset();
        let root_run_id = items_executor.root_run_id();
        let initial_tasks = items_executor
            .runs
            .get_mut(&root_run_id)
            .unwrap()
            .initialize_all();
        items_executor.scheduler.notify_new_tasks(&initial_tasks);

        // Run with fuel=1 - should complete exactly 1 task
        items_executor
            .run_internal(Some(std::num::NonZeroUsize::new(1).unwrap()))
            .await
            .unwrap();

        // Should not be complete yet (only 1 of 3 steps done)
        assert!(items_executor.state().incomplete() > 0);

        // Run with fuel=1 again
        items_executor
            .run_internal(Some(std::num::NonZeroUsize::new(1).unwrap()))
            .await
            .unwrap();

        // Still not complete (2 of 3 steps done)
        assert!(items_executor.state().incomplete() > 0);

        // Run with fuel=None to complete
        items_executor.run_internal(None).await.unwrap();

        // Now should be complete
        assert_eq!(items_executor.state().incomplete(), 0);
    }

    #[tokio::test]
    async fn test_chain_flow_dependencies() {
        // Test a flow where steps depend on previous steps
        use crate::testing::create_chain_flow;

        let flow = Arc::new(create_chain_flow(3));
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let input = ValueRef::new(json!({"x": 1}));

        let executor = MockExecutorBuilder::new().build().await;
        let state_store = executor.metadata_store().clone();
        let run_state = create_run_state(flow.clone(), flow_id, input);
        let run_id = run_state.run_id();

        let mut items_executor = FlowExecutorBuilder::new(executor, run_state)
            .scheduler(Box::new(DepthFirstScheduler::new()))
            .build()
            .await
            .unwrap();

        items_executor.execute_to_completion().await.unwrap();

        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(matches!(results[0].result, Some(FlowResult::Success(_))));
    }

    #[tokio::test]
    async fn test_diamond_flow_parallel_execution() {
        // Test diamond DAG: A → B, A → C, B+C → D
        // B and C should be able to execute in parallel after A completes
        use crate::testing::create_diamond_flow;

        let flow = Arc::new(create_diamond_flow());
        let flow_id = BlobId::from_flow(&flow).unwrap();

        // Create behaviors for each step input
        let behaviors = vec![
            // Step A receives input
            (
                json!({"x": 1}),
                FlowResult::Success(ValueRef::new(json!({"a": "result"}))),
            ),
            // Steps B and C receive A's output
            (
                json!({"a": "result"}),
                FlowResult::Success(ValueRef::new(json!({"bc": "result"}))),
            ),
            // Step D receives object with B and C results
            (
                json!({"b": {"bc": "result"}, "c": {"bc": "result"}}),
                FlowResult::Success(ValueRef::new(json!({"d": "final"}))),
            ),
        ];

        let executor = create_executor_with_behaviors(behaviors).await;
        let state_store = executor.metadata_store().clone();
        let run_state = create_run_state(flow.clone(), flow_id, ValueRef::new(json!({"x": 1})));
        let run_id = run_state.run_id();

        let mut items_executor = FlowExecutorBuilder::new(executor, run_state)
            .max_concurrency(2) // Allow parallel execution of B and C
            .scheduler(Box::new(BreadthFirstScheduler::new()))
            .build()
            .await
            .unwrap();
        items_executor.execute_to_completion().await.unwrap();

        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(matches!(results[0].result, Some(FlowResult::Success(_))));
    }

    #[tokio::test]
    async fn test_validation_failure() {
        // Test that validation failures are caught by FlowExecutorBuilder
        use stepflow_core::workflow::FlowBuilder;

        // Create an invalid flow with circular dependency
        let flow = Arc::new(
            FlowBuilder::test_flow()
                .steps(vec![
                    StepBuilder::new("step1")
                        .component("/mock/test")
                        .input(ValueExpr::Step {
                            step: "step2".to_string(), // depends on step2
                            path: Default::default(),
                        })
                        .build(),
                    StepBuilder::new("step2")
                        .component("/mock/test")
                        .input(ValueExpr::Step {
                            step: "step1".to_string(), // depends on step1 - circular!
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
        let flow_id = BlobId::from_flow(&flow).unwrap();

        let executor = MockExecutorBuilder::new().build().await;
        let run_state = create_run_state(flow.clone(), flow_id, ValueRef::new(json!({})));

        // Build should fail due to validation
        let result = FlowExecutorBuilder::new(executor, run_state).build().await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_skip_validation() {
        // Test that skip_validation() bypasses validation
        use stepflow_core::workflow::FlowBuilder;

        // Create an invalid flow
        let flow = Arc::new(
            FlowBuilder::test_flow()
                .steps(vec![
                    StepBuilder::new("step1")
                        .component("/mock/test")
                        .input(ValueExpr::Step {
                            step: "step2".to_string(),
                            path: Default::default(),
                        })
                        .build(),
                    StepBuilder::new("step2")
                        .component("/mock/test")
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
        let flow_id = BlobId::from_flow(&flow).unwrap();

        let executor = MockExecutorBuilder::new().build().await;
        let run_state = create_run_state(flow.clone(), flow_id, ValueRef::new(json!({})));

        // Build should succeed with skip_validation
        let result = FlowExecutorBuilder::new(executor, run_state)
            .skip_validation()
            .build()
            .await;

        // Build succeeds (validation skipped)
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_variables_in_step_input() {
        // Test that workflow variables are properly resolved in step inputs
        use crate::testing::MockExecutorBuilder;
        use stepflow_core::workflow::{FlowBuilder, JsonPath};

        // Create a flow where step input uses a variable
        // Step input: { "api_key": { "$variable": "api_key" }, "data": { "$input": "" } }
        let flow = Arc::new(
            FlowBuilder::test_flow()
                .steps(vec![
                    StepBuilder::new("step1")
                        .component("/mock/test")
                        .input(ValueExpr::Object(vec![
                            (
                                "api_key".to_string(),
                                ValueExpr::Variable {
                                    variable: JsonPath::from("api_key"),
                                    default: None,
                                },
                            ),
                            (
                                "data".to_string(),
                                ValueExpr::Input {
                                    input: Default::default(),
                                },
                            ),
                        ]))
                        .build(),
                ])
                .output(ValueExpr::Step {
                    step: "step1".to_string(),
                    path: Default::default(),
                })
                .build(),
        );
        let flow_id = BlobId::from_flow(&flow).unwrap();

        // Mock executor that expects the resolved input with the variable value
        let expected_input = json!({"api_key": "secret123", "data": {"x": 1}});
        let executor = MockExecutorBuilder::new()
            .with_input(expected_input.clone())
            .with_success_result(json!({"processed": true}))
            .build()
            .await;
        let state_store = executor.metadata_store().clone();

        // Create variables map
        let mut variables = HashMap::new();
        variables.insert("api_key".to_string(), ValueRef::new(json!("secret123")));

        let run_state = create_run_state_with_vars(
            flow.clone(),
            flow_id,
            ValueRef::new(json!({"x": 1})),
            variables,
        );
        let run_id = run_state.run_id();

        let mut items_executor = FlowExecutorBuilder::new(executor, run_state)
            .build()
            .await
            .unwrap();
        items_executor.execute_to_completion().await.unwrap();

        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        // Step should have executed successfully with resolved variable
        assert_eq!(results.len(), 1);
        assert!(matches!(results[0].result, Some(FlowResult::Success(_))));
    }

    #[tokio::test]
    async fn test_variable_with_default() {
        // Test that variable defaults are used when variable is not provided
        use crate::testing::MockExecutorBuilder;
        use stepflow_core::workflow::{FlowBuilder, JsonPath};

        // Create a flow where step input uses a variable with a default
        // { "$variable": "missing_var", "$default": "default_value" }
        let flow = Arc::new(
            FlowBuilder::test_flow()
                .steps(vec![
                    StepBuilder::new("step1")
                        .component("/mock/test")
                        .input(ValueExpr::Variable {
                            variable: JsonPath::from("missing_var"),
                            default: Some(Box::new(ValueExpr::Literal(json!("default_value")))),
                        })
                        .build(),
                ])
                .output(ValueExpr::Step {
                    step: "step1".to_string(),
                    path: Default::default(),
                })
                .build(),
        );
        let flow_id = BlobId::from_flow(&flow).unwrap();

        // Mock executor expects the default value
        let executor = MockExecutorBuilder::new()
            .with_input(json!("default_value"))
            .with_success_result(json!({"result": "ok"}))
            .build()
            .await;
        let state_store = executor.metadata_store().clone();

        // No variables provided - should use default
        let run_state = create_run_state(flow.clone(), flow_id, ValueRef::new(json!({})));
        let run_id = run_state.run_id();

        let mut items_executor = FlowExecutorBuilder::new(executor, run_state)
            .build()
            .await
            .unwrap();

        items_executor.execute_to_completion().await.unwrap();

        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(matches!(results[0].result, Some(FlowResult::Success(_))));
    }

    #[tokio::test]
    async fn test_variable_missing_fails() {
        // Test that a missing variable without default causes failure
        use crate::testing::MockExecutorBuilder;
        use stepflow_core::workflow::{FlowBuilder, JsonPath};

        // Create a flow where step input uses a variable without default
        let flow = Arc::new(
            FlowBuilder::test_flow()
                .steps(vec![
                    StepBuilder::new("step1")
                        .component("/mock/test")
                        .input(ValueExpr::Variable {
                            variable: JsonPath::from("missing_var"),
                            default: None, // No default!
                        })
                        .build(),
                ])
                .output(ValueExpr::Step {
                    step: "step1".to_string(),
                    path: Default::default(),
                })
                .build(),
        );
        let flow_id = BlobId::from_flow(&flow).unwrap();

        let executor = MockExecutorBuilder::new().build().await;
        let state_store = executor.metadata_store().clone();

        // No variables provided - should fail on missing variable
        let run_state = create_run_state(flow.clone(), flow_id, ValueRef::new(json!({})));
        let run_id = run_state.run_id();

        let mut items_executor = FlowExecutorBuilder::new(executor, run_state)
            .build()
            .await
            .unwrap();

        items_executor.execute_to_completion().await.unwrap();

        let results = state_store
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        // Step should have failed due to missing variable
        assert_eq!(results.len(), 1);
        assert!(matches!(results[0].result, Some(FlowResult::Failed(_))));
    }

    #[tokio::test]
    async fn test_handle_submit_request() {
        // Test handle_submit_request directly by calling it and verifying
        // that the subflow's RunState is created and tasks are scheduled.
        use stepflow_plugin::SubflowRequest;

        // Create a no-op subflow
        let subflow = Arc::new(
            stepflow_core::workflow::FlowBuilder::test_flow()
                .output(ValueExpr::Input {
                    input: Default::default(),
                })
                .build(),
        );
        let subflow_id = BlobId::from_flow(&subflow).unwrap();

        // Create a simple main flow
        let flow = Arc::new(create_linear_flow(1));
        let flow_id = BlobId::from_flow(&flow).unwrap();

        let executor = MockExecutorBuilder::new().build().await;
        let run_state = create_run_state(
            flow.clone(),
            flow_id.clone(),
            ValueRef::new(json!({"x": 1})),
        );

        let mut items_executor = FlowExecutorBuilder::new(executor, run_state)
            .build()
            .await
            .unwrap();

        // Create a subflow request manually
        let subflow_run_id = Uuid::now_v7();
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let request = SubflowRequest {
            flow: subflow.clone(),
            flow_id: subflow_id.clone(),
            inputs: vec![ValueRef::new(json!({"subflow": "input"}))],
            variables: std::collections::HashMap::new(),
            overrides: None,
            max_concurrency: None,
            parent_run_id: items_executor.root_run_id(),
            item_index: 0,
            step_index: 0,
            subflow_key: Uuid::now_v7(),
            run_id: subflow_run_id,
            response_tx,
        };

        // Call handle_submit_request directly
        items_executor.handle_submit_request(request).await.unwrap();

        // Receive the response
        let response_run_id = response_rx.await.expect("should receive response");
        assert_eq!(response_run_id, subflow_run_id);

        // The subflow has 0 steps, so it completes immediately and is evicted
        // from in-memory state. Results are persisted to the metadata store.
        assert!(
            !items_executor.runs.contains_key(&subflow_run_id),
            "Completed subflow should be evicted from runs"
        );
        assert_ne!(subflow_run_id, items_executor.root_run_id());
    }

    #[tokio::test]
    async fn test_subflow_with_steps_executes() {
        // Test that a subflow with actual steps gets executed by the executor.
        //
        // We submit a subflow (with steps) BEFORE starting the main loop,
        // then run the executor and verify both complete.
        use stepflow_plugin::SubflowRequest;

        // Create a flow with one step for both main and subflow
        let flow = Arc::new(create_linear_flow(1));
        let flow_id = BlobId::from_flow(&flow).unwrap();

        let executor = MockExecutorBuilder::new().build().await;
        let state_store = executor.metadata_store().clone();
        let run_state = create_run_state(
            flow.clone(),
            flow_id.clone(),
            ValueRef::new(json!({"x": 1})),
        );
        let main_run_id = run_state.run_id();

        let mut items_executor = FlowExecutorBuilder::new(executor, run_state)
            .build()
            .await
            .unwrap();

        // Submit a subflow BEFORE starting execution
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let request = SubflowRequest {
            flow: flow.clone(),
            flow_id: flow_id.clone(),
            inputs: vec![ValueRef::new(json!({"x": 2}))], // Different input
            variables: std::collections::HashMap::new(),
            overrides: None,
            max_concurrency: None,
            parent_run_id: main_run_id,
            item_index: 0,
            step_index: 0,
            subflow_key: Uuid::now_v7(),
            run_id: Uuid::now_v7(),
            response_tx,
        };
        items_executor.handle_submit_request(request).await.unwrap();

        let subflow_run_id = response_rx.await.expect("should receive response");

        // Now run the executor - it should process both main flow and subflow
        items_executor.execute_to_completion().await.unwrap();

        // Wait for subflow completion via state store notification
        let wait_result = tokio::time::timeout(
            tokio::time::Duration::from_secs(1),
            state_store.wait_for_completion(subflow_run_id),
        )
        .await;
        assert!(
            wait_result.is_ok(),
            "Subflow completion signal should arrive"
        );

        // Verify main flow results
        let main_results = state_store
            .get_item_results(main_run_id, ResultOrder::ByIndex)
            .await
            .unwrap();
        assert_eq!(main_results.len(), 1);
        assert!(
            matches!(&main_results[0].result, Some(FlowResult::Success(_))),
            "Main flow should succeed, got: {:?}",
            main_results[0].result
        );

        // Verify subflow results - wait_for_completion guarantees results are recorded
        let subflow_results = state_store
            .get_item_results(subflow_run_id, ResultOrder::ByIndex)
            .await
            .unwrap();
        assert_eq!(subflow_results.len(), 1);
        assert!(
            matches!(&subflow_results[0].result, Some(FlowResult::Success(_))),
            "Subflow should succeed, got: {:?}",
            subflow_results[0].result
        );
    }

    #[tokio::test]
    async fn test_subflow_submitted_during_execution() {
        // Test that a subflow can be submitted via channel DURING execution
        // while the main flow has a task in flight.
        //
        // This tests the real use case: a component submits a subflow while executing.
        use crate::testing::create_env_with_wait_signal;

        // Create a flow with one step
        let flow = Arc::new(create_linear_flow(1));
        let flow_id = BlobId::from_flow(&flow).unwrap();

        // Create environment with wait signal for the main flow's input
        // The main flow step will block until we signal it
        let main_input = json!({"wait": true});
        let (env, signal) = create_env_with_wait_signal(main_input.clone()).await;
        let state_store = env.metadata_store().clone();
        let run_state = create_run_state(flow.clone(), flow_id.clone(), ValueRef::new(main_input));

        let mut items_executor = FlowExecutorBuilder::new(env, run_state)
            .build()
            .await
            .unwrap();

        // Get the submit sender to send subflow requests via channel
        let submit_sender = items_executor.submit_sender().clone();

        // Spawn the executor to run in the background
        let exec_handle = tokio::spawn(async move { items_executor.execute_to_completion().await });

        // Give the executor time to start the main flow step (which will block on signal)
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Submit a subflow via the channel while main flow is in flight
        let subflow_run_id = submit_sender
            .submit(
                flow.clone(),
                flow_id.clone(),
                vec![ValueRef::new(json!({"x": 2}))],
                std::collections::HashMap::new(),
                None,
                None,
                Uuid::now_v7(),
            )
            .await
            .expect("subflow submit should succeed");

        // Wait for the subflow to complete via state store (it should run while main flow is blocked)
        let _ = state_store.wait_for_completion(subflow_run_id).await;

        // Now signal the main flow to complete
        drop(signal);

        // Wait for the executor to finish
        exec_handle.await.unwrap().unwrap();

        // Verify subflow results
        let subflow_results = state_store
            .get_item_results(subflow_run_id, ResultOrder::ByIndex)
            .await
            .unwrap();
        assert_eq!(subflow_results.len(), 1);
        assert!(
            matches!(&subflow_results[0].result, Some(FlowResult::Success(_))),
            "Subflow should succeed, got: {:?}",
            subflow_results[0].result
        );
    }

    #[tokio::test]
    async fn test_empty_subflow_completes_immediately() {
        // Test that a subflow with 0 items completes immediately without deadlock.
        //
        // This was a bug: empty subflows had no tasks to schedule, so `complete_task`
        // was never called, and the completion channel was never signaled.
        use stepflow_plugin::SubflowRequest;

        let flow = Arc::new(create_linear_flow(1));
        let flow_id = BlobId::from_flow(&flow).unwrap();

        let executor = MockExecutorBuilder::new().build().await;
        let state_store = executor.metadata_store().clone();
        let run_state = create_run_state(
            flow.clone(),
            flow_id.clone(),
            ValueRef::new(json!({"x": 1})),
        );

        let mut items_executor = FlowExecutorBuilder::new(executor, run_state)
            .build()
            .await
            .unwrap();

        // Submit an EMPTY subflow (0 items)
        let subflow_run_id = Uuid::now_v7();
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let request = SubflowRequest {
            flow: flow.clone(),
            flow_id: flow_id.clone(),
            inputs: vec![], // Empty!
            variables: std::collections::HashMap::new(),
            overrides: None,
            max_concurrency: None,
            parent_run_id: items_executor.root_run_id(),
            item_index: 0,
            step_index: 0,
            subflow_key: Uuid::now_v7(),
            run_id: subflow_run_id,
            response_tx,
        };
        items_executor.handle_submit_request(request).await.unwrap();

        let response_run_id = response_rx.await.expect("should receive response");
        assert_eq!(response_run_id, subflow_run_id);

        // Wait for the empty subflow to complete (status update is async)
        let wait_result = tokio::time::timeout(
            tokio::time::Duration::from_secs(1),
            state_store.wait_for_completion(subflow_run_id),
        )
        .await;
        assert!(
            wait_result.is_ok(),
            "Empty subflow should complete without timeout"
        );

        // Verify the status is Completed
        let run_details = state_store
            .get_run(subflow_run_id)
            .await
            .unwrap()
            .expect("subflow run should exist");
        assert_eq!(
            run_details.summary.status,
            ExecutionStatus::Completed,
            "Empty subflow should be completed, got: {:?}",
            run_details.summary.status
        );

        // The empty subflow completed immediately and was evicted from in-memory state.
        // Results are persisted to the metadata store.
        assert!(
            items_executor.run_state(subflow_run_id).is_none(),
            "Completed empty subflow should be evicted from runs"
        );
    }

    #[tokio::test]
    async fn test_subflow_runs_despite_max_in_flight_exhausted() {
        // Test that subflow tasks can run even when max_in_flight is 1 and
        // the parent task is "in flight" (waiting for the subflow).
        //
        // This was a deadlock bug: the parent occupied the only slot, so
        // subflow tasks couldn't start, so the parent never completed.
        use stepflow_plugin::SubflowRequest;

        let flow = Arc::new(create_linear_flow(1));
        let flow_id = BlobId::from_flow(&flow).unwrap();

        let executor = MockExecutorBuilder::new().build().await;
        let state_store = executor.metadata_store().clone();
        let run_state = create_run_state(
            flow.clone(),
            flow_id.clone(),
            ValueRef::new(json!({"x": 1})),
        );

        let mut items_executor = FlowExecutorBuilder::new(executor, run_state)
            .max_concurrency(1) // Only 1 slot - this was the trigger for the deadlock
            .build()
            .await
            .unwrap();

        // Submit a subflow BEFORE starting - this will be processed in the first
        // loop iteration when we also start the main flow task.
        let subflow_run_id = Uuid::now_v7();
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let request = SubflowRequest {
            flow: flow.clone(),
            flow_id: flow_id.clone(),
            inputs: vec![ValueRef::new(json!({"y": 2}))],
            variables: std::collections::HashMap::new(),
            overrides: None,
            max_concurrency: None,
            parent_run_id: items_executor.root_run_id(),
            item_index: 0,
            step_index: 0,
            subflow_key: Uuid::now_v7(),
            run_id: subflow_run_id,
            response_tx,
        };
        items_executor.handle_submit_request(request).await.unwrap();

        let response_run_id = response_rx.await.expect("should receive response");
        assert_eq!(response_run_id, subflow_run_id);

        // Now run execution - with max_in_flight=1, the original bug would deadlock
        // because the main task would occupy the slot and subflow tasks couldn't run.
        // The fix ensures at least 1 task can always run (max_to_start >= 1).
        let result = tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            items_executor.execute_to_completion(),
        )
        .await;

        assert!(
            result.is_ok(),
            "Execution should complete without timeout (no deadlock)"
        );
        result.unwrap().unwrap();

        // Verify subflow completed via state store
        let wait_result = tokio::time::timeout(
            tokio::time::Duration::from_secs(1),
            state_store.wait_for_completion(subflow_run_id),
        )
        .await;
        assert!(
            wait_result.is_ok(),
            "Subflow completion signal should arrive (not timeout)"
        );

        let run_details = state_store
            .get_run(subflow_run_id)
            .await
            .unwrap()
            .expect("subflow run should exist");
        assert!(
            matches!(
                run_details.summary.status,
                ExecutionStatus::Completed | ExecutionStatus::Failed
            ),
            "Subflow should be complete, got: {:?}",
            run_details.summary.status
        );
    }

    #[tokio::test]
    async fn test_subflow_results_available_after_completion_signal() {
        // Test that subflow results are recorded BEFORE the completion signal is sent.
        //
        // This was a race condition: completion was signaled from a spawned task,
        // but the results might not have been recorded yet when the parent read them.
        use stepflow_plugin::SubflowRequest;

        let flow = Arc::new(create_linear_flow(1));
        let flow_id = BlobId::from_flow(&flow).unwrap();

        let executor = MockExecutorBuilder::new().build().await;
        let state_store = executor.metadata_store().clone();
        let run_state = create_run_state(
            flow.clone(),
            flow_id.clone(),
            ValueRef::new(json!({"x": 1})),
        );

        let mut items_executor = FlowExecutorBuilder::new(executor, run_state)
            .build()
            .await
            .unwrap();

        // Submit a subflow with 3 items
        let subflow_run_id = Uuid::now_v7();
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let request = SubflowRequest {
            flow: flow.clone(),
            flow_id: flow_id.clone(),
            inputs: vec![
                ValueRef::new(json!({"i": 1})),
                ValueRef::new(json!({"i": 2})),
                ValueRef::new(json!({"i": 3})),
            ],
            variables: std::collections::HashMap::new(),
            overrides: None,
            max_concurrency: None,
            parent_run_id: items_executor.root_run_id(),
            item_index: 0,
            step_index: 0,
            subflow_key: Uuid::now_v7(),
            run_id: subflow_run_id,
            response_tx,
        };
        items_executor.handle_submit_request(request).await.unwrap();

        let response_run_id = response_rx.await.expect("should receive response");
        assert_eq!(response_run_id, subflow_run_id);

        // Run execution
        items_executor.execute_to_completion().await.unwrap();

        // Wait for subflow completion via state store
        state_store
            .wait_for_completion(subflow_run_id)
            .await
            .unwrap();

        // IMMEDIATELY after completion signal, results should be available
        // (no sleep needed - the fix ensures results are recorded before signaling)
        let results = state_store
            .get_item_results(subflow_run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        assert_eq!(results.len(), 3, "Should have all 3 results");

        // All results should be present (not None/Running)
        for (i, result) in results.iter().enumerate() {
            assert!(
                result.result.is_some(),
                "Result at index {} should be present, got status: {:?}",
                i,
                result.status
            );
        }
    }

    #[tokio::test]
    async fn test_subflow_run_record_created_in_state_store() {
        // Test that subflow run records are created in the state store,
        // not just in the executor's internal runs map.
        //
        // This was a bug: handle_submit_request only created RunState internally,
        // so get_item_results couldn't find the run in the state store.
        use stepflow_plugin::SubflowRequest;

        let flow = Arc::new(create_linear_flow(1));
        let flow_id = BlobId::from_flow(&flow).unwrap();

        let executor = MockExecutorBuilder::new().build().await;
        let state_store = executor.metadata_store().clone();

        let run_state = create_run_state(
            flow.clone(),
            flow_id.clone(),
            ValueRef::new(json!({"x": 1})),
        );
        let mut items_executor = FlowExecutorBuilder::new(executor, run_state)
            .build()
            .await
            .unwrap();

        // Submit a subflow
        let subflow_run_id = Uuid::now_v7();
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let request = SubflowRequest {
            flow: flow.clone(),
            flow_id: flow_id.clone(),
            inputs: vec![
                ValueRef::new(json!({"y": 1})),
                ValueRef::new(json!({"y": 2})),
            ],
            variables: std::collections::HashMap::new(),
            overrides: None,
            max_concurrency: None,
            parent_run_id: items_executor.root_run_id(),
            item_index: 0,
            step_index: 0,
            subflow_key: Uuid::now_v7(),
            run_id: subflow_run_id,
            response_tx,
        };
        items_executor.handle_submit_request(request).await.unwrap();

        let response_run_id = response_rx.await.expect("should receive response");
        assert_eq!(response_run_id, subflow_run_id);

        // The run should be queryable from the state store
        // (queue_write with CreateRun should have been called synchronously)
        let results = state_store
            .get_item_results(subflow_run_id, ResultOrder::ByIndex)
            .await
            .unwrap();

        // Should have 2 items (even if they're not completed yet)
        assert_eq!(
            results.len(),
            2,
            "State store should know about the 2-item subflow run"
        );
    }

    #[tokio::test]
    async fn test_subflow_hierarchy_correctly_persisted() {
        // Test that subflow run records have correct parent_run_id and root_run_id
        // in the state store.
        //
        // This was a bug: handle_submit_request used CreateRunParams::new() which
        // sets root_run_id = run_id and parent_run_id = None, losing the hierarchy.
        use stepflow_plugin::SubflowRequest;

        let flow = Arc::new(create_linear_flow(1));
        let flow_id = BlobId::from_flow(&flow).unwrap();

        let executor = MockExecutorBuilder::new().build().await;
        let state_store = executor.metadata_store().clone();

        let run_state = create_run_state(
            flow.clone(),
            flow_id.clone(),
            ValueRef::new(json!({"x": 1})),
        );
        let mut items_executor = FlowExecutorBuilder::new(executor, run_state)
            .build()
            .await
            .unwrap();

        let root_run_id = items_executor.root_run_id();

        // Submit a subflow - generate run_id upfront so caller can query results
        // even if response channel fails
        let subflow_run_id = Uuid::now_v7();
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let request = SubflowRequest {
            flow: flow.clone(),
            flow_id: flow_id.clone(),
            inputs: vec![ValueRef::new(json!({"y": 1}))],
            variables: std::collections::HashMap::new(),
            overrides: None,
            max_concurrency: None,
            parent_run_id: root_run_id,
            item_index: 0,
            step_index: 0,
            subflow_key: Uuid::now_v7(),
            run_id: subflow_run_id,
            response_tx,
        };
        items_executor.handle_submit_request(request).await.unwrap();

        let response_run_id = response_rx.await.expect("should receive response");
        assert_eq!(response_run_id, subflow_run_id);

        // Verify the subflow is different from the root
        assert_ne!(subflow_run_id, root_run_id);

        // Get the run details from state store and verify hierarchy
        let run_details = state_store
            .get_run(subflow_run_id)
            .await
            .unwrap()
            .expect("subflow run should exist in state store");

        // Verify the hierarchy is correct
        assert_eq!(
            run_details.summary.root_run_id, root_run_id,
            "Subflow should have root_run_id pointing to the top-level run"
        );
        assert_eq!(
            run_details.summary.parent_run_id,
            Some(root_run_id),
            "Subflow should have parent_run_id pointing to the run that submitted it"
        );
    }

    #[tokio::test]
    async fn test_completed_subflow_evicted_from_runs() {
        // Test that when a subflow with steps completes, it is evicted from the
        // executor's in-memory runs map. Results remain in the metadata store.
        use stepflow_plugin::SubflowRequest;

        let flow = Arc::new(create_linear_flow(1));
        let flow_id = BlobId::from_flow(&flow).unwrap();

        let executor = MockExecutorBuilder::new().build().await;
        let state_store = executor.metadata_store().clone();
        let run_state = create_run_state(
            flow.clone(),
            flow_id.clone(),
            ValueRef::new(json!({"x": 1})),
        );
        let main_run_id = run_state.run_id();

        let mut items_executor = FlowExecutorBuilder::new(executor, run_state)
            .build()
            .await
            .unwrap();

        // Submit a subflow with steps BEFORE starting execution
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let request = SubflowRequest {
            flow: flow.clone(),
            flow_id: flow_id.clone(),
            inputs: vec![ValueRef::new(json!({"x": 2}))],
            variables: std::collections::HashMap::new(),
            overrides: None,
            max_concurrency: None,
            parent_run_id: main_run_id,
            item_index: 0,
            step_index: 0,
            subflow_key: Uuid::now_v7(),
            run_id: Uuid::now_v7(),
            response_tx,
        };
        items_executor.handle_submit_request(request).await.unwrap();
        let subflow_run_id = response_rx.await.expect("should receive response");

        // Before execution: both root and subflow are in runs
        assert_eq!(items_executor.runs.len(), 2);

        // Execute to completion
        items_executor.execute_to_completion().await.unwrap();

        // After execution: only root remains in runs, subflow was evicted
        assert_eq!(
            items_executor.runs.len(),
            1,
            "Only root run should remain in runs after subflow completes"
        );
        assert!(
            items_executor.runs.contains_key(&main_run_id),
            "Root run should still be in runs"
        );
        assert!(
            !items_executor.runs.contains_key(&subflow_run_id),
            "Completed subflow should be evicted from runs"
        );

        // Results are still available from the metadata store
        let subflow_results = state_store
            .get_item_results(subflow_run_id, ResultOrder::ByIndex)
            .await
            .unwrap();
        assert_eq!(subflow_results.len(), 1);
        assert!(
            matches!(&subflow_results[0].result, Some(FlowResult::Success(_))),
            "Subflow result should be in metadata store, got: {:?}",
            subflow_results[0].result
        );
    }
}
