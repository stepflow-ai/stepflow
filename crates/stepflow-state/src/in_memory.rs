use error_stack::ResultExt as _;
use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};

use crate::{StateStore, state_store::StepResult};
use stepflow_core::{FlowResult, blob::BlobId, workflow::ValueRef};
use uuid::Uuid;

use crate::StateError;
use tokio::sync::RwLock;
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
}

impl InMemoryStateStore {
    /// Create a new in-memory state store.
    pub fn new() -> Self {
        Self {
            blobs: Arc::new(RwLock::new(HashMap::new())),
            executions: Arc::new(RwLock::new(HashMap::new())),
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
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<BlobId, StateError>> + Send + '_>> {
        let blobs = self.blobs.clone();

        Box::pin(async move {
            let blob_id = BlobId::from_content(&data).change_context(StateError::Internal)?;

            // Store the data (overwrites are fine since content is identical)
            {
                let mut blobs = blobs.write().await;
                blobs.insert(blob_id.as_str().to_string(), data);
            }

            Ok(blob_id)
        })
    }

    fn get_blob(
        &self,
        blob_id: &BlobId,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<ValueRef, StateError>> + Send + '_>> {
        let blobs = self.blobs.clone();
        let blob_id_str = blob_id.as_str().to_string();

        Box::pin(async move {
            let blobs = blobs.read().await;
            blobs.get(&blob_id_str).cloned().ok_or_else(|| {
                error_stack::report!(StateError::BlobNotFound {
                    blob_id: blob_id_str.clone()
                })
            })
        })
    }

    fn record_step_result(
        &self,
        execution_id: Uuid,
        step_result: StepResult<'_>,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<(), StateError>> + Send + '_>> {
        let executions = self.executions.clone();
        let step_idx = step_result.step_idx();
        let owned_step_result = step_result.to_owned();

        Box::pin(async move {
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
        })
    }

    fn get_step_result_by_index(
        &self,
        execution_id: Uuid,
        step_idx: usize,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<FlowResult, StateError>> + Send + '_>>
    {
        let executions = self.executions.clone();
        let execution_id_str = execution_id.to_string();

        Box::pin(async move {
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
        })
    }

    fn get_step_result_by_id(
        &self,
        execution_id: Uuid,
        step_id: &str,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<FlowResult, StateError>> + Send + '_>>
    {
        let executions = self.executions.clone();
        let step_id_owned = step_id.to_string();

        Box::pin(async move {
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
        })
    }

    fn list_step_results(
        &self,
        execution_id: Uuid,
    ) -> Pin<
        Box<
            dyn Future<Output = error_stack::Result<Vec<StepResult<'static>>, StateError>>
                + Send
                + '_,
        >,
    > {
        let executions = self.executions.clone();

        Box::pin(async move {
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
        })
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
