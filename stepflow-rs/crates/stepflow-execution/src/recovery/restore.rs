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

//! Execution state restoration from journal replay and/or checkpoints.
//!
//! Both recovery paths (full journal replay and checkpoint-accelerated) produce
//! the same output: a [`RecoveredState`] containing the root run's state, a
//! subflow deduplication map, and any in-flight subflow states.
//!
//! The [`Recovery`] struct holds context shared across both paths (journal,
//! flow, blob store, metadata store, root_run_id). The two entry points are
//! [`Recovery::restore_from_journal`] and [`Recovery::restore_from_checkpoint`], which share
//! the common tail-replay logic in [`Recovery::replay_events`].

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use error_stack::ResultExt as _;
use futures::StreamExt as _;
use stepflow_core::status::ExecutionStatus;
use stepflow_state::{CreateRunParams, ExecutionJournal, SequenceNumber};

use crate::checkpoint::CheckpointData;
use crate::{ExecutionError, Result, RunState};

/// State recovered from either a checkpoint or full journal replay.
///
/// Both recovery paths (checkpoint-accelerated and full journal replay)
/// produce the same output: the root run's state, a subflow deduplication
/// map, and any recovered subflow states.
pub(super) struct RecoveredState {
    /// The root run's execution state.
    pub run_state: RunState,
    /// Subflow deduplication map.
    ///
    /// Key: `(parent_run_id, item_index, step_index, subflow_key)`.
    /// Value: `subflow_run_id`.
    pub subflow_map: HashMap<(uuid::Uuid, u32, usize, uuid::Uuid), uuid::Uuid>,
    /// Additional (subflow) RunStates keyed by run_id.
    pub subflow_runs: HashMap<uuid::Uuid, RunState>,
    /// Subflow run IDs that were in-flight at crash time (initialized but not completed).
    ///
    /// These already have a `RunInitialized` journal event from the original execution,
    /// so recovery must skip writing a duplicate.
    pub inflight_subflow_run_ids: HashSet<uuid::Uuid>,
    /// Terminal status from the root run's `RunCompleted` event, if present.
    ///
    /// Set when the journal contains a `RunCompleted` for the root run,
    /// indicating the run finished but the metadata store may not have been
    /// updated before the crash (journal-first ordering crash window).
    pub root_terminal_status: Option<ExecutionStatus>,
}

/// Shared context for execution state recovery.
///
/// Holds references to the journal, flow definition, blob store, and metadata
/// store — the "static" parts shared by both recovery paths. The two entry
/// points [`from_journal`](Self::from_journal) and
/// [`from_checkpoint`](Self::from_checkpoint) use this context to avoid
/// threading the same parameters through every internal function.
pub(super) struct Recovery<'a> {
    pub root_run_id: uuid::Uuid,
    journal: &'a dyn ExecutionJournal,
    flow: &'a Arc<stepflow_core::workflow::Flow>,
    blob_store: &'a dyn stepflow_state::BlobStore,
    metadata_store: &'a dyn stepflow_state::MetadataStore,
}

impl<'a> Recovery<'a> {
    /// Create a new Recovery context.
    pub fn new(
        root_run_id: uuid::Uuid,
        journal: &'a dyn ExecutionJournal,
        flow: &'a Arc<stepflow_core::workflow::Flow>,
        blob_store: &'a dyn stepflow_state::BlobStore,
        metadata_store: &'a dyn stepflow_state::MetadataStore,
    ) -> Self {
        Self {
            root_run_id,
            journal,
            flow,
            blob_store,
            metadata_store,
        }
    }

    // -----------------------------------------------------------------------
    // Full journal replay (no checkpoint)
    // -----------------------------------------------------------------------

    /// Full journal replay path — no checkpoint available.
    ///
    /// Streams the journal from the root run's start sequence, extracts
    /// `RootRunCreated` as the first event, then replays remaining events
    /// through [`replay_events`](Self::replay_events) to reconstruct
    /// execution state for both the root and any subflows.
    ///
    /// For completed subflows, checks the metadata store to determine whether
    /// results have been persisted. If the journal records a `RunCompleted` but
    /// the metadata store still shows `Running` (crash between journal write and
    /// metadata update), the function reconstructs the subflow's state and syncs
    /// the metadata store.
    pub async fn restore_from_journal(
        &self,
        root_info: &stepflow_state::RunRecoveryInfo,
    ) -> Result<RecoveredState> {
        let start = root_info.start_sequence;
        let mut stream = self.journal.stream_from(self.root_run_id, start);

        // The first event must be RootRunCreated for the root run.
        let first_event = stream
            .next()
            .await
            .ok_or_else(|| {
                log::warn!(
                    "No journal entries for execution tree {}, cannot recover",
                    self.root_run_id
                );
                error_stack::report!(ExecutionError::RecoveryFailed)
                    .attach_printable("No journal entries found for this run")
            })?
            .change_context(ExecutionError::RecoveryFailed)?;

        let (inputs, variables) = match first_event {
            stepflow_state::JournalEvent::RootRunCreated {
                run_id: event_run_id,
                inputs,
                variables,
                ..
            } if event_run_id == self.root_run_id => (inputs, variables),
            _ => {
                return Err(error_stack::report!(ExecutionError::RecoveryFailed)
                    .attach_printable("First journal event is not RootRunCreated for root run"));
            }
        };

        // Create RunState for the root run
        let mut run_state = RunState::new(
            self.root_run_id,
            root_info.flow_id.clone(),
            self.flow.clone(),
            inputs,
            variables,
        );

        let mut subflow_runs: HashMap<uuid::Uuid, RunState> = HashMap::new();
        let mut subflow_map: HashMap<(uuid::Uuid, u32, usize, uuid::Uuid), uuid::Uuid> =
            HashMap::new();
        let mut inflight_subflow_run_ids: HashSet<uuid::Uuid> = HashSet::new();
        let mut root_terminal_status: Option<ExecutionStatus> = None;

        // Replay from the root run's start sequence.
        let event_count = self
            .replay_events(
                stream,
                &mut run_state,
                &mut subflow_runs,
                &mut subflow_map,
                &mut inflight_subflow_run_ids,
                &mut root_terminal_status,
                || ExecutionError::RecoveryFailed,
                None,
            )
            .await?;

        log::info!(
            "Replayed {} journal events for execution tree {}, root complete={}",
            event_count + 1, // +1 for the RootRunCreated event consumed above
            self.root_run_id,
            run_state.is_complete()
        );

        Ok(RecoveredState {
            run_state,
            subflow_map,
            subflow_runs,
            inflight_subflow_run_ids,
            root_terminal_status,
        })
    }

    // -----------------------------------------------------------------------
    // Checkpoint-accelerated recovery
    // -----------------------------------------------------------------------

    /// Restore execution state from a checkpoint.
    ///
    /// Deserializes the checkpoint, validates structural integrity, rebuilds all
    /// RunStates and the subflow dedup map, then replays tail journal events
    /// through [`replay_events`](Self::replay_events).
    ///
    /// When a `RunCompleted` tail event is encountered for a subflow, the metadata
    /// store is checked. If it is out of sync (crash between journal write and
    /// metadata update), the subflow's item results and status are synced before
    /// evicting the RunState.
    ///
    /// Returns an error if the checkpoint is present but cannot be restored
    /// (deserialization failure, structural mismatch, missing blobs). The caller
    /// should treat this as a hard failure — the full journal may not be available
    /// after a checkpoint has been written.
    pub async fn restore_from_checkpoint(
        &self,
        stored_cp: &stepflow_state::StoredCheckpoint,
    ) -> Result<RecoveredState> {
        let checkpoint_data = CheckpointData::deserialize(&stored_cp.data).map_err(|e| {
            error_stack::report!(ExecutionError::CheckpointError)
                .attach_printable(format!("checkpoint deserialization failed: {e}"))
        })?;

        log::info!(
            "Restoring from checkpoint at sequence {:?} for tree {}",
            stored_cp.sequence,
            self.root_run_id
        );

        // Restore the root run from checkpoint
        let root_checkpoint = checkpoint_data
            .runs
            .iter()
            .find(|rc| rc.run_id == self.root_run_id)
            .ok_or_else(|| {
                error_stack::report!(ExecutionError::CheckpointError)
                    .attach_printable("Root run not found in checkpoint")
            })?;

        let mut run_state =
            RunState::from_checkpoint(root_checkpoint, self.flow.clone()).map_err(|e| {
                error_stack::report!(ExecutionError::CheckpointError)
                    .attach_printable(format!("root run restore failed: {e}"))
            })?;

        // Restore subflow RunStates from checkpoint
        let mut subflow_runs: HashMap<uuid::Uuid, RunState> = HashMap::new();
        for rc in &checkpoint_data.runs {
            if rc.run_id == self.root_run_id {
                continue; // Skip root, already restored
            }
            let sub_flow = self
                .blob_store
                .get_flow(&rc.flow_id)
                .await
                .change_context(ExecutionError::CheckpointError)?
                .ok_or_else(|| {
                    error_stack::report!(ExecutionError::CheckpointError).attach_printable(format!(
                        "Subflow flow not found for run {}, flow_id={}",
                        rc.run_id, rc.flow_id
                    ))
                })?;
            let sub_state = RunState::from_checkpoint(rc, sub_flow).map_err(|e| {
                error_stack::report!(ExecutionError::CheckpointError)
                    .attach_printable(format!("subflow {} restore failed: {e}", rc.run_id))
            })?;
            subflow_runs.insert(rc.run_id, sub_state);
        }

        // Restore subflow dedup map from checkpoint
        let mut subflow_map: HashMap<(uuid::Uuid, u32, usize, uuid::Uuid), uuid::Uuid> =
            HashMap::new();
        for mapping in &checkpoint_data.subflow_map {
            subflow_map.insert(
                (
                    mapping.parent_run_id,
                    mapping.item_index,
                    mapping.step_index,
                    mapping.subflow_key,
                ),
                mapping.subflow_run_id,
            );
        }

        // Stream tail events (after checkpoint) to bring state up to date.
        let tail_stream = self
            .journal
            .stream_from(self.root_run_id, stored_cp.sequence.next());

        // Seed in-flight set: all checkpoint-restored subflows were past initialization.
        let mut inflight_subflow_run_ids: HashSet<uuid::Uuid> = checkpoint_data
            .runs
            .iter()
            .filter(|rc| rc.run_id != self.root_run_id)
            .map(|rc| rc.run_id)
            .collect();
        let mut root_terminal_status: Option<ExecutionStatus> = None;

        // Checkpoint recovery: use the checkpoint sequence as a lower bound to
        // prune sub-runs created before the checkpoint from the metadata query.
        let tail_count = self
            .replay_events(
                tail_stream,
                &mut run_state,
                &mut subflow_runs,
                &mut subflow_map,
                &mut inflight_subflow_run_ids,
                &mut root_terminal_status,
                || ExecutionError::CheckpointError,
                Some(stored_cp.sequence),
            )
            .await?;

        log::info!(
            "Restored from checkpoint + replayed {} tail events for tree {}, root complete={}",
            tail_count,
            self.root_run_id,
            run_state.is_complete()
        );

        Ok(RecoveredState {
            run_state,
            subflow_map,
            subflow_runs,
            inflight_subflow_run_ids,
            root_terminal_status,
        })
    }

    // -----------------------------------------------------------------------
    // Shared replay logic
    // -----------------------------------------------------------------------

    /// Replay a stream of journal events, applying them to all in-memory
    /// RunStates and handling subflow lifecycle events.
    ///
    /// This is the core replay loop shared by both journal-only and checkpoint
    /// recovery paths. For each event it:
    ///
    /// 1. Applies the event to the root RunState and all subflow RunStates.
    /// 2. On `SubRunCreated`: updates the dedup map, creates a new RunState
    ///    (loading the flow definition from the blob store), and ensures a
    ///    metadata record exists (crash window: journal written, metadata not).
    /// 3. On `RunInitialized` (non-root): adds the run to the in-flight set.
    /// 4. On `RunCompleted` (root): captures the terminal status in
    ///    `root_terminal_status` for the caller to sync.
    /// 5. On `RunCompleted` (non-root): syncs the metadata store if needed
    ///    (crash window recovery), then evicts the RunState.
    ///
    /// Before iterating, pre-fetches all sub-runs from the metadata store in a
    /// single batch query. For checkpoint recovery, `replay_start_offset` prunes
    /// the query to sub-runs created at or after the checkpoint sequence,
    /// avoiding a full scan of the tree.
    ///
    /// Returns the number of events replayed.
    #[allow(clippy::too_many_arguments)]
    async fn replay_events(
        &self,
        mut events: stepflow_state::JournalEventStream<'_>,
        run_state: &mut RunState,
        subflow_runs: &mut HashMap<uuid::Uuid, RunState>,
        subflow_map: &mut HashMap<(uuid::Uuid, u32, usize, uuid::Uuid), uuid::Uuid>,
        inflight_subflow_run_ids: &mut HashSet<uuid::Uuid>,
        root_terminal_status: &mut Option<ExecutionStatus>,
        make_error: fn() -> ExecutionError,
        replay_start_offset: Option<SequenceNumber>,
    ) -> Result<usize> {
        // Pre-fetch sub-runs from the metadata store in a single query.
        // Returns two sets:
        // - `known`: all sub-run IDs that have metadata records
        // - `completed`: sub-run IDs with terminal status (sync can be skipped)
        //
        // Uses `replay_start_offset` as a `not_finished_before_seqno` bound to
        // prune sub-runs that finished before the replay window.
        let SubflowMetadata {
            known_in_metadata,
            completed_in_metadata,
        } = self
            .prefetch_subflow_metadata(make_error, replay_start_offset)
            .await?;

        let mut event_count: usize = 0;

        while let Some(event_result) = events.next().await {
            let event = event_result.change_context(make_error())?;
            event_count += 1;

            // Apply event to root and all subflow RunStates
            run_state.apply_event(&event);
            for sub_state in subflow_runs.values_mut() {
                sub_state.apply_event(&event);
            }

            // Handle SubRunCreated: update dedup map and create RunState.
            // The new sub-run is inserted into subflow_runs so subsequent events
            // in this loop are applied to it naturally.
            if let stepflow_state::JournalEvent::SubRunCreated {
                run_id: sub_run_id,
                flow_id: sub_flow_id,
                inputs: sub_inputs,
                variables: sub_variables,
                parent_run_id,
                item_index,
                step_index,
                subflow_key,
            } = &event
            {
                subflow_map.insert(
                    (*parent_run_id, *item_index, *step_index, *subflow_key),
                    *sub_run_id,
                );
                if !subflow_runs.contains_key(sub_run_id) {
                    let sub_flow = self
                        .blob_store
                        .get_flow(sub_flow_id)
                        .await
                        .change_context(make_error())?
                        .ok_or_else(|| {
                            error_stack::report!(make_error()).attach_printable(format!(
                                "Subflow flow not found for run {sub_run_id}, flow_id={sub_flow_id}"
                            ))
                        })?;
                    let sub_state = RunState::new_subflow(
                        *sub_run_id,
                        sub_flow_id.clone(),
                        self.root_run_id,
                        *parent_run_id,
                        sub_flow,
                        sub_inputs.clone(),
                        sub_variables.clone(),
                    );
                    subflow_runs.insert(*sub_run_id, sub_state);
                }

                // Ensure metadata record exists for this sub-run.
                //
                // Crash window: SubRunCreated was journalled but the crash happened
                // before create_run(metadata). Without a metadata record, subsequent
                // sync_run_state_to_metadata and FlowExecutor completion updates
                // would fail to find the run. Create it here so the metadata store
                // is consistent before execution resumes.
                if !known_in_metadata.contains(sub_run_id) {
                    log::info!(
                        "Creating missing metadata record for sub-run {} (crash window recovery)",
                        sub_run_id
                    );
                    let mut params = CreateRunParams::new_subflow(
                        *sub_run_id,
                        sub_flow_id.clone(),
                        sub_inputs.clone(),
                        self.root_run_id,
                        *parent_run_id,
                    );
                    params.created_at_seqno = replay_start_offset;
                    if let Err(e) = self.metadata_store.create_run(params).await {
                        log::warn!(
                            "Failed to create metadata record for sub-run {} during recovery: {:?}",
                            sub_run_id,
                            e
                        );
                    }
                }
            }

            // Track in-flight subflows: add on RunInitialized (non-root).
            if let stepflow_state::JournalEvent::RunInitialized { run_id: rid, .. } = &event
                && *rid != self.root_run_id
            {
                inflight_subflow_run_ids.insert(*rid);
            }

            // Handle RunCompleted: sync metadata for subflows, capture status for root.
            if let stepflow_state::JournalEvent::RunCompleted {
                run_id: completed_id,
                status,
            } = &event
            {
                if *completed_id == self.root_run_id {
                    // Root run completed: capture the terminal status for the caller
                    // to sync. We don't sync here because the root RunState is still
                    // needed by the caller (tree.rs) for the sync operation.
                    *root_terminal_status = Some(*status);
                } else {
                    // Subflow completed: sync metadata if needed, then evict.
                    // If we have a RunState for this subflow and it's not already synced
                    // in the metadata store, sync it (crash window: journal RunCompleted
                    // written but metadata store not updated before crash).
                    if let Some(sub_state) = subflow_runs.get(completed_id)
                        && !completed_in_metadata.contains(completed_id)
                    {
                        log::info!(
                            "Syncing metadata for completed subflow {} (status={:?})",
                            completed_id,
                            status
                        );
                        sync_run_state_to_metadata(
                            *completed_id,
                            sub_state,
                            *status,
                            self.metadata_store,
                            make_error,
                        )
                        .await?;
                    }

                    subflow_runs.remove(completed_id);
                    inflight_subflow_run_ids.remove(completed_id);
                }
            }
        }

        Ok(event_count)
    }

    /// Pre-fetch sub-run metadata from the metadata store.
    ///
    /// Queries the metadata store for all sub-runs in this execution tree, optionally
    /// filtered by `replay_start_offset`. For checkpoint recovery, this uses
    /// `not_finished_before_seqno` to include sub-runs that are still running or
    /// finished at/after the checkpoint — capturing sub-runs created before the
    /// checkpoint that completed in the tail events. For full replay, `None` queries
    /// all sub-runs.
    async fn prefetch_subflow_metadata(
        &self,
        make_error: fn() -> ExecutionError,
        replay_start_offset: Option<SequenceNumber>,
    ) -> Result<SubflowMetadata> {
        let filters = stepflow_dtos::RunFilters {
            root_run_id: Some(self.root_run_id),
            not_finished_before_seqno: replay_start_offset.map(|s| s.value()),
            ..Default::default()
        };

        let runs = self
            .metadata_store
            .list_runs(&filters)
            .await
            .change_context(make_error())?;

        let mut known_in_metadata = HashSet::new();
        let mut completed_in_metadata = HashSet::new();

        for r in runs {
            if r.run_id == self.root_run_id {
                continue; // Skip root
            }
            known_in_metadata.insert(r.run_id);
            if matches!(
                r.status,
                ExecutionStatus::Completed | ExecutionStatus::Failed
            ) {
                completed_in_metadata.insert(r.run_id);
            }
        }

        Ok(SubflowMetadata {
            known_in_metadata,
            completed_in_metadata,
        })
    }
}

/// Pre-fetched sub-run metadata from the metadata store.
struct SubflowMetadata {
    /// All sub-run IDs known to the metadata store (have records).
    known_in_metadata: HashSet<uuid::Uuid>,
    /// Sub-run IDs with terminal status (completed/failed), whose metadata
    /// is already in sync and can skip per-event sync checks.
    completed_in_metadata: HashSet<uuid::Uuid>,
}

/// Write a completed run's item results and status to the metadata store.
///
/// This resolves each item's output from the RunState and records it,
/// then updates the run status to match the journal. Used for both subflow
/// crash-window sync during replay and root run sync in tree.rs.
pub(super) async fn sync_run_state_to_metadata(
    run_id: uuid::Uuid,
    sub_state: &RunState,
    status: ExecutionStatus,
    metadata_store: &dyn stepflow_state::MetadataStore,
    make_error: fn() -> ExecutionError,
) -> Result<()> {
    let items_state = sub_state.items_state();
    for item_index in 0..items_state.item_count() {
        let item = items_state.item(item_index);
        let result = sub_state.flow().output().resolve(item);
        let step_statuses = items_state.get_item_step_statuses(item_index);
        metadata_store
            .record_item_result(run_id, item_index as usize, result, step_statuses)
            .await
            .change_context(make_error())?;
    }
    metadata_store
        .update_run_status(run_id, status, None)
        .await
        .change_context(make_error())?;
    Ok(())
}
