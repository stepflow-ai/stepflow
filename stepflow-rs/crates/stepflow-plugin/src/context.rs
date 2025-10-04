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

    /// Submit a batch execution and return the batch ID immediately.
    ///
    /// This method creates a batch execution with multiple inputs and returns
    /// a batch ID that can be used to query status and results.
    ///
    /// # Arguments
    /// * `flow` - The workflow to execute
    /// * `flow_id` - ID of the workflow
    /// * `inputs` - Vector of input values, one for each run in the batch
    /// * `max_concurrency` - Optional maximum number of concurrent executions
    ///
    /// # Returns
    /// A unique batch ID for the submitted batch
    fn submit_batch(
        &self,
        flow: Arc<Flow>,
        flow_id: BlobId,
        inputs: Vec<ValueRef>,
        max_concurrency: Option<usize>,
    ) -> BoxFuture<'_, crate::Result<uuid::Uuid>>;

    /// Get batch status and optionally results, with optional waiting.
    ///
    /// # Arguments
    /// * `batch_id` - The batch ID to query
    /// * `wait` - If true, wait for batch completion before returning
    /// * `include_results` - If true, include full outputs in response
    ///
    /// # Returns
    /// Tuple of (batch details, optional outputs)
    fn get_batch(
        &self,
        batch_id: uuid::Uuid,
        wait: bool,
        include_results: bool,
    ) -> BoxFuture<
        '_,
        crate::Result<(
            stepflow_state::BatchDetails,
            Option<Vec<stepflow_state::BatchOutputInfo>>,
        )>,
    >;

    /// Convenience method: submit batch, wait for completion, and return results.
    ///
    /// This is equivalent to calling submit_batch followed by get_batch with wait=true and include_results=true.
    fn execute_batch(
        &self,
        flow: Arc<Flow>,
        flow_id: BlobId,
        inputs: Vec<ValueRef>,
        max_concurrency: Option<usize>,
    ) -> BoxFuture<'_, crate::Result<Vec<FlowResult>>> {
        async move {
            let batch_id = self
                .submit_batch(flow, flow_id, inputs, max_concurrency)
                .await?;
            let (_details, outputs) = self.get_batch(batch_id, true, true).await?;
            let outputs = outputs.expect("include_results=true should return outputs");

            // Extract FlowResult from each BatchOutputInfo
            let results: Vec<FlowResult> = outputs
                .into_iter()
                .map(|output_info| {
                    output_info.result.unwrap_or_else(|| {
                        // If no result, create a failed result based on status
                        FlowResult::Failed(stepflow_core::FlowError::new(
                            500,
                            format!(
                                "Run at index {} has no result (status: {:?})",
                                output_info.batch_input_index, output_info.status
                            ),
                        ))
                    })
                })
                .collect();

            Ok(results)
        }
        .boxed()
    }
}

/// Execution context that combines a Context with an execution ID.
#[derive(Clone)]
pub struct ExecutionContext {
    context: Arc<dyn Context>,
    run_id: Uuid,
    step_id: Option<String>,
    flow: Option<Arc<Flow>>,
    flow_id: Option<BlobId>,
}

impl ExecutionContext {
    /// Create a new ExecutionContext.
    pub fn new(context: Arc<dyn Context>, run_id: Uuid, step_id: Option<String>) -> Self {
        Self {
            context,
            run_id,
            step_id,
            flow: None,
            flow_id: None,
        }
    }

    /// Create a new ExecutionContext with a flow for metadata access.
    pub fn new_with_flow(
        context: Arc<dyn Context>,
        run_id: Uuid,
        step_id: Option<String>,
        flow: Arc<Flow>,
        flow_id: BlobId,
    ) -> Self {
        Self {
            context,
            run_id,
            step_id,
            flow: Some(flow),
            flow_id: Some(flow_id),
        }
    }

    /// Create a new ExecutionContext for a specific step.
    pub fn for_step(context: Arc<dyn Context>, run_id: Uuid, step_id: String) -> Self {
        Self {
            context,
            run_id,
            step_id: Some(step_id),
            flow: None,
            flow_id: None,
        }
    }

    /// Create a new ExecutionContext for a specific step with flow metadata access.
    pub fn for_step_with_flow(
        context: Arc<dyn Context>,
        run_id: Uuid,
        step_id: String,
        flow: Arc<Flow>,
        flow_id: BlobId,
    ) -> Self {
        Self {
            context,
            run_id,
            step_id: Some(step_id),
            flow: Some(flow),
            flow_id: Some(flow_id),
        }
    }

    /// Create a new ExecutionContext for workflow-level operations (no specific step).
    pub fn for_workflow(context: Arc<dyn Context>, run_id: Uuid) -> Self {
        Self {
            context,
            run_id,
            step_id: None,
            flow: None,
            flow_id: None,
        }
    }

    /// Create a new ExecutionContext for workflow-level operations with flow metadata access.
    pub fn for_workflow_with_flow(
        context: Arc<dyn Context>,
        run_id: Uuid,
        flow: Arc<Flow>,
        flow_id: BlobId,
    ) -> Self {
        Self {
            context,
            run_id,
            step_id: None,
            flow: Some(flow),
            flow_id: Some(flow_id),
        }
    }

    /// Create a new ExecutionContext with a different step ID, reusing the same context and run_id.
    pub fn with_step(&self, step_id: String) -> Self {
        Self {
            context: self.context.clone(),
            run_id: self.run_id,
            step_id: Some(step_id),
            flow: self.flow.clone(),
            flow_id: self.flow_id.clone(),
        }
    }

    /// Get the execution ID for this context.
    pub fn run_id(&self) -> Uuid {
        self.run_id
    }

    /// Get the step ID for this context, if available.
    pub fn step_id(&self) -> Option<&str> {
        self.step_id.as_deref()
    }

    /// Get the flow ID for this context, if available.
    pub fn flow_id(&self) -> Option<&BlobId> {
        self.flow_id.as_ref()
    }

    /// Get a reference to the state store.
    pub fn state_store(&self) -> &Arc<dyn StateStore> {
        self.context.state_store()
    }

    /// Get the underlying context.
    pub fn context(&self) -> &Arc<dyn Context> {
        &self.context
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

    fn submit_batch(
        &self,
        flow: Arc<Flow>,
        flow_id: BlobId,
        inputs: Vec<ValueRef>,
        max_concurrency: Option<usize>,
    ) -> BoxFuture<'_, crate::Result<uuid::Uuid>> {
        self.context
            .submit_batch(flow, flow_id, inputs, max_concurrency)
    }

    fn get_batch(
        &self,
        batch_id: uuid::Uuid,
        wait: bool,
        include_results: bool,
    ) -> BoxFuture<
        '_,
        crate::Result<(
            stepflow_state::BatchDetails,
            Option<Vec<stepflow_state::BatchOutputInfo>>,
        )>,
    > {
        self.context.get_batch(batch_id, wait, include_results)
    }
}
