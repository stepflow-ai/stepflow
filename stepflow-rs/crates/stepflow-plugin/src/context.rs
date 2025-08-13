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

use futures::future::{BoxFuture, FutureExt as _};
use std::path::Path;
use std::sync::Arc;
use stepflow_core::{
    BlobId, FlowResult,
    workflow::{Flow, ValueRef},
};
use stepflow_state::StateStore;
use uuid::Uuid;

/// Trait for interacting with the workflow runtime.
pub trait Context: Send + Sync {
    /// Submits a nested workflow for execution and returns its execution ID.
    ///
    /// Implementation should use Arc::clone on self if it needs to pass ownership
    /// to spawned tasks or other async contexts.
    fn submit_flow(
        &self,
        flow: Arc<Flow>,
        flow_id: BlobId,
        input: ValueRef,
    ) -> BoxFuture<'_, crate::Result<Uuid>>;

    /// Retrieves the result of a previously submitted workflow.
    fn flow_result(&self, run_id: Uuid) -> BoxFuture<'_, crate::Result<FlowResult>>;

    /// Executes a nested workflow and waits for its completion.
    fn execute_flow(
        &self,
        flow: Arc<Flow>,
        flow_id: BlobId,
        input: ValueRef,
    ) -> BoxFuture<'_, crate::Result<FlowResult>> {
        async move {
            let run_id = self.submit_flow(flow, flow_id, input).await?;
            self.flow_result(run_id).await
        }
        .boxed()
    }

    /// Executes a flow by blob ID - combines blob retrieval, deserialization, and execution.
    ///
    /// This is a convenience method that handles the common pattern of:
    /// 1. Retrieving a flow blob by ID from the state store
    /// 2. Deserializing the flow from blob data
    /// 3. Executing the flow with given input
    ///
    /// # Arguments
    /// * `flow_id` - The blob ID of the flow to execute
    /// * `input` - The input data for the flow
    ///
    /// # Returns
    /// The result of the flow execution
    fn execute_flow_by_id(
        &self,
        flow_id: &BlobId,
        input: ValueRef,
    ) -> BoxFuture<'_, crate::Result<FlowResult>> {
        let flow_id = flow_id.clone();
        async move {
            use error_stack::ResultExt as _;

            // Retrieve the flow from the blob store
            let blob_data = self
                .state_store()
                .get_blob(&flow_id)
                .await
                .change_context(crate::PluginError::Execution)?;

            // Deserialize the flow from blob data
            let flow = blob_data
                .as_flow()
                .ok_or_else(|| error_stack::report!(crate::PluginError::Execution))?
                .clone();

            // Execute the flow
            self.execute_flow(flow, flow_id, input).await
        }
        .boxed()
    }

    /// Get the state store for this executor.
    fn state_store(&self) -> &Arc<dyn StateStore>;

    /// Working directory of the Stepflow Config.
    fn working_directory(&self) -> &Path;
}

/// Execution context that combines a Context with an execution ID.
#[derive(Clone)]
pub struct ExecutionContext {
    context: Arc<dyn Context>,
    run_id: Uuid,
}

impl ExecutionContext {
    /// Create a new ExecutionContext.
    pub fn new(context: Arc<dyn Context>, run_id: Uuid) -> Self {
        Self { context, run_id }
    }

    /// Get the execution ID for this context.
    pub fn run_id(&self) -> Uuid {
        self.run_id
    }

    /// Get a reference to the state store.
    pub fn state_store(&self) -> &Arc<dyn StateStore> {
        self.context.state_store()
    }
}

impl Context for ExecutionContext {
    fn state_store(&self) -> &Arc<dyn StateStore> {
        self.context.state_store()
    }

    /// Submit a nested workflow for execution.
    fn submit_flow(
        &self,
        flow: Arc<Flow>,
        flow_id: BlobId,
        input: ValueRef,
    ) -> BoxFuture<'_, crate::Result<Uuid>> {
        self.context.submit_flow(flow, flow_id, input)
    }

    /// Get the result of a workflow execution.
    fn flow_result(&self, run_id: Uuid) -> BoxFuture<'_, crate::Result<FlowResult>> {
        self.context.flow_result(run_id)
    }

    /// Execute a nested workflow and wait for completion.
    fn execute_flow(
        &self,
        flow: Arc<Flow>,
        flow_id: BlobId,
        input: ValueRef,
    ) -> BoxFuture<'_, crate::Result<FlowResult>> {
        self.context.execute_flow(flow, flow_id, input)
    }

    fn working_directory(&self) -> &Path {
        self.context.working_directory()
    }
}
