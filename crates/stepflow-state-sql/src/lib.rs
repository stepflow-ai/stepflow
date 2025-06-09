//! SQL-based StateStore implementation using sqlx
//!
//! Provides persistent storage for blobs and execution state using SQLite.
//! Future versions will support PostgreSQL and MySQL.

mod migrations;
mod sqlite_state_store;

pub use sqlite_state_store::{SqliteStateStore, SqliteStateStoreConfig};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use stepflow_core::{workflow::ValueRef, FlowResult};
    use stepflow_state::{StateStore, StepResult};
    use uuid::Uuid;

    #[tokio::test]
    async fn test_blob_storage() {
        let store = SqliteStateStore::in_memory().await.unwrap();

        // Create test data
        let test_data = json!({"hello": "world", "number": 42});
        let value_ref = ValueRef::new(test_data.clone());

        // Store blob
        let blob_id = store.put_blob(value_ref.clone()).await.unwrap();

        // Retrieve blob
        let retrieved = store.get_blob(&blob_id).await.unwrap();

        assert_eq!(retrieved.as_ref(), &test_data);
    }

    #[tokio::test]
    async fn test_step_result_storage() {
        let store = SqliteStateStore::in_memory().await.unwrap();
        let execution_id = Uuid::new_v4();

        // Create test step result
        let flow_result = FlowResult::Success {
            result: ValueRef::new(json!({"result": "success"})),
        };
        let step_result = StepResult::new(0, "test_step", flow_result.clone());

        // Store step result
        store
            .record_step_result(execution_id, step_result)
            .await
            .unwrap();

        // Retrieve by index
        let retrieved_by_index = store
            .get_step_result_by_index(execution_id, 0)
            .await
            .unwrap();
        assert_eq!(retrieved_by_index, flow_result);

        // Retrieve by ID
        let retrieved_by_id = store
            .get_step_result_by_id(execution_id, "test_step")
            .await
            .unwrap();
        assert_eq!(retrieved_by_id, flow_result);

        // List all results
        let all_results = store.list_step_results(execution_id).await.unwrap();
        assert_eq!(all_results.len(), 1);
        assert_eq!(all_results[0].step_idx(), 0);
        assert_eq!(all_results[0].step_id(), "test_step");
        assert_eq!(*all_results[0].result(), flow_result);
    }

    #[tokio::test]
    async fn test_blob_deduplication() {
        let store = SqliteStateStore::in_memory().await.unwrap();

        let test_data = json!({"test": "data"});
        let value_ref = ValueRef::new(test_data);

        // Store the same data twice
        let blob_id1 = store.put_blob(value_ref.clone()).await.unwrap();
        let blob_id2 = store.put_blob(value_ref).await.unwrap();

        // Should get the same ID (deduplication)
        assert_eq!(blob_id1, blob_id2);
    }
}
