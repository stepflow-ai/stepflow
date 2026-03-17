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

//! Periodic checkpoint creator for execution state.
//!
//! The [`Checkpointer`] tracks how many journal entries have been written
//! since the last checkpoint and creates a new checkpoint when the configured
//! interval is reached. It is created by the executor and passed into
//! `FlowExecutor`, which calls [`record_entry`](Checkpointer::record_entry)
//! on every journal write and [`maybe_checkpoint`](Checkpointer::maybe_checkpoint)
//! after task completions.

use std::collections::HashMap;
use std::sync::Arc;

use error_stack::ResultExt as _;
use stepflow_state::{CheckpointStore, SequenceNumber};
use uuid::Uuid;

use crate::checkpoint::CheckpointData;
use crate::run_state::RunState;
use crate::{ExecutionError, Result};

/// Periodic checkpoint creator for execution state.
///
/// Maintains an internal counter of journal entries written since the last
/// checkpoint. Does NOT rely on sequence number arithmetic (sequences may
/// be sparse, e.g. Kafka offsets). The counter is incremented by
/// [`record_entry`](Checkpointer::record_entry) (called from
/// `FlowExecutor::write_journal`), and the latest sequence is only used
/// as the checkpoint position marker.
pub struct Checkpointer {
    checkpoint_store: Arc<dyn CheckpointStore>,
    root_run_id: Uuid,
    /// Number of journal events between checkpoints (0 = disabled).
    interval: usize,
    /// Entries written since the last checkpoint.
    entries_since_checkpoint: usize,
    /// Latest sequence number from a journal write.
    latest_sequence: Option<SequenceNumber>,
}

impl Checkpointer {
    /// Create a new Checkpointer.
    ///
    /// # Arguments
    /// * `checkpoint_store` - Where to persist checkpoints
    /// * `root_run_id` - The root run ID for this execution tree
    /// * `interval` - Number of journal entries between checkpoints (0 = disabled)
    pub fn new(
        checkpoint_store: Arc<dyn CheckpointStore>,
        root_run_id: Uuid,
        interval: usize,
    ) -> Self {
        Self {
            checkpoint_store,
            root_run_id,
            interval,
            entries_since_checkpoint: 0,
            latest_sequence: None,
        }
    }

    /// Record that one journal entry was written.
    ///
    /// Called by `FlowExecutor::write_journal` on every write.
    pub fn record_entry(&mut self, sequence: SequenceNumber) {
        self.latest_sequence = Some(sequence);
        self.entries_since_checkpoint += 1;
    }

    /// Create a checkpoint if enough entries have been written since the last one.
    ///
    /// This should be called after task completions (when state has changed
    /// meaningfully). It only writes a checkpoint when the entry count reaches
    /// the configured interval.
    pub async fn maybe_checkpoint(
        &mut self,
        runs: &HashMap<Uuid, RunState>,
        subflow_map: &HashMap<(Uuid, u32, usize, Uuid), Uuid>,
        in_flight_task_ids: &HashMap<(Uuid, u32, usize), String>,
    ) -> Result<()> {
        if self.interval == 0 {
            return Ok(());
        }
        if self.entries_since_checkpoint < self.interval {
            return Ok(());
        }

        let sequence = self.latest_sequence.ok_or_else(|| {
            error_stack::report!(ExecutionError::CheckpointError)
                .attach_printable("No sequence number recorded before checkpoint attempt")
        })?;

        let entries_written = self.entries_since_checkpoint;
        let checkpoint = CheckpointData::capture(runs, subflow_map, in_flight_task_ids, sequence);
        let data = checkpoint
            .serialize()
            .change_context(ExecutionError::CheckpointError)?;

        self.checkpoint_store
            .put_checkpoint(self.root_run_id, sequence, data)
            .await
            .change_context(ExecutionError::CheckpointError)?;

        self.entries_since_checkpoint = 0;

        log::info!(
            "Checkpoint created for run {} at sequence {:?} ({} entries since last)",
            self.root_run_id,
            sequence,
            entries_written,
        );

        Ok(())
    }

    /// Delete all checkpoints for this execution tree.
    ///
    /// Called after a run completes to free storage. Failures are logged
    /// but do not propagate — checkpoint cleanup is best-effort.
    pub async fn cleanup(&self) {
        if self.interval == 0 {
            return;
        }
        if let Err(e) = self
            .checkpoint_store
            .delete_checkpoints(self.root_run_id)
            .await
        {
            log::warn!(
                "Failed to delete checkpoints for run {}: {:?}",
                self.root_run_id,
                e
            );
        }
    }
}
