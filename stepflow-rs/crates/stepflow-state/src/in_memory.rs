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

use dashmap::DashMap;
use error_stack::ResultExt as _;
use futures::future::{BoxFuture, FutureExt as _};
use std::collections::{HashMap, HashSet};
use stepflow_core::status::ExecutionStatus;

use crate::{
    BlobStore, ExecutionJournal, JournalEntry, MetadataStore, RootJournalInfo,
    RunCompletionNotifier, SequenceNumber, state_store::CreateRunParams,
};
use stepflow_core::{
    FlowResult,
    blob::{BlobData, BlobId, BlobType},
    workflow::{ValueRef, WorkflowOverrides},
};
use stepflow_dtos::{
    ItemDetails, ItemResult, ItemStatistics, ResultOrder, RunDetails, RunFilters, RunSummary,
};
use uuid::Uuid;

use crate::StateError;

/// Combined state for a workflow run.
/// Groups all run-related data together under a single key.
#[derive(Debug)]
struct RunState {
    /// Run metadata and input information (no results - stored separately)
    details: RunDetails,
    /// Item results by item index
    item_results: HashMap<usize, FlowResult>,
    /// Per-item step statuses by item index
    item_step_statuses: HashMap<usize, Vec<stepflow_dtos::StepStatusInfo>>,
    /// Per-item completion timestamps
    item_completed_at: HashMap<usize, chrono::DateTime<chrono::Utc>>,
}

impl RunState {
    fn new(details: RunDetails) -> Self {
        Self {
            details,
            item_results: HashMap::new(),
            item_step_statuses: HashMap::new(),
            item_completed_at: HashMap::new(),
        }
    }
}

/// Journal state for a single run.
#[derive(Debug, Default)]
struct JournalState {
    /// Journal entries stored in order.
    entries: Vec<JournalEntry>,
}

/// In-memory implementation of MetadataStore, BlobStore, and ExecutionJournal.
///
/// This provides a simple, fast storage implementation suitable for
/// single-process execution. Uses DashMap for concurrent access with
/// fine-grained locking per key.
pub struct InMemoryStateStore {
    /// Map from blob ID (SHA-256 hash) to stored JSON data
    blobs: DashMap<String, BlobData>,
    /// Map from run_id to combined run state
    runs: DashMap<Uuid, RunState>,
    /// Notifier for run completion events
    completion_notifier: RunCompletionNotifier,
    /// Map from root_run_id to journal state.
    /// All events for an execution tree (parent + subflows) share the same journal.
    journals: DashMap<Uuid, JournalState>,
}

impl InMemoryStateStore {
    /// Create a new in-memory state store.
    pub fn new() -> Self {
        Self {
            blobs: DashMap::new(),
            runs: DashMap::new(),
            completion_notifier: RunCompletionNotifier::new(),
            journals: DashMap::new(),
        }
    }

    /// Remove all state for a specific execution.
    /// This is useful for cleanup after workflow completion.
    ///
    /// Note: This is a concrete implementation method, not part of the StateStore trait.
    /// Eviction strategies may evolve to be more nuanced (partial eviction, etc.).
    pub fn evict_execution(&self, run_id: Uuid) {
        self.runs.remove(&run_id);
    }

    /// Synchronous version of create_run for use in queue_write.
    fn create_run_sync(&self, params: CreateRunParams) {
        let now = chrono::Utc::now();
        let item_count = params.item_count();

        // Build item_details from inputs
        let item_details: Vec<ItemDetails> = params
            .inputs
            .iter()
            .enumerate()
            .map(|(idx, input)| ItemDetails {
                item_index: idx as u32,
                input: input.clone(),
                status: ExecutionStatus::Running,
                steps: Vec::new(),
                completed_at: None,
            })
            .collect();

        let execution_details = RunDetails {
            summary: RunSummary {
                run_id: params.run_id,
                flow_id: params.flow_id,
                flow_name: params.workflow_name,
                status: ExecutionStatus::Running,
                items: ItemStatistics {
                    total: item_count,
                    running: item_count,
                    ..Default::default()
                },
                created_at: now,
                completed_at: None,
                root_run_id: params.root_run_id,
                parent_run_id: params.parent_run_id,
                orchestrator_id: params.orchestrator_id,
            },
            item_details: Some(item_details),
            overrides: if params.overrides.is_empty() {
                None
            } else {
                Some(params.overrides)
            },
        };

        // Idempotent: only insert if not exists (preserves existing run)
        self.runs
            .entry(params.run_id)
            .or_insert_with(|| RunState::new(execution_details));
    }
}

impl Default for InMemoryStateStore {
    fn default() -> Self {
        Self::new()
    }
}

impl BlobStore for InMemoryStateStore {
    fn put_blob(
        &self,
        data: ValueRef,
        blob_type: BlobType,
    ) -> BoxFuture<'_, error_stack::Result<BlobId, StateError>> {
        async move {
            let blob_id =
                BlobId::compute(&data, &blob_type).change_context(StateError::Serialization)?;
            let blob_data = BlobData::from_value_ref(data, blob_type, blob_id.clone())
                .change_context(StateError::Serialization)?;

            // Store the data (overwrites are fine since content is identical)
            self.blobs.insert(blob_id.as_str().to_string(), blob_data);

            Ok(blob_id)
        }
        .boxed()
    }

    fn get_blob_opt(
        &self,
        blob_id: &BlobId,
    ) -> BoxFuture<'_, error_stack::Result<Option<BlobData>, StateError>> {
        let blob_id_str = blob_id.as_str().to_string();

        async move {
            Ok(self
                .blobs
                .get(&blob_id_str)
                .map(|entry| entry.value().clone()))
        }
        .boxed()
    }

    fn set_blob_filename(
        &self,
        blob_id: &BlobId,
        filename: String,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let blob_id_str = blob_id.as_str().to_string();
        let blob_id = blob_id.clone();
        async move {
            if let Some(mut entry) = self.blobs.get_mut(&blob_id_str) {
                entry.value_mut().filename = Some(filename);
                Ok(())
            } else {
                Err(error_stack::report!(StateError::BlobNotFound {
                    blob_id: blob_id.to_string()
                }))
            }
        }
        .boxed()
    }

    // Note: store_flow and get_flow use default implementations from the trait
}

impl MetadataStore for InMemoryStateStore {
    fn create_run(
        &self,
        params: CreateRunParams,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        self.create_run_sync(params);
        async move { Ok(()) }.boxed()
    }

    fn get_run(
        &self,
        run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Option<RunDetails>, StateError>> {
        async move {
            Ok(self.runs.get(&run_id).map(|run_state| {
                let mut details = run_state.details.clone();
                // Populate item_details with step statuses and results from storage
                if let Some(ref mut item_details) = details.item_details {
                    for item in item_details.iter_mut() {
                        let idx = item.item_index as usize;
                        // Update steps from stored step_statuses
                        if let Some(step_statuses) = run_state.item_step_statuses.get(&idx) {
                            item.steps = step_statuses.clone();
                        }
                        // Update status from stored result
                        if let Some(result) = run_state.item_results.get(&idx) {
                            item.status = match result {
                                FlowResult::Success(_) => ExecutionStatus::Completed,
                                FlowResult::Failed(_) => ExecutionStatus::Failed,
                            };
                        }
                        // Update completed_at from stored timestamp
                        if let Some(completed_at) = run_state.item_completed_at.get(&idx) {
                            item.completed_at = Some(*completed_at);
                        }
                    }
                }
                details
            }))
        }
        .boxed()
    }

    fn list_runs(
        &self,
        filters: &RunFilters,
    ) -> BoxFuture<'_, error_stack::Result<Vec<RunSummary>, StateError>> {
        let filters = filters.clone();

        async move {
            let mut results: Vec<RunSummary> = self
                .runs
                .iter()
                .map(|entry| entry.details.summary.clone())
                .filter(|exec| {
                    // Apply status filter
                    if let Some(ref status) = filters.status
                        && &exec.status != status
                    {
                        return false;
                    }

                    // Apply workflow name filter
                    if let Some(ref workflow_name) = filters.flow_name
                        && exec.flow_name.as_ref() != Some(workflow_name)
                    {
                        return false;
                    }

                    // Apply root_run_id filter (get all runs in this tree)
                    if let Some(root_run_id) = filters.root_run_id
                        && exec.root_run_id != root_run_id
                    {
                        return false;
                    }

                    // Apply parent_run_id filter (get direct children of this parent)
                    if let Some(parent_run_id) = filters.parent_run_id
                        && exec.parent_run_id != Some(parent_run_id)
                    {
                        return false;
                    }

                    // Apply roots_only filter (get only top-level runs)
                    if filters.roots_only == Some(true) && exec.parent_run_id.is_some() {
                        return false;
                    }

                    // Apply orchestrator_id filter
                    if let Some(ref orch_filter) = filters.orchestrator_id
                        && exec.orchestrator_id.as_ref() != orch_filter.as_ref()
                    {
                        return false;
                    }

                    true
                })
                .collect();

            // Sort by creation time (newest first)
            results.sort_by(|a, b| b.created_at.cmp(&a.created_at));

            // Apply pagination
            if let Some(offset) = filters.offset {
                if offset < results.len() {
                    results = results[offset..].to_vec();
                } else {
                    results.clear();
                }
            }

            if let Some(limit) = filters.limit {
                results.truncate(limit);
            }

            Ok(results)
        }
        .boxed()
    }

    fn update_run_status(
        &self,
        run_id: Uuid,
        status: ExecutionStatus,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        async move {
            let is_terminal = matches!(
                status,
                ExecutionStatus::Completed
                    | ExecutionStatus::Failed
                    | ExecutionStatus::Cancelled
                    | ExecutionStatus::RecoveryFailed
            );

            if let Some(mut run_state) = self.runs.get_mut(&run_id) {
                run_state.details.summary.status = status;

                if is_terminal {
                    run_state.details.summary.completed_at = Some(chrono::Utc::now());
                }
            }

            // Notify waiters if the run reached a terminal status
            if is_terminal {
                self.completion_notifier.notify_completion(run_id);
            }

            Ok(())
        }
        .boxed()
    }

    fn get_run_overrides(
        &self,
        run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Option<WorkflowOverrides>, StateError>> {
        async move {
            Ok(self
                .runs
                .get(&run_id)
                .map(|run_state| run_state.details.overrides.clone())
                .unwrap_or(None))
        }
        .boxed()
    }

    fn record_item_result(
        &self,
        run_id: Uuid,
        item_index: usize,
        result: FlowResult,
        step_statuses: Vec<stepflow_dtos::StepStatusInfo>,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        async move {
            let mut run_completed = false;

            if let Some(mut run_state) = self.runs.get_mut(&run_id) {
                let now = chrono::Utc::now();

                // Check if this item was previously not recorded (running)
                let was_running = !run_state.item_results.contains_key(&item_index);

                // Update the result, step statuses, and record completion time
                run_state.item_results.insert(item_index, result.clone());
                run_state
                    .item_step_statuses
                    .insert(item_index, step_statuses);
                run_state.item_completed_at.insert(item_index, now);

                // Update item statistics
                if was_running {
                    run_state.details.summary.items.running =
                        run_state.details.summary.items.running.saturating_sub(1);
                }
                match result {
                    FlowResult::Success(_) => {
                        run_state.details.summary.items.completed += 1;
                    }
                    FlowResult::Failed(_) => {
                        run_state.details.summary.items.failed += 1;
                    }
                }

                // Check if all items are complete
                if run_state.details.summary.items.running == 0 {
                    // Determine overall status based on item outcomes
                    let status = if run_state.details.summary.items.failed > 0 {
                        ExecutionStatus::Failed
                    } else {
                        ExecutionStatus::Completed
                    };
                    run_state.details.summary.status = status;
                    run_state.details.summary.completed_at = Some(now);
                    run_completed = true;
                }
            }

            // Notify waiters if the run completed
            if run_completed {
                self.completion_notifier.notify_completion(run_id);
            }

            Ok(())
        }
        .boxed()
    }

    fn get_item_results(
        &self,
        run_id: Uuid,
        order: ResultOrder,
    ) -> BoxFuture<'_, error_stack::Result<Vec<ItemResult>, StateError>> {
        async move {
            let run_state = match self.runs.get(&run_id) {
                Some(state) => state,
                None => return Ok(Vec::new()),
            };

            let item_count = run_state.details.summary.items.total;
            let mut items: Vec<ItemResult> = (0..item_count)
                .map(|idx| {
                    let result = run_state.item_results.get(&idx).cloned();
                    let status = match &result {
                        Some(FlowResult::Success(_)) => ExecutionStatus::Completed,
                        Some(FlowResult::Failed(_)) => ExecutionStatus::Failed,
                        None => ExecutionStatus::Running,
                    };
                    ItemResult {
                        item_index: idx,
                        status,
                        result,
                        completed_at: run_state.item_completed_at.get(&idx).copied(),
                    }
                })
                .collect();

            // Sort by completion time if requested
            if order == ResultOrder::ByCompletion {
                items.sort_by(|a, b| {
                    match (&a.completed_at, &b.completed_at) {
                        // Both have completion times - sort by time
                        (Some(a_time), Some(b_time)) => a_time.cmp(b_time),
                        // Items with completion time come first
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        // Neither has completion time - preserve index order
                        (None, None) => a.item_index.cmp(&b.item_index),
                    }
                });
            }

            Ok(items)
        }
        .boxed()
    }

    fn update_run_orchestrator(
        &self,
        run_id: Uuid,
        orchestrator_id: Option<String>,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        async move {
            if let Some(mut run_state) = self.runs.get_mut(&run_id) {
                run_state.details.summary.orchestrator_id = orchestrator_id;
            }
            Ok(())
        }
        .boxed()
    }

    fn orphan_runs_by_stale_orchestrators(
        &self,
        live_orchestrator_ids: &HashSet<String>,
    ) -> BoxFuture<'_, error_stack::Result<usize, StateError>> {
        let live_ids = live_orchestrator_ids.clone();
        async move {
            let mut orphaned = 0;
            for mut entry in self.runs.iter_mut() {
                let summary = &mut entry.details.summary;
                if summary.status == ExecutionStatus::Running
                    && let Some(ref orch_id) = summary.orchestrator_id
                    && !live_ids.contains(orch_id)
                {
                    summary.orchestrator_id = None;
                    orphaned += 1;
                }
            }
            Ok(orphaned)
        }
        .boxed()
    }

    fn wait_for_completion(
        &self,
        run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        async move {
            // Subscribe BEFORE checking status to avoid race condition where
            // completion happens between status check and subscribe
            let receiver = self.completion_notifier.subscribe();

            // Now check if already complete
            if let Some(run_state) = self.runs.get(&run_id) {
                if matches!(
                    run_state.details.summary.status,
                    ExecutionStatus::Completed
                        | ExecutionStatus::Failed
                        | ExecutionStatus::Cancelled
                        | ExecutionStatus::RecoveryFailed
                ) {
                    return Ok(());
                }
            } else {
                return Err(error_stack::report!(StateError::RunNotFound { run_id }));
            }

            // Wait for notification using the receiver we subscribed before status check
            self.completion_notifier
                .wait_for_completion_with_receiver(run_id, receiver)
                .await;

            // After receiving our notification, poll until we see the terminal status.
            // We should only receive our run_id's notification once, so if the status
            // isn't terminal yet (due to timing), just poll rather than waiting again.
            loop {
                if let Some(run_state) = self.runs.get(&run_id) {
                    if matches!(
                        run_state.details.summary.status,
                        ExecutionStatus::Completed
                            | ExecutionStatus::Failed
                            | ExecutionStatus::Cancelled
                            | ExecutionStatus::RecoveryFailed
                    ) {
                        return Ok(());
                    }
                } else {
                    return Err(error_stack::report!(StateError::RunNotFound { run_id }));
                }

                // Brief yield to allow the status update to propagate
                tokio::task::yield_now().await;
            }
        }
        .boxed()
    }
}

impl ExecutionJournal for InMemoryStateStore {
    fn append(
        &self,
        entry: JournalEntry,
    ) -> BoxFuture<'_, error_stack::Result<SequenceNumber, crate::StateError>> {
        async move {
            // Key by root_run_id so all events for an execution tree share one journal
            let mut journal = self.journals.entry(entry.root_run_id).or_default();
            // Sequence number is current length (0-indexed)
            let sequence = SequenceNumber::new(journal.entries.len() as u64);
            journal.entries.push(entry);
            Ok(sequence)
        }
        .boxed()
    }

    fn flush(
        &self,
        _root_run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<(), crate::StateError>> {
        // In-memory journal has no buffering - all writes are immediate
        async move { Ok(()) }.boxed()
    }

    fn read_from(
        &self,
        root_run_id: Uuid,
        from_sequence: SequenceNumber,
        limit: usize,
    ) -> BoxFuture<'_, error_stack::Result<Vec<(SequenceNumber, JournalEntry)>, crate::StateError>>
    {
        async move {
            let journal = match self.journals.get(&root_run_id) {
                Some(j) => j,
                None => return Ok(Vec::new()),
            };

            let start_idx = from_sequence.value() as usize;

            let entries: Vec<_> = journal
                .entries
                .iter()
                .enumerate()
                .skip(start_idx)
                .take(limit)
                .map(|(idx, entry)| {
                    let seq = SequenceNumber::new(idx as u64);
                    (seq, entry.clone())
                })
                .collect();

            Ok(entries)
        }
        .boxed()
    }

    fn latest_sequence(
        &self,
        root_run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Option<SequenceNumber>, crate::StateError>> {
        async move {
            let result = self.journals.get(&root_run_id).and_then(|journal| {
                if journal.entries.is_empty() {
                    None
                } else {
                    // Latest sequence is length - 1
                    Some(SequenceNumber::new((journal.entries.len() - 1) as u64))
                }
            });
            Ok(result)
        }
        .boxed()
    }

    fn list_active_roots(
        &self,
    ) -> BoxFuture<'_, error_stack::Result<Vec<RootJournalInfo>, crate::StateError>> {
        async move {
            let mut infos = Vec::new();

            for entry in self.journals.iter() {
                let root_run_id = *entry.key();
                let journal = entry.value();

                if journal.entries.is_empty() {
                    continue;
                }

                // Latest sequence is length - 1
                let latest_sequence = SequenceNumber::new((journal.entries.len() - 1) as u64);

                infos.push(RootJournalInfo {
                    root_run_id,
                    latest_sequence,
                    entry_count: journal.entries.len() as u64,
                });
            }

            Ok(infos)
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_blob_storage() {
        let store = InMemoryStateStore::new();

        // Test data
        let test_data = json!({"message": "Hello, world!", "count": 42});
        let value_ref = ValueRef::new(test_data.clone());

        // Create blob
        let blob_id = store
            .put_blob(value_ref.clone(), stepflow_core::BlobType::Data)
            .await
            .unwrap();

        // Blob ID should be deterministic (SHA-256 hash)
        assert_eq!(blob_id.as_str().len(), 64); // SHA-256 produces 64 hex characters

        // Retrieve blob
        let retrieved = store.get_blob(&blob_id).await.unwrap();
        assert_eq!(retrieved.data().as_ref(), &test_data);

        // Same content should produce same blob ID
        let value_ref2 = ValueRef::new(test_data.clone());
        let blob_id2 = store
            .put_blob(value_ref2, stepflow_core::BlobType::Data)
            .await
            .unwrap();
        assert_eq!(blob_id, blob_id2);

        // Non-existent blob should return error
        let fake_blob_id = BlobId::new("a".repeat(64)).unwrap();
        let result = store.get_blob(&fake_blob_id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_blob_id_validation() {
        // Valid blob ID
        let valid_id = BlobId::new("a".repeat(64)).unwrap();
        assert_eq!(valid_id.as_str().len(), 64);

        // Invalid length
        let invalid_length = BlobId::new("abc".to_string());
        assert!(invalid_length.is_err());

        // Invalid characters
        let invalid_chars = BlobId::new("g".repeat(64));
        assert!(invalid_chars.is_err());
    }

    #[tokio::test]
    async fn test_blob_id_from_content() {
        let data1 = ValueRef::new(json!({"key": "value"}));
        let data2 = ValueRef::new(json!({"key": "value"}));
        let data3 = ValueRef::new(json!({"key": "different"}));

        let id1 = BlobId::from_content(&data1).unwrap();
        let id2 = BlobId::from_content(&data2).unwrap();
        let id3 = BlobId::from_content(&data3).unwrap();

        // Same content produces same ID
        assert_eq!(id1, id2);

        // Different content produces different ID
        assert_ne!(id1, id3);

        // All IDs are valid SHA-256 hashes
        assert_eq!(id1.as_str().len(), 64);
        assert_eq!(id3.as_str().len(), 64);
    }
}
