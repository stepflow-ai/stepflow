use error_stack::ResultExt as _;
use futures::future::{BoxFuture, FutureExt as _};
use std::{collections::HashMap, sync::Arc};
use stepflow_core::status::ExecutionStatus;
use stepflow_core::workflow::FlowHash;

use crate::{
    StateStore,
    state_store::{
        ExecutionDetails, ExecutionFilters, ExecutionSummary, StepInfo, StepResult,
        WorkflowLabelMetadata, WorkflowWithMetadata,
    },
};
use stepflow_core::{
    FlowResult,
    blob::BlobId,
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
    step_results: Vec<Option<StepResult<'static>>>,
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
/// persistent storage backends for distributed or long-running workflows.
pub struct InMemoryStateStore {
    /// Map from blob ID (SHA-256 hash) to stored JSON data
    blobs: Arc<RwLock<HashMap<String, ValueRef>>>,
    /// Map from execution_id to execution-specific state
    executions: Arc<RwLock<HashMap<Uuid, ExecutionState>>>,
    /// Map from workflow hash to workflow content
    workflows: Arc<RwLock<HashMap<String, Arc<Flow>>>>,
    /// Map from (workflow_name, label) to workflow label metadata
    workflow_labels: WorkflowLabelsMap,
    /// Map from execution_id to execution details
    execution_metadata: Arc<RwLock<HashMap<Uuid, ExecutionDetails>>>,
    /// Map from execution_id to step info
    step_info: Arc<RwLock<HashMap<Uuid, HashMap<usize, StepInfo>>>>,
}

impl InMemoryStateStore {
    /// Create a new in-memory state store.
    pub fn new() -> Self {
        Self {
            blobs: Arc::new(RwLock::new(HashMap::new())),
            executions: Arc::new(RwLock::new(HashMap::new())),
            workflows: Arc::new(RwLock::new(HashMap::new())),
            workflow_labels: Arc::new(RwLock::new(HashMap::new())),
            execution_metadata: Arc::new(RwLock::new(HashMap::new())),
            step_info: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Remove all state for a specific execution.
    /// This is useful for cleanup after workflow completion.
    ///
    /// Note: This is a concrete implementation method, not part of the StateStore trait.
    /// Eviction strategies may evolve to be more nuanced (partial eviction, etc.).
    pub async fn evict_execution(&self, execution_id: Uuid) {
        let mut executions = self.executions.write().await;
        executions.remove(&execution_id);

        let mut metadata = self.execution_metadata.write().await;
        metadata.remove(&execution_id);

        let mut step_info = self.step_info.write().await;
        step_info.remove(&execution_id);
    }
}

impl Default for InMemoryStateStore {
    fn default() -> Self {
        Self::new()
    }
}

impl StateStore for InMemoryStateStore {
    fn put_blob(&self, data: ValueRef) -> BoxFuture<'_, error_stack::Result<BlobId, StateError>> {
        let blobs = self.blobs.clone();

        async move {
            let blob_id = BlobId::from_content(&data).change_context(StateError::Internal)?;

            // Store the data (overwrites are fine since content is identical)
            {
                let mut blobs = blobs.write().await;
                blobs.insert(blob_id.as_str().to_string(), data);
            }

            Ok(blob_id)
        }
        .boxed()
    }

    fn get_blob(
        &self,
        blob_id: &BlobId,
    ) -> BoxFuture<'_, error_stack::Result<ValueRef, StateError>> {
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

    fn record_step_result(
        &self,
        execution_id: Uuid,
        step_result: StepResult<'_>,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let executions = self.executions.clone();
        let step_idx = step_result.step_idx();
        let owned_step_result = step_result.to_owned();

        async move {
            let mut executions = executions.write().await;
            let execution_state = executions.entry(execution_id).or_default();

            // Ensure the vector has enough capacity
            execution_state.ensure_capacity(step_idx);

            // Update the step_id_to_index mapping for fast lookup by ID
            execution_state
                .step_id_to_index
                .insert(owned_step_result.step_id().to_string(), step_idx);

            // Store the result at the step index
            execution_state.step_results[step_idx] = Some(owned_step_result);

            Ok(())
        }
        .boxed()
    }

    fn get_step_result_by_index(
        &self,
        execution_id: Uuid,
        step_idx: usize,
    ) -> BoxFuture<'_, error_stack::Result<FlowResult, StateError>> {
        let executions = self.executions.clone();
        let execution_id_str = execution_id.to_string();

        async move {
            let executions = executions.read().await;
            let execution_state = executions.get(&execution_id).ok_or_else(|| {
                error_stack::report!(StateError::StepResultNotFoundByIndex {
                    execution_id: execution_id_str.clone(),
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
                        execution_id: execution_id_str,
                        step_idx,
                    })
                })
        }
        .boxed()
    }

    fn get_step_result_by_id(
        &self,
        execution_id: Uuid,
        step_id: &str,
    ) -> BoxFuture<'_, error_stack::Result<FlowResult, StateError>> {
        let executions = self.executions.clone();
        let step_id_owned = step_id.to_string();

        async move {
            let executions = executions.read().await;
            let execution_state = executions.get(&execution_id).ok_or_else(|| {
                error_stack::report!(StateError::StepResultNotFoundById {
                    execution_id,
                    step_id: step_id_owned.clone(),
                })
            })?;

            // Use O(1) HashMap lookup to find the step by ID
            match execution_state.step_id_to_index.get(&step_id_owned) {
                Some(&step_idx) => execution_state
                    .step_results
                    .get(step_idx)
                    .and_then(|opt| opt.as_ref())
                    .map(|step_result| step_result.result().clone())
                    .ok_or_else(|| {
                        error_stack::report!(StateError::StepResultNotFoundById {
                            execution_id,
                            step_id: step_id_owned,
                        })
                    }),
                None => Err(error_stack::report!(StateError::StepResultNotFoundById {
                    execution_id,
                    step_id: step_id_owned,
                })),
            }
        }
        .boxed()
    }

    fn list_step_results(
        &self,
        execution_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Vec<StepResult<'static>>, StateError>> {
        let executions = self.executions.clone();

        async move {
            let executions = executions.read().await;
            let execution_state = match executions.get(&execution_id) {
                Some(state) => state,
                None => return Ok(Vec::new()), // No execution found, return empty list
            };

            // Vec maintains natural ordering, so no sorting needed
            let results: Vec<StepResult<'static>> = execution_state
                .step_results
                .iter()
                .filter_map(|opt| opt.as_ref().map(|step_result| step_result.to_owned()))
                .collect();

            Ok(results)
        }
        .boxed()
    }

    // Workflow Management Methods

    fn store_workflow(
        &self,
        workflow: Arc<Flow>,
    ) -> BoxFuture<'_, error_stack::Result<FlowHash, StateError>> {
        let workflows = self.workflows.clone();
        let workflow_hash = Flow::hash(workflow.as_ref());

        async move {
            // Store the workflow directly (overwrites are fine since content is identical)
            {
                let mut workflows = workflows.write().await;
                workflows.insert(workflow_hash.to_string(), workflow.clone());
            }

            Ok(workflow_hash)
        }
        .boxed()
    }

    fn get_workflow(
        &self,
        workflow_hash: &FlowHash,
    ) -> BoxFuture<'_, error_stack::Result<Option<Arc<Flow>>, StateError>> {
        let workflows = self.workflows.clone();
        let hash = workflow_hash.to_string();

        async move {
            let workflows = workflows.read().await;
            let workflow = workflows.get(&hash).cloned();

            Ok(workflow)
        }
        .boxed()
    }

    fn get_workflows_by_name(
        &self,
        name: &str,
    ) -> BoxFuture<
        '_,
        error_stack::Result<Vec<(FlowHash, chrono::DateTime<chrono::Utc>)>, StateError>,
    > {
        let workflows = self.workflows.clone();
        let name = name.to_string();

        async move {
            let workflows = workflows.read().await;
            let mut results = Vec::new();

            for workflow in workflows.values() {
                if workflow.name.as_ref() == Some(&name) {
                    // Use current time for creation since we don't track it in in-memory
                    let created_at = chrono::Utc::now();
                    let hash = Flow::hash(workflow);
                    results.push((hash, created_at));
                }
            }

            // Sort by creation time (newest first)
            results.sort_by(|a, b| b.1.cmp(&a.1));
            Ok(results)
        }
        .boxed()
    }

    fn get_named_workflow(
        &self,
        name: &str,
        label: Option<&str>,
    ) -> BoxFuture<'_, error_stack::Result<Option<WorkflowWithMetadata>, StateError>> {
        let workflows = self.workflows.clone();
        let workflow_labels = self.workflow_labels.clone();
        let name = name.to_string();
        let label = label.map(|s| s.to_string());

        async move {
            match label {
                Some(label_str) => {
                    // Get workflow by label
                    let labels = workflow_labels.read().await;
                    let key = (name, label_str);

                    if let Some(label_metadata) = labels.get(&key) {
                        let workflows = workflows.read().await;
                        if let Some(workflow) =
                            workflows.get(&label_metadata.workflow_hash.to_string())
                        {
                            return Ok(Some(WorkflowWithMetadata {
                                workflow: workflow.clone(),
                                workflow_hash: label_metadata.workflow_hash.clone(),
                                created_at: label_metadata.created_at,
                                label_info: Some(label_metadata.clone()),
                            }));
                        }
                    }
                    Ok(None)
                }
                None => {
                    // Get latest workflow by name
                    let workflows = workflows.read().await;

                    for workflow in workflows.values() {
                        if workflow.name.as_ref() == Some(&name) {
                            let created_at = chrono::Utc::now();
                            let hash = Flow::hash(workflow);
                            return Ok(Some(WorkflowWithMetadata {
                                workflow: workflow.clone(),
                                workflow_hash: hash,
                                created_at,
                                label_info: None,
                            }));
                        }
                    }
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
        workflow_hash: FlowHash,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let workflow_labels = self.workflow_labels.clone();
        let name = name.to_string();
        let label = label.to_string();

        async move {
            let now = chrono::Utc::now();
            let workflow_label = WorkflowLabelMetadata {
                name: name.clone(),
                label: label.clone(),
                workflow_hash,
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
        let workflow_labels = self.workflow_labels.clone();
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

    fn list_workflow_names(&self) -> BoxFuture<'_, error_stack::Result<Vec<String>, StateError>> {
        let workflows = self.workflows.clone();

        async move {
            let workflows = workflows.read().await;
            let mut names = std::collections::HashSet::new();

            for workflow in workflows.values() {
                if let Some(name) = &workflow.name {
                    names.insert(name.clone());
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
        let workflow_labels = self.workflow_labels.clone();
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

    fn create_execution(
        &self,
        execution_id: Uuid,
        workflow_hash: FlowHash,
        workflow_name: Option<&str>,
        workflow_label: Option<&str>,
        debug_mode: bool,
        input: ValueRef,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let metadata = self.execution_metadata.clone();
        let now = chrono::Utc::now();
        let execution_details = ExecutionDetails {
            summary: ExecutionSummary {
                execution_id,
                workflow_hash,
                workflow_name: workflow_name.map(|s| s.to_string()),
                workflow_label: workflow_label.map(|s| s.to_string()),
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
            metadata.insert(execution_id, execution_details);
            Ok(())
        }
        .boxed()
    }

    fn update_execution_status(
        &self,
        execution_id: Uuid,
        status: ExecutionStatus,
        result: Option<ValueRef>,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        let metadata = self.execution_metadata.clone();

        async move {
            let mut metadata = metadata.write().await;
            if let Some(exec_metadata) = metadata.get_mut(&execution_id) {
                exec_metadata.summary.status = status;
                exec_metadata.result = result;

                if matches!(status, ExecutionStatus::Completed | ExecutionStatus::Failed) {
                    exec_metadata.summary.completed_at = Some(chrono::Utc::now());
                }
            }
            Ok(())
        }
        .boxed()
    }

    fn get_execution(
        &self,
        execution_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Option<ExecutionDetails>, StateError>> {
        let metadata = self.execution_metadata.clone();

        async move {
            let metadata = metadata.read().await;
            let exec_metadata = match metadata.get(&execution_id) {
                Some(metadata) => metadata,
                None => return Ok(None),
            };

            Ok(Some(exec_metadata.clone()))
        }
        .boxed()
    }

    fn list_executions(
        &self,
        filters: &ExecutionFilters,
    ) -> BoxFuture<'_, error_stack::Result<Vec<ExecutionSummary>, StateError>> {
        let metadata = self.execution_metadata.clone();
        let filters = filters.clone();

        async move {
            let metadata = metadata.read().await;
            let mut results: Vec<ExecutionSummary> = metadata
                .values()
                .filter(|exec| {
                    // Apply status filter
                    if let Some(ref status) = filters.status {
                        if &exec.summary.status != status {
                            return false;
                        }
                    }

                    // Apply workflow name filter
                    if let Some(ref workflow_name) = filters.workflow_name {
                        if exec.summary.workflow_name.as_ref() != Some(workflow_name) {
                            return false;
                        }
                    }

                    // Apply workflow label filter
                    if let Some(ref workflow_label) = filters.workflow_label {
                        if exec.summary.workflow_label.as_ref() != Some(workflow_label) {
                            return false;
                        }
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
        execution_id: uuid::Uuid,
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

            step_info_guard.insert(execution_id, execution_steps);
            Ok(())
        }
        .boxed()
    }

    fn update_step_status(
        &self,
        execution_id: uuid::Uuid,
        step_index: usize,
        status: stepflow_core::status::StepStatus,
    ) -> BoxFuture<'_, error_stack::Result<(), crate::StateError>> {
        let step_info_map = self.step_info.clone();

        async move {
            let mut step_info_guard = step_info_map.write().await;

            if let Some(execution_steps) = step_info_guard.get_mut(&execution_id) {
                if let Some(step_info) = execution_steps.get_mut(&step_index) {
                    step_info.status = status;
                    step_info.updated_at = chrono::Utc::now();
                }
            }

            Ok(())
        }
        .boxed()
    }

    fn get_step_info_for_execution(
        &self,
        execution_id: uuid::Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Vec<StepInfo>, crate::StateError>> {
        let step_info_map = self.step_info.clone();

        async move {
            let step_info_guard = step_info_map.read().await;

            let step_infos = step_info_guard
                .get(&execution_id)
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
        execution_id: uuid::Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Vec<StepInfo>, crate::StateError>> {
        let step_info_map = self.step_info.clone();

        async move {
            let step_info_guard = step_info_map.read().await;

            // Get all step info for this execution
            let execution_steps = step_info_guard
                .get(&execution_id)
                .cloned()
                .unwrap_or_default();

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
        let blob_id = store.put_blob(value_ref.clone()).await.unwrap();

        // Blob ID should be deterministic (SHA-256 hash)
        assert_eq!(blob_id.as_str().len(), 64); // SHA-256 produces 64 hex characters

        // Retrieve blob
        let retrieved = store.get_blob(&blob_id).await.unwrap();
        assert_eq!(retrieved.as_ref(), &test_data);

        // Same content should produce same blob ID
        let value_ref2 = ValueRef::new(test_data.clone());
        let blob_id2 = store.put_blob(value_ref2).await.unwrap();
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
        let store = InMemoryStateStore::new();
        let execution_id = Uuid::new_v4();

        // Test data
        let step1_result = FlowResult::Success {
            result: ValueRef::new(serde_json::json!({"output": "hello"})),
        };
        let step2_result = FlowResult::Skipped;

        // Record step results with both index and ID
        store
            .record_step_result(
                execution_id,
                StepResult::new(0, "step1", step1_result.clone()),
            )
            .await
            .unwrap();
        store
            .record_step_result(
                execution_id,
                StepResult::new(1, "step2", step2_result.clone()),
            )
            .await
            .unwrap();

        // Retrieve by index
        let retrieved_by_idx_0 = store
            .get_step_result_by_index(execution_id, 0)
            .await
            .unwrap();
        let retrieved_by_idx_1 = store
            .get_step_result_by_index(execution_id, 1)
            .await
            .unwrap();
        assert_eq!(retrieved_by_idx_0, step1_result);
        assert_eq!(retrieved_by_idx_1, step2_result);

        // Retrieve by ID
        let retrieved_by_id_1 = store
            .get_step_result_by_id(execution_id, "step1")
            .await
            .unwrap();
        let retrieved_by_id_2 = store
            .get_step_result_by_id(execution_id, "step2")
            .await
            .unwrap();
        assert_eq!(retrieved_by_id_1, step1_result);
        assert_eq!(retrieved_by_id_2, step2_result);

        // List all step results (should be ordered by index)
        let all_results = store.list_step_results(execution_id).await.unwrap();
        assert_eq!(all_results.len(), 2);
        assert_eq!(all_results[0], StepResult::new(0, "step1", step1_result));
        assert_eq!(all_results[1], StepResult::new(1, "step2", step2_result));

        // Non-existent step should return error
        let result_by_idx = store.get_step_result_by_index(execution_id, 99).await;
        assert!(result_by_idx.is_err());
        let result_by_id = store
            .get_step_result_by_id(execution_id, "nonexistent")
            .await;
        assert!(result_by_id.is_err());

        // Different execution ID should return empty list
        let other_execution_id = Uuid::new_v4();
        let other_results = store.list_step_results(other_execution_id).await.unwrap();
        assert!(other_results.is_empty());
    }

    #[tokio::test]
    async fn test_step_result_overwrite() {
        let store = InMemoryStateStore::new();
        let execution_id = Uuid::new_v4();

        // Record initial result
        let initial_result = FlowResult::Success {
            result: ValueRef::new(serde_json::json!({"attempt": 1})),
        };
        store
            .record_step_result(execution_id, StepResult::new(0, "step1", initial_result))
            .await
            .unwrap();

        // Overwrite with new result
        let new_result = FlowResult::Success {
            result: ValueRef::new(serde_json::json!({"attempt": 2})),
        };
        store
            .record_step_result(
                execution_id,
                StepResult::new(0, "step1", new_result.clone()),
            )
            .await
            .unwrap();

        // Should retrieve the new result by both index and ID
        let retrieved_by_idx = store
            .get_step_result_by_index(execution_id, 0)
            .await
            .unwrap();
        let retrieved_by_id = store
            .get_step_result_by_id(execution_id, "step1")
            .await
            .unwrap();
        assert_eq!(retrieved_by_idx, new_result);
        assert_eq!(retrieved_by_id, new_result);

        // Should still only have one entry for this step
        let all_results = store.list_step_results(execution_id).await.unwrap();
        assert_eq!(all_results.len(), 1);
        assert_eq!(all_results[0], StepResult::new(0, "step1", new_result));
    }

    #[tokio::test]
    async fn test_step_result_ordering() {
        let store = InMemoryStateStore::new();
        let execution_id = Uuid::new_v4();

        // Insert steps out of order to test ordering
        let step2_result = FlowResult::Success {
            result: ValueRef::new(serde_json::json!({"step": 2})),
        };
        let step0_result = FlowResult::Success {
            result: ValueRef::new(serde_json::json!({"step": 0})),
        };
        let step1_result = FlowResult::Skipped;

        // Record in non-sequential order
        store
            .record_step_result(
                execution_id,
                StepResult::new(2, "step2", step2_result.clone()),
            )
            .await
            .unwrap();
        store
            .record_step_result(
                execution_id,
                StepResult::new(0, "step0", step0_result.clone()),
            )
            .await
            .unwrap();
        store
            .record_step_result(
                execution_id,
                StepResult::new(1, "step1", step1_result.clone()),
            )
            .await
            .unwrap();

        // List should return results ordered by step index
        let all_results = store.list_step_results(execution_id).await.unwrap();
        assert_eq!(all_results.len(), 3);
        assert_eq!(all_results[0], StepResult::new(0, "step0", step0_result));
        assert_eq!(all_results[1], StepResult::new(1, "step1", step1_result));
        assert_eq!(all_results[2], StepResult::new(2, "step2", step2_result));
    }

    #[tokio::test]
    async fn test_execution_eviction() {
        let store = InMemoryStateStore::new();
        let execution_id = Uuid::new_v4();

        // Store some step results
        let step_result = FlowResult::Success {
            result: ValueRef::new(serde_json::json!({"output": "test"})),
        };
        store
            .record_step_result(
                execution_id,
                StepResult::new(0, "step1", step_result.clone()),
            )
            .await
            .unwrap();

        // Verify the result exists
        let retrieved = store
            .get_step_result_by_index(execution_id, 0)
            .await
            .unwrap();
        assert_eq!(retrieved, step_result);

        // Evict the execution
        store.evict_execution(execution_id).await;

        // Verify the result no longer exists
        let result = store.get_step_result_by_index(execution_id, 0).await;
        assert!(result.is_err());

        // Verify list returns empty
        let all_results = store.list_step_results(execution_id).await.unwrap();
        assert!(all_results.is_empty());
    }
}
