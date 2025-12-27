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
    BlobId, FlowResult, GetRunOptions, SubmitRunParams,
    workflow::{Flow, ValueRef, WorkflowOverrides},
};
use stepflow_state::{RunStatus, StateStore};
use uuid::Uuid;

/// Trait for interacting with the workflow runtime.
pub trait Context: Send + Sync {
    /// Submit a run with 1 or N items.
    ///
    /// If `params.wait` is false, returns immediately with status=Running.
    /// If `params.wait` is true, blocks until completion and returns final status.
    fn submit_run(&self, params: SubmitRunParams) -> BoxFuture<'_, crate::Result<RunStatus>>;

    /// Get run status and optionally results.
    ///
    /// If `options.wait` is true, blocks until all items complete.
    /// If `options.include_results` is true, includes item results in the response.
    fn get_run(
        &self,
        run_id: Uuid,
        options: GetRunOptions,
    ) -> BoxFuture<'_, crate::Result<RunStatus>>;

    /// Get the state store for this executor.
    fn state_store(&self) -> &Arc<dyn StateStore>;

    /// Working directory of the Stepflow Config.
    fn working_directory(&self) -> &Path;

    // ========================================================================
    // Convenience methods (using unified API)
    // ========================================================================

    /// Executes a nested workflow and waits for its completion.
    fn execute_flow(
        &self,
        flow: Arc<Flow>,
        flow_id: BlobId,
        input: ValueRef,
        overrides: Option<WorkflowOverrides>,
    ) -> BoxFuture<'_, crate::Result<FlowResult>> {
        async move {
            let mut params = SubmitRunParams::new(flow, flow_id, vec![input]).with_wait(true);
            if let Some(overrides) = overrides {
                params = params.with_overrides(overrides);
            }
            let run_status = self.submit_run(params).await?;

            // Extract the single result
            let results = run_status.results.ok_or_else(|| {
                error_stack::report!(crate::PluginError::Execution)
                    .attach_printable("Expected results in response when wait=true")
            })?;
            let item_result = results.into_iter().next().ok_or_else(|| {
                error_stack::report!(crate::PluginError::Execution)
                    .attach_printable("Expected at least one result")
            })?;
            item_result.result.ok_or_else(|| {
                error_stack::report!(crate::PluginError::Execution).attach_printable(format!(
                    "Item has no result (status: {:?})",
                    item_result.status
                ))
            })
        }
        .boxed()
    }

    /// Executes a flow by blob ID - combines blob retrieval, deserialization, and execution.
    ///
    /// This is a convenience method that handles the common pattern of:
    /// 1. Retrieving a flow blob by ID from the state store
    /// 2. Deserializing the flow from blob data
    /// 3. Executing the flow with given input and optional overrides
    fn execute_flow_by_id(
        &self,
        flow_id: &BlobId,
        input: ValueRef,
        overrides: Option<WorkflowOverrides>,
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
            self.execute_flow(flow, flow_id, input, overrides).await
        }
        .boxed()
    }

    /// Convenience method: submit batch, wait for completion, and return results.
    fn execute_batch(
        &self,
        flow: Arc<Flow>,
        flow_id: BlobId,
        inputs: Vec<ValueRef>,
        max_concurrency: Option<usize>,
        overrides: Option<WorkflowOverrides>,
    ) -> BoxFuture<'_, crate::Result<Vec<FlowResult>>> {
        async move {
            let mut params = SubmitRunParams::new(flow, flow_id, inputs).with_wait(true);
            if let Some(max_concurrency) = max_concurrency {
                params = params.with_max_concurrency(max_concurrency);
            }
            if let Some(overrides) = overrides {
                params = params.with_overrides(overrides);
            }
            let run_status = self.submit_run(params).await?;

            // Extract FlowResult from each result
            let results = run_status.results.ok_or_else(|| {
                error_stack::report!(crate::PluginError::Execution)
                    .attach_printable("Expected results in response when wait=true")
            })?;

            results
                .into_iter()
                .enumerate()
                .map(|(idx, item_result)| {
                    item_result.result.ok_or_else(|| {
                        error_stack::report!(crate::PluginError::Execution).attach_printable(
                            format!(
                                "Item at index {} has no result (status: {:?})",
                                idx, item_result.status
                            ),
                        )
                    })
                })
                .collect()
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
    fn submit_run(&self, params: SubmitRunParams) -> BoxFuture<'_, crate::Result<RunStatus>> {
        self.context.submit_run(params)
    }

    fn get_run(
        &self,
        run_id: Uuid,
        options: GetRunOptions,
    ) -> BoxFuture<'_, crate::Result<RunStatus>> {
        self.context.get_run(run_id, options)
    }

    fn state_store(&self) -> &Arc<dyn StateStore> {
        self.context.state_store()
    }

    fn working_directory(&self) -> &Path {
        self.context.working_directory()
    }
}
