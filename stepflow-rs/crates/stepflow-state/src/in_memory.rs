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

use error_stack::ResultExt as _;
use futures::future::{BoxFuture, FutureExt as _};
use std::{collections::HashMap, sync::Arc};
use stepflow_core::status::ExecutionStatus;

use crate::{
    StateStore,
    state_store::{
        RunDetails, RunFilters, RunSummary, StepInfo, StepResult, WorkflowLabelMetadata,
        WorkflowWithMetadata,
    },
};
use stepflow_core::{
    FlowResult,
    blob::{BlobData, BlobId, BlobType},
    workflow::{Flow, ValueRef},
};
use uuid::Uuid;

use crate::StateError;
use tokio::sync::RwLock;

type WorkflowLabelsMap = Arc<RwLock<HashMap<(String, String), WorkflowLabelMetadata>>>;
/// Execution-specific state storage for a single workflow execution.
#[derive(Debug)]
struct ExecutionState {
    /// Vector of step results indexed by step index
    /// None indicates the step hasn't completed yet
    step_results: Vec<Option<StepResult>>,
    /// Map from step_id to step_index for O(1) lookup by ID
    step_id_to_index: HashMap<String, usize>,
}

impl ExecutionState {
    fn new(capacity: usize) -> Self {
        Self {
            step_results: (0..capacity).map(|_| None).collect(),
            step_id_to_index: HashMap::new(),
        }
    }

    fn ensure_capacity(&mut self, step_idx: usize) {
        if step_idx >= self.step_results.len() {
            self.step_results.resize_with(step_idx + 1, || None);
        }
    }
}

impl Default for ExecutionState {
    fn default() -> Self {
        Self::new(0)
    }
}

/// In-memory implementation of StateStore.
///
/// This provides a simple, fast storage implementation suitable for
/// single-process execution. In the future, this can be extended with
/// persistent storage backends for distributed or long-running flows.
pub struct InMemoryStateStore {
    /// Map from blob ID (SHA-256 hash) to stored JSON data
    blobs: Arc<RwLock<HashMap<String, BlobData>>>,
    /// Map from run_id to execution-specific state
    executions: Arc<RwLock<HashMap<Uuid, ExecutionState>>>,
    /// Map from flow hash to flow content
    flows: Arc<RwLock<HashMap<String, Arc<Flow>>>>,
    /// Map from (flow_name, label) to flow label metadata
    flow_labels: WorkflowLabelsMap,
    /// Map from run_id to execution details
    execution_metadata: Arc<RwLock<HashMap<Uuid, RunDetails>>>,
    /// Map from run_id to step info
    step_info: Arc<RwLock<HashMap<Uuid, HashMap<usize, StepInfo>>>>,
}

impl InMemoryStateStore {
    /// Create a new in-memory state store.
    pub fn new() -> Self {
        Self {
            blobs: Arc::new(RwLock::new(HashMap::new())),
            executions: Arc::new(RwLock::new(HashMap::new())),
            flows: Arc::new(RwLock::new(HashMap::new())),
            flow_labels: Arc::new(RwLock::new(HashMap::new())),
            execution_metadata: Arc::new(RwLock::new(HashMap::new())),
            step_info: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Remove all state for a specific execution.
    /// This is useful for cleanup after workflow completion.
    ///
    /// Note: This is a concrete implementation method, not part of the StateStore trait.
    /// Eviction strategies may evolve to be more nuanced (partial eviction, etc.).
    pub async fn evict_execution(&self, run_id: Uuid) {
        let mut executions = self.executions.write().await;
        executions.remove(&run_id);

        let mut metadata = self.execution_metadata.write().await;
        metadata.remove(&run_id);

        let mut step_info = self.step_info.write().await;
        step_info.remove(&run_id);
    }

    /// Record the result of a step execution (private implementation method).
    ///
    /// This operation is executed synchronously since it's in-memory with no I/O cost.
    fn record_step_result(&self, run_id: Uuid, step_result: StepResult) {
        // For in-memory store, execute synchronously - no I/O cost justifies async complexity
        let step_idx = step_result.step_idx();

        // Block until we can get the lock - this is acceptable for in-memory operations
        let mut executions = futures::executor::block_on(self.executions.write());
        let execution_state = executions.entry(run_id).or_default();

        // Ensure the vector has enough capacity
        execution_state.ensure_capacity(step_idx);

        // Update the step_id_to_index mapping for fast lookup by ID
        execution_state
            .step_id_to_index
            .insert(step_result.step_id().to_string(), step_idx);

        // Store the result at the step index
        execution_state.step_results[step_idx] = Some(step_result);
    }

    /// Update multiple steps to the same status (private implementation method).
    ///
    /// This operation is executed synchronously since it's in-memory with no I/O cost.
    fn update_step_statuses(
        &self,
        run_id: Uuid,
        status: stepflow_core::status::StepStatus,
        step_indices: bit_set::BitSet,
    ) {
        // For in-memory store, execute synchronously
        let mut step_info_guard = futures::executor::block_on(self.step_info.write());

        if let Some(execution_steps) = step_info_guard.get_mut(&run_id) {
            let now = chrono::Utc::now();
            for step_index in step_indices.iter() {
                if let Some(step_info) = execution_steps.get_mut(&step_index) {
                    step_info.status = status;
                    step_info.updated_at = now;
                }
            }
        }
    }
}

impl Default for InMemoryStateStore {
    fn default() -> Self {
        Self::new()
    }
}

impl StateStore for InMemoryStateStore {
    fn put_blob(
        &self,
        data: ValueRef,
        blob_type: BlobType,
    ) -> BoxFuture<'_, error_stack::Result<BlobId, StateError>> {
        let blobs = self.blobs.clone();

        async move {
            let blob_id = BlobId::from_content(&data).change_context(StateError::Internal)?;
            let blob_data = BlobData::from_value_ref(data, blob_type, blob_id.clone())
                .change_context(StateError::Internal)?;

            // Store the data (overwrites are fine since content is identical)
            {
                let mut blobs = blobs.write().await;
                blobs.insert(blob_id.as_str().to_string(), blob_data);
            }

            Ok(blob_id)
        }
        .boxed()
    }

    fn get_blob(
        &self,
        blob_id: &BlobId,
    ) -> BoxFuture<'_, error_stack::Result<BlobData, StateError>> {
        let blobs = self.blobs.clone();
        let blob_id_str = blob_id.as_str().to_string();

        async move {
            let blobs = blobs.read().await;
            blobs.get(&blob_id_str).cloned().ok_or_else(|| {
                error_stack::report!(StateError::BlobNotFound {
                    blob_id: blob_id_str.clone()
                })
            })
        }
        .boxed()
    }

    fn get_step_result(
        &self,
        run_id: Uuid,
        step_idx: usize,
    ) -> BoxFuture<'_, error_stack::Result<FlowResult, StateError>> {
        let executions = self.executions.clone();
        let run_id_str = run_id.to_string();

        async move {
            let executions = executions.read().await;
            let execution_state = executions.get(&run_id).ok_or_else(|| {
                error_stack::report!(StateError::StepResultNotFoundByIndex {
                    run_id: run_id_str.clone(),
                    step_idx,
                })
            })?;

            execution_state
                .step_results
                .get(step_idx)
                .and_then(|opt| opt.as_ref())
                .map(|step_result| step_result.result().clone())
                .ok_or_else(|| {
                    error_stack::report!(StateError::StepResultNotFoundByIndex {
                        run_id: run_id_str,
                        step_idx,
                    })
                })
        }
        .boxed()
    }

    fn list_step_results(
        &self,
        run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Vec<StepResult>, StateError>> {
        let executions = self.executions.clone();

        async move {
            let executions = executions.read().await;
            let execution_state = match executions.get(&run_id) {
                Some(state) => state,
                None => return Ok(Vec::new()), // No execution found, return empty list
            };

            // Vec maintains natural ordering, so no sorting needed
            let results: Vec<StepResult> = execution_state
                .step_results
                .iter()
                .filter_map(|opt| opt.as_ref().cloned())
                .collect();

            Ok(results)
        }
        .boxed()
    }

    // Workflow Management Methods

    fn store_flow(
        &self,
        workflow: Arc<Flow>,
    ) -> BoxFuture<'_, error_stack::Result<BlobId, StateError>> {
        async move {
            // Serialize the workflow as blob data
            let flow_data = ValueRef::new(serde_json::to_value(workflow.as_ref()).unwrap());

            // Store as a blob with Flow type
            self.put_blob(flow_data, BlobType::Flow).await
        }
        .boxed()
    }

    fn get_flow(
        &self,
        flow_id: &BlobId,
    ) -> BoxFuture<'_, error_stack::Result<Option<Arc<Flow>>, StateError>> {
        let flow_id = flow_id.clone();
        let blobs = self.blobs.clone();

        async move {
            // Try to get the blob data directly
            let blobs = blobs.read().await;
            match blobs.get(flow_id.as_str()) {
                Some(blob_data) => {
                    // Use the typed accessor
                    if let Some(flow) = blob_data.as_flow() {
                        Ok(Some(flow.clone()))
                    } else {
                        Ok(None) // Not a flow blob
                    }
                }
                None => Ok(None), // Blob not found
            }
        }
        .boxed()
    }

    fn get_flows(
        &self,
        _name: &str,
    ) -> BoxFuture<'_, error_stack::Result<Vec<(BlobId, chrono::DateTime<chrono::Utc>)>, StateError>>
    {
        async move {
            // TODO: Implement workflow name tracking in blob-based storage
            // For now, return empty list since we no longer track workflow names separately
            Ok(Vec::new())
        }
        .boxed()
    }

    fn get_named_flow(
        &self,
        name: &str,
        label: Option<&str>,
    ) -> BoxFuture<'_, error_stack::Result<Option<WorkflowWithMetadata>, StateError>> {
        let flows = self.flows.clone();
        let flow_labels = self.flow_labels.clone();
        let name = name.to_string();
        let label = label.map(|s| s.to_string());

        async move {
            match label {
                Some(label_str) => {
                    // Get workflow by label
                    let labels = flow_labels.read().await;
                    let key = (name, label_str);

                    if let Some(label_metadata) = labels.get(&key) {
                        let flows = flows.read().await;
                        if let Some(workflow) = flows.get(&label_metadata.flow_id.to_string()) {
                            return Ok(Some(WorkflowWithMetadata {
                                workflow: workflow.clone(),
                                flow_id: label_metadata.flow_id.clone(),
                                created_at: label_metadata.created_at,
                                label_info: Some(label_metadata.clone()),
                            }));
                        }
                    }
                    Ok(None)
                }
                None => {
                    // TODO: Implement name-based workflow lookup with blob storage
                    // For now, return None since we no longer maintain a separate workflow index
                    Ok(None)
                }
            }
        }
        .boxed()
    }

    fn create_or_update_label(
        &self,
        name: &str,
        label: &str,
        flow_id: BlobId,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let workflow_labels = self.flow_labels.clone();
        let name = name.to_string();
        let label = label.to_string();

        async move {
            let now = chrono::Utc::now();
            let workflow_label = WorkflowLabelMetadata {
                name: name.clone(),
                label: label.clone(),
                flow_id,
                created_at: now,
                updated_at: now,
            };

            let mut labels = workflow_labels.write().await;
            labels.insert((name, label), workflow_label);

            Ok(())
        }
        .boxed()
    }

    fn list_labels_for_name(
        &self,
        name: &str,
    ) -> BoxFuture<'_, error_stack::Result<Vec<WorkflowLabelMetadata>, StateError>> {
        let workflow_labels = self.flow_labels.clone();
        let name = name.to_string();

        async move {
            let labels = workflow_labels.read().await;
            let results: Vec<WorkflowLabelMetadata> = labels
                .iter()
                .filter(|((n, _label), _workflow_label)| n == &name)
                .map(|((_name, _label), workflow_label)| workflow_label.clone())
                .collect();
            Ok(results)
        }
        .boxed()
    }

    fn list_flow_names(&self) -> BoxFuture<'_, error_stack::Result<Vec<String>, StateError>> {
        let flows = self.flows.clone();

        async move {
            let flows = flows.read().await;
            let mut names = std::collections::HashSet::new();

            for flow in flows.values() {
                if let Some(name) = flow.name() {
                    names.insert(name.to_owned());
                }
            }

            let mut result: Vec<String> = names.into_iter().collect();
            result.sort();
            Ok(result)
        }
        .boxed()
    }

    fn delete_label(
        &self,
        name: &str,
        label: &str,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let workflow_labels = self.flow_labels.clone();
        let name = name.to_string();
        let label = label.to_string();

        async move {
            let mut labels = workflow_labels.write().await;
            let key = (name, label);
            labels.remove(&key);

            Ok(())
        }
        .boxed()
    }

    fn create_run(
        &self,
        run_id: Uuid,
        flow_id: BlobId,
        workflow_name: Option<&str>,
        workflow_label: Option<&str>,
        debug_mode: bool,
        input: ValueRef,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let metadata = self.execution_metadata.clone();
        let now = chrono::Utc::now();
        let execution_details = RunDetails {
            summary: RunSummary {
                run_id,
                flow_id,
                flow_name: workflow_name.map(|s| s.to_string()),
                flow_label: workflow_label.map(|s| s.to_string()),
                status: ExecutionStatus::Running,
                debug_mode,
                created_at: now,
                completed_at: None,
            },
            input,
            result: None,
        };

        async move {
            let mut metadata = metadata.write().await;
            metadata.insert(run_id, execution_details);
            Ok(())
        }
        .boxed()
    }

    fn update_run_status(
        &self,
        run_id: Uuid,
        status: ExecutionStatus,
        result: Option<ValueRef>,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let metadata = self.execution_metadata.clone();

        async move {
            let mut metadata = metadata.write().await;
            if let Some(exec_metadata) = metadata.get_mut(&run_id) {
                exec_metadata.summary.status = status;
                exec_metadata.result = result.map(FlowResult::Success);

                if matches!(status, ExecutionStatus::Completed | ExecutionStatus::Failed) {
                    exec_metadata.summary.completed_at = Some(chrono::Utc::now());
                }
            }
            Ok(())
        }
        .boxed()
    }

    fn get_run(
        &self,
        run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Option<RunDetails>, StateError>> {
        let metadata = self.execution_metadata.clone();

        async move {
            let metadata = metadata.read().await;
            let exec_metadata = match metadata.get(&run_id) {
                Some(metadata) => metadata,
                None => return Ok(None),
            };

            Ok(Some(exec_metadata.clone()))
        }
        .boxed()
    }

    fn list_runs(
        &self,
        filters: &RunFilters,
    ) -> BoxFuture<'_, error_stack::Result<Vec<RunSummary>, StateError>> {
        let metadata = self.execution_metadata.clone();
        let filters = filters.clone();

        async move {
            let metadata = metadata.read().await;
            let mut results: Vec<RunSummary> = metadata
                .values()
                .filter(|exec| {
                    // Apply status filter
                    if let Some(ref status) = filters.status
                        && &exec.summary.status != status
                    {
                        return false;
                    }

                    // Apply workflow name filter
                    if let Some(ref workflow_name) = filters.flow_name
                        && exec.summary.flow_name.as_ref() != Some(workflow_name)
                    {
                        return false;
                    }

                    // Apply workflow label filter
                    if let Some(ref workflow_label) = filters.flow_label
                        && exec.summary.flow_label.as_ref() != Some(workflow_label)
                    {
                        return false;
                    }

                    true
                })
                .map(|exec| exec.summary.clone())
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

    // Step Status Management

    fn initialize_step_info(
        &self,
        run_id: uuid::Uuid,
        steps: &[StepInfo],
    ) -> BoxFuture<'_, error_stack::Result<(), crate::StateError>> {
        let steps = steps.to_vec();
        let step_info_map = self.step_info.clone();

        async move {
            let mut step_info_guard = step_info_map.write().await;

            // Create a map from step_index to StepInfo for this execution
            let mut execution_steps = HashMap::new();
            for step in steps {
                execution_steps.insert(step.step_index, step);
            }

            step_info_guard.insert(run_id, execution_steps);
            Ok(())
        }
        .boxed()
    }

    fn update_step_status(
        &self,
        run_id: uuid::Uuid,
        step_index: usize,
        status: stepflow_core::status::StepStatus,
    ) {
        // For in-memory store, execute synchronously
        let mut step_info_guard = futures::executor::block_on(self.step_info.write());

        if let Some(execution_steps) = step_info_guard.get_mut(&run_id)
            && let Some(step_info) = execution_steps.get_mut(&step_index)
        {
            step_info.status = status;
            step_info.updated_at = chrono::Utc::now();
        }
    }

    fn queue_write(
        &self,
        operation: crate::StateWriteOperation,
    ) -> error_stack::Result<(), crate::StateError> {
        // For in-memory store, process operations immediately since there's no I/O cost
        match operation {
            crate::StateWriteOperation::RecordStepResult {
                run_id,
                step_result,
            } => {
                self.record_step_result(run_id, step_result);
                Ok(())
            }
            crate::StateWriteOperation::UpdateStepStatuses {
                run_id,
                status,
                step_indices,
            } => {
                self.update_step_statuses(run_id, status, step_indices);
                Ok(())
            }
            crate::StateWriteOperation::Flush {
                run_id: _,
                completion_notify,
            } => {
                // For in-memory store, all writes are immediate, so nothing to flush
                let _ = completion_notify.send(Ok(()));
                Ok(())
            }
        }
    }

    fn flush_pending_writes(
        &self,
        _run_id: uuid::Uuid,
    ) -> BoxFuture<'_, error_stack::Result<(), crate::StateError>> {
        // For in-memory store, all writes are immediate, so nothing to flush
        async move { Ok(()) }.boxed()
    }

    fn get_step_info_for_execution(
        &self,
        run_id: uuid::Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Vec<StepInfo>, crate::StateError>> {
        let step_info_map = self.step_info.clone();

        async move {
            let step_info_guard = step_info_map.read().await;

            let step_infos = step_info_guard
                .get(&run_id)
                .map(|execution_steps| {
                    let mut steps: Vec<StepInfo> = execution_steps.values().cloned().collect();
                    // Sort by step_index for consistent ordering
                    steps.sort_by_key(|step| step.step_index);
                    steps
                })
                .unwrap_or_default();

            Ok(step_infos)
        }
        .boxed()
    }

    fn get_runnable_steps(
        &self,
        run_id: uuid::Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Vec<StepInfo>, crate::StateError>> {
        let step_info_map = self.step_info.clone();

        async move {
            let step_info_guard = step_info_map.read().await;

            // Get all step info for this execution
            let execution_steps = step_info_guard.get(&run_id).cloned().unwrap_or_default();

            // Find steps that are marked as runnable
            let mut runnable_steps = Vec::new();

            for step_info in execution_steps.values() {
                if step_info.status == stepflow_core::status::StepStatus::Runnable {
                    runnable_steps.push(step_info.clone());
                }
            }

            // Sort by step_index for consistent ordering
            runnable_steps.sort_by_key(|step| step.step_index);

            Ok(runnable_steps)
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

    #[tokio::test]
    async fn test_step_result_storage() {
        use stepflow_core::workflow::ValueRef;

        let store = InMemoryStateStore::new();
        let run_id = Uuid::new_v4();

        // Test data
        let step1_result =
            FlowResult::Success(ValueRef::new(serde_json::json!({"output":"hello"})));
        let step2_result = FlowResult::Skipped;

        // Record step results with both index and ID
        store.record_step_result(run_id, StepResult::new(0, "step1", step1_result.clone()));
        store.record_step_result(run_id, StepResult::new(1, "step2", step2_result.clone()));

        // Retrieve by index
        let retrieved_by_idx_0 = store.get_step_result(run_id, 0).await.unwrap();
        let retrieved_by_idx_1 = store.get_step_result(run_id, 1).await.unwrap();
        assert_eq!(retrieved_by_idx_0, step1_result);
        assert_eq!(retrieved_by_idx_1, step2_result);

        // List all step results (should be ordered by index)
        let all_results = store.list_step_results(run_id).await.unwrap();
        assert_eq!(all_results.len(), 2);
        assert_eq!(all_results[0], StepResult::new(0, "step1", step1_result));
        assert_eq!(all_results[1], StepResult::new(1, "step2", step2_result));

        // Non-existent step should return error
        let result_by_idx = store.get_step_result(run_id, 99).await;
        assert!(result_by_idx.is_err());

        // Different execution ID should return empty list
        let other_run_id = Uuid::new_v4();
        let other_results = store.list_step_results(other_run_id).await.unwrap();
        assert!(other_results.is_empty());
    }

    #[tokio::test]
    async fn test_step_result_overwrite() {
        use stepflow_core::workflow::ValueRef;

        let store = InMemoryStateStore::new();
        let run_id = Uuid::new_v4();

        // Record initial result
        let initial_result = FlowResult::Success(ValueRef::new(serde_json::json!({"attempt":1})));
        store.record_step_result(run_id, StepResult::new(0, "step1", initial_result));

        // Overwrite with new result
        let new_result = FlowResult::Success(ValueRef::new(serde_json::json!({"attempt":2})));
        store.record_step_result(run_id, StepResult::new(0, "step1", new_result.clone()));

        // Should retrieve the new result by both index and ID
        let retrieved_by_idx = store.get_step_result(run_id, 0).await.unwrap();

        assert_eq!(retrieved_by_idx, new_result);

        // Should still only have one entry for this step
        let all_results = store.list_step_results(run_id).await.unwrap();
        assert_eq!(all_results.len(), 1);
        assert_eq!(all_results[0], StepResult::new(0, "step1", new_result));
    }

    #[tokio::test]
    async fn test_step_result_ordering() {
        let store = InMemoryStateStore::new();
        let run_id = Uuid::new_v4();

        // Insert steps out of order to test ordering
        let step2_result = FlowResult::Success(ValueRef::new(serde_json::json!({"step":2})));
        let step0_result = FlowResult::Success(ValueRef::new(serde_json::json!({"step":0})));
        let step1_result = FlowResult::Skipped;

        // Record in non-sequential order
        store.record_step_result(run_id, StepResult::new(2, "step2", step2_result.clone()));
        store.record_step_result(run_id, StepResult::new(0, "step0", step0_result.clone()));
        store.record_step_result(run_id, StepResult::new(1, "step1", step1_result.clone()));

        // List should return results ordered by step index
        let all_results = store.list_step_results(run_id).await.unwrap();
        assert_eq!(all_results.len(), 3);
        assert_eq!(all_results[0], StepResult::new(0, "step0", step0_result));
        assert_eq!(all_results[1], StepResult::new(1, "step1", step1_result));
        assert_eq!(all_results[2], StepResult::new(2, "step2", step2_result));
    }

    #[tokio::test]
    async fn test_execution_eviction() {
        let store = InMemoryStateStore::new();
        let run_id = Uuid::new_v4();

        // Store some step results
        let step_result = FlowResult::Success(ValueRef::new(serde_json::json!({"output":"test"})));
        store.record_step_result(run_id, StepResult::new(0, "step1", step_result.clone()));

        // Verify the result exists
        let retrieved = store.get_step_result(run_id, 0).await.unwrap();
        assert_eq!(retrieved, step_result);

        // Evict the execution
        store.evict_execution(run_id).await;

        // Verify the result no longer exists
        let result = store.get_step_result(run_id, 0).await;
        assert!(result.is_err());

        // Verify list returns empty
        let all_results = store.list_step_results(run_id).await.unwrap();
        assert!(all_results.is_empty());
    }
}
