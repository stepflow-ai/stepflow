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

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::{pin::Pin, sync::Arc};
use stepflow_core::{BlobId, FlowResult, workflow::ValueRef};
use stepflow_plugin::ExecutionContext;
use stepflow_state::InMemoryStateStore;
use uuid::Uuid;

/// A mock execution context for testing built-in components.
///
/// This provides a way to create ExecutionContext instances for testing
/// built-in components without requiring complex workflow execution.
pub struct MockContext {
    executor: Arc<MockExecutor>,
}

impl MockContext {
    /// Create a new mock context.
    pub fn new() -> Self {
        Self {
            executor: Arc::new(MockExecutor {
                state_store: Arc::new(InMemoryStateStore::new()),
                batches: Arc::new(Mutex::new(HashMap::new())),
            }),
        }
    }

    /// Get an execution context for testing from this mock context.
    pub fn execution_context(&self) -> ExecutionContext {
        ExecutionContext::new(
            self.executor.clone(),
            Uuid::now_v7(),
            Some("test_step".to_string()),
        )
    }
}

impl Default for MockContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Mock batch data stored for testing
#[derive(Clone)]
struct MockBatch {
    flow_id: BlobId,
    inputs: Vec<ValueRef>,
}

/// Mock executor implementation for testing.
struct MockExecutor {
    state_store: Arc<dyn stepflow_state::StateStore>,
    batches: Arc<Mutex<HashMap<Uuid, MockBatch>>>,
}

impl stepflow_plugin::Context for MockExecutor {
    fn submit_flow(
        &self,
        _params: stepflow_core::SubmitFlowParams,
    ) -> Pin<Box<dyn std::future::Future<Output = stepflow_plugin::Result<Uuid>> + Send + '_>> {
        Box::pin(async { Ok(Uuid::now_v7()) })
    }

    fn flow_result(
        &self,
        _run_id: Uuid,
    ) -> Pin<Box<dyn std::future::Future<Output = stepflow_plugin::Result<FlowResult>> + Send + '_>>
    {
        Box::pin(async {
            let result = serde_json::json!({"message": "Hello from nested flow"});
            Ok(FlowResult::Success(ValueRef::new(result)))
        })
    }

    fn state_store(&self) -> &Arc<dyn stepflow_state::StateStore> {
        &self.state_store
    }

    fn working_directory(&self) -> &std::path::Path {
        Path::new(".")
    }

    fn submit_batch(
        &self,
        params: stepflow_core::SubmitBatchParams,
    ) -> Pin<Box<dyn std::future::Future<Output = stepflow_plugin::Result<Uuid>> + Send + '_>> {
        let batches = self.batches.clone();
        Box::pin(async move {
            let batch_id = Uuid::now_v7();
            batches.lock().unwrap().insert(
                batch_id,
                MockBatch {
                    flow_id: params.flow_id,
                    inputs: params.inputs,
                },
            );
            Ok(batch_id)
        })
    }

    fn get_batch(
        &self,
        batch_id: Uuid,
        _wait: bool,
        include_results: bool,
    ) -> Pin<
        Box<
            dyn std::future::Future<
                    Output = stepflow_plugin::Result<(
                        stepflow_state::BatchDetails,
                        Option<Vec<stepflow_state::BatchOutputInfo>>,
                    )>,
                > + Send
                + '_,
        >,
    > {
        let batches = self.batches.clone();
        Box::pin(async move {
            // Get the batch from our mock storage
            let batch = batches
                .lock()
                .unwrap()
                .get(&batch_id)
                .cloned()
                .ok_or_else(|| {
                    error_stack::report!(stepflow_plugin::PluginError::Execution)
                        .attach_printable("Batch not found")
                })?;

            let num_inputs = batch.inputs.len();

            // Use a timestamp we can construct without chrono dependency
            // BatchDetails expects chrono types, so we construct them via serialization
            use stepflow_state::{BatchDetails, BatchMetadata, BatchStatistics, BatchStatus};

            let batch_details = BatchDetails {
                metadata: BatchMetadata {
                    batch_id,
                    flow_id: batch.flow_id.clone(),
                    flow_name: None,
                    total_inputs: num_inputs,
                    status: BatchStatus::Running,
                    // Use a mock timestamp - we're in test code
                    created_at: serde_json::from_str("\"2024-01-01T00:00:00Z\"").unwrap(),
                },
                statistics: BatchStatistics {
                    completed_runs: num_inputs,
                    running_runs: 0,
                    failed_runs: 0,
                    cancelled_runs: 0,
                    paused_runs: 0,
                },
                completed_at: Some(serde_json::from_str("\"2024-01-01T00:00:00Z\"").unwrap()),
            };

            let outputs = if include_results {
                // Create mock successful results for each input
                Some(
                    (0..num_inputs)
                        .map(|i| stepflow_state::BatchOutputInfo {
                            batch_input_index: i,
                            status: stepflow_core::status::ExecutionStatus::Completed,
                            result: Some(FlowResult::Success(
                                serde_json::json!({"doubled": 42}).into(),
                            )),
                        })
                        .collect(),
                )
            } else {
                None
            };

            Ok((batch_details, outputs))
        })
    }
}
