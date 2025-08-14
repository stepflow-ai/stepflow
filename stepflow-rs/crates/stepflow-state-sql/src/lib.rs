// Licensed to the Apache Software Foundation (ASF) under one or more contributor license agreements.
// See the NOTICE file distributed with this work for additional information regarding copyright
// ownership.  The ASF licenses this file to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance with the License.  You may obtain a
// copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied.  See the License for the specific language governing permissions and limitations under
// the License.

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
    use stepflow_core::values::ValueTemplate;
    use stepflow_core::workflow::FlowV1;
    use stepflow_core::{BlobType, FlowResult, workflow::ValueRef};
    use stepflow_state::{StateStore as _, StepResult};
    use uuid::Uuid;

    #[tokio::test]
    async fn test_blob_storage() {
        let store = SqliteStateStore::in_memory().await.unwrap();

        // Create test data
        let test_data = json!({"hello": "world", "number": 42});
        let value_ref = ValueRef::new(test_data.clone());

        // Store blob
        let blob_id = store
            .put_blob(value_ref.clone(), stepflow_core::BlobType::Data)
            .await
            .unwrap();

        // Retrieve blob
        let retrieved = store.get_blob(&blob_id).await.unwrap();

        assert_eq!(retrieved.data().as_ref(), &test_data);
    }

    #[tokio::test]
    async fn test_step_result_storage() {
        let store = SqliteStateStore::in_memory().await.unwrap();
        let run_id = Uuid::new_v4();

        // First store a workflow
        let flow = stepflow_core::workflow::Flow::V1(FlowV1 {
            name: None,
            description: None,
            input_schema: None,
            output_schema: None,
            output: ValueTemplate::empty_object(),
            steps: vec![stepflow_core::workflow::Step {
                id: "test_step".to_string(),
                component: stepflow_core::workflow::Component::from_string("/test/mock"),
                input: ValueTemplate::empty_object(),
                input_schema: None,
                output_schema: None,
                skip_if: None,
                on_error: stepflow_core::workflow::ErrorAction::Fail,
                metadata: std::collections::HashMap::new(),
            }],
            version: None,
            test: None,
            examples: None,
            metadata: std::collections::HashMap::new(),
        });
        let flow_arc = std::sync::Arc::new(flow);
        let flow_data = ValueRef::new(serde_json::to_value(flow_arc.as_ref()).unwrap());
        let flow_id = store.put_blob(flow_data, BlobType::Flow).await.unwrap();

        // Then create the execution
        store
            .create_run(
                run_id,
                flow_id,
                None,                     // flow_name
                None,                     // flow_label
                false,                    // debug_mode
                ValueRef::new(json!({})), // input
            )
            .await
            .unwrap();

        // Create test step result
        let flow_result = FlowResult::Success(ValueRef::new(json!({"result":"success"})));
        let step_result = StepResult::new(0, "test_step", flow_result.clone());

        // Store step result
        store
            .queue_write(stepflow_state::StateWriteOperation::RecordStepResult {
                run_id,
                step_result,
            })
            .unwrap();

        // Flush to ensure the write is persisted
        store.flush_pending_writes(run_id).await.unwrap();

        // Retrieve by index
        let retrieved_by_index = store.get_step_result(run_id, 0).await.unwrap();
        assert_eq!(retrieved_by_index, flow_result);

        // List all results
        let all_results = store.list_step_results(run_id).await.unwrap();
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
        let blob_id1 = store
            .put_blob(value_ref.clone(), stepflow_core::BlobType::Data)
            .await
            .unwrap();
        let blob_id2 = store
            .put_blob(value_ref, stepflow_core::BlobType::Data)
            .await
            .unwrap();

        // Should get the same ID (deduplication)
        assert_eq!(blob_id1, blob_id2);
    }
}
