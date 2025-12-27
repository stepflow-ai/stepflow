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
use std::sync::RwLock;
use std::{pin::Pin, sync::Arc};
use stepflow_core::status::ExecutionStatus;
use stepflow_core::{FlowResult, GetRunOptions, SubmitRunParams, workflow::ValueRef};
use stepflow_plugin::ExecutionContext;
use stepflow_state::{InMemoryStateStore, ItemStatistics, RunStatus};
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
                batch_inputs: RwLock::new(HashMap::new()),
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

/// Mock executor implementation for testing.
struct MockExecutor {
    state_store: Arc<dyn stepflow_state::StateStore>,
    /// Track batch inputs so get_batch can return mock results
    batch_inputs: RwLock<HashMap<Uuid, usize>>,
}

impl stepflow_plugin::Context for MockExecutor {
    fn submit_run(
        &self,
        params: SubmitRunParams,
    ) -> Pin<Box<dyn std::future::Future<Output = stepflow_plugin::Result<RunStatus>> + Send + '_>>
    {
        let run_id = Uuid::now_v7();
        let input_count = params.item_count();
        let wait = params.wait;
        self.batch_inputs
            .write()
            .unwrap()
            .insert(run_id, input_count);

        Box::pin(async move {
            let flow_id = stepflow_core::BlobId::new(
                "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            )
            .expect("mock blob id");
            let now = chrono::Utc::now();

            // If wait=true, return completed status with mock results
            let (status, completed_at, results) = if wait {
                let mock_results = (0..input_count)
                    .map(|i| stepflow_state::ItemResult {
                        item_index: i,
                        status: ExecutionStatus::Completed,
                        result: Some(FlowResult::Success(ValueRef::new(
                            serde_json::json!({"message": "Hello from nested flow"}),
                        ))),
                    })
                    .collect();
                (ExecutionStatus::Completed, Some(now), Some(mock_results))
            } else {
                (ExecutionStatus::Running, None, None)
            };

            Ok(RunStatus {
                run_id,
                flow_id,
                flow_name: None,
                flow_label: None,
                status,
                items: ItemStatistics {
                    total: input_count,
                    completed: if wait { input_count } else { 0 },
                    running: if wait { 0 } else { input_count },
                    failed: 0,
                    cancelled: 0,
                },
                created_at: now,
                completed_at,
                results,
            })
        })
    }

    fn get_run(
        &self,
        run_id: Uuid,
        options: GetRunOptions,
    ) -> Pin<Box<dyn std::future::Future<Output = stepflow_plugin::Result<RunStatus>> + Send + '_>>
    {
        let input_count = self.batch_inputs.read().unwrap().get(&run_id).copied();

        Box::pin(async move {
            let count = input_count.unwrap_or(1);
            let flow_id = stepflow_core::BlobId::new(
                "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            )
            .expect("mock blob id");
            let now = chrono::Utc::now();

            let results = if options.include_results {
                Some(
                    (0..count)
                        .map(|i| stepflow_state::ItemResult {
                            item_index: i,
                            status: ExecutionStatus::Completed,
                            result: Some(FlowResult::Success(ValueRef::new(
                                serde_json::json!({"message": "Hello from nested flow"}),
                            ))),
                        })
                        .collect(),
                )
            } else {
                None
            };

            Ok(RunStatus {
                run_id,
                flow_id,
                flow_name: None,
                flow_label: None,
                status: ExecutionStatus::Completed,
                items: ItemStatistics {
                    total: count,
                    completed: count,
                    running: 0,
                    failed: 0,
                    cancelled: 0,
                },
                created_at: now,
                completed_at: Some(now),
                results,
            })
        })
    }

    fn state_store(&self) -> &Arc<dyn stepflow_state::StateStore> {
        &self.state_store
    }

    fn working_directory(&self) -> &std::path::Path {
        Path::new(".")
    }
}
