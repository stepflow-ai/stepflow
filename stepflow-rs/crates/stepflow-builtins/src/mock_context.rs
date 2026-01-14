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

use std::sync::Arc;
use stepflow_core::{FlowResult, status::ExecutionStatus, workflow::ValueRef};
use stepflow_plugin::{
    ExecutionContext, RunContext, StepflowEnvironment, StepflowEnvironmentBuilder, subflow_channel,
};
use stepflow_state::{CreateRunParams, StateStoreExt as _};
use uuid::Uuid;

/// A mock execution context for testing built-in components.
///
/// This provides a way to create ExecutionContext instances for testing
/// built-in components without requiring complex workflow execution.
/// It includes a mock subflow submitter that returns mock results.
pub struct MockContext {
    env: Arc<StepflowEnvironment>,
}

impl MockContext {
    /// Create a new mock context with subflow support.
    pub async fn new() -> Self {
        let env = StepflowEnvironmentBuilder::build_in_memory()
            .await
            .expect("In-memory environment should always initialize successfully");
        Self { env }
    }

    /// Get an execution context for testing from this mock context.
    ///
    /// This creates a new execution context with a subflow submitter that
    /// returns mock results immediately.
    pub fn execution_context(&self) -> ExecutionContext {
        // Create run ID for this context
        let run_id = Uuid::now_v7();

        // Create a channel for this specific execution context
        let (submitter, mut receiver) = subflow_channel(10, run_id);

        // Clone what we need for the spawned task
        let state_store = self.env.state_store().clone();

        // Spawn a task that handles subflow requests with mock results
        tokio::spawn(async move {
            while let Some(request) = receiver.recv().await {
                let subflow_run_id = Uuid::now_v7();
                let input_count = request.inputs.len();

                // Create the run in state store
                let create_params =
                    CreateRunParams::new(subflow_run_id, request.flow_id, request.inputs);
                let _ = state_store.create_run(create_params).await;

                // Record mock results for each item
                for item_index in 0..input_count {
                    let mock_result = FlowResult::Success(ValueRef::new(
                        serde_json::json!({"message": "Hello from nested flow"}),
                    ));
                    let _ = state_store
                        .record_item_result(subflow_run_id, item_index, mock_result)
                        .await;
                }

                // Update status to completed (this triggers state store notification)
                let _ = state_store
                    .update_run_status(subflow_run_id, ExecutionStatus::Completed)
                    .await;

                // Send back the run_id (caller will use state_store.wait_for_completion)
                let _ = request.response_tx.send(subflow_run_id);
            }
        });

        // Create run context with the submitter using the builder pattern
        let run_context = Arc::new(RunContext::for_root(run_id).with_submitter(submitter));

        ExecutionContext::for_testing(self.env.clone(), run_context)
    }
}
