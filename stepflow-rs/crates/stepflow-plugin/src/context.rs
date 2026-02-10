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

//! Execution context types for workflow runs.

use crate::subflow::SubflowSubmitter;
use std::path::Path;
use std::sync::Arc;
use stepflow_core::{
    BlobId, FlowResult, StepflowEnvironment,
    workflow::{Flow, ValueRef, WorkflowOverrides},
};
use stepflow_dtos::ResultOrder;
use stepflow_state::{BlobStore, BlobStoreExt as _, MetadataStore, MetadataStoreExt as _};
use uuid::Uuid;

/// Run context for workflow execution.
///
/// This is the primary context type for workflow runs, providing access to:
/// - Run hierarchy information (run_id, root_run_id)
/// - The flow being executed and its ID
/// - The shared environment (state store, working directory, plugin router)
/// - Subflow submission for nested flow execution
///
/// `RunContext` is passed to plugins during step execution. The step being
/// executed is passed separately as a `StepId` parameter.
#[derive(Clone)]
pub struct RunContext {
    /// The current run ID.
    pub run_id: Uuid,
    /// The root run ID for this execution tree.
    ///
    /// For top-level runs, this equals `run_id`.
    /// For sub-flows, this is the original run that started the tree.
    pub root_run_id: Uuid,
    /// The flow being executed.
    pub flow: Arc<Flow>,
    /// Flow ID (content hash) for the current flow.
    pub flow_id: BlobId,
    /// The shared environment.
    env: Arc<StepflowEnvironment>,
    /// Optional submitter for in-process sub-flow submission.
    ///
    /// When present, sub-flows can be submitted directly to the parent
    /// executor without going through the external API.
    pub subflow_submitter: Option<SubflowSubmitter>,
}

impl std::fmt::Debug for RunContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunContext")
            .field("run_id", &self.run_id)
            .field("root_run_id", &self.root_run_id)
            .field("flow_id", &self.flow_id)
            .field("subflow_submitter", &self.subflow_submitter.is_some())
            .finish()
    }
}

impl RunContext {
    /// Create a new RunContext for a root (top-level) run.
    ///
    /// Sets `root_run_id` equal to `run_id`.
    pub fn new(
        run_id: Uuid,
        flow: Arc<Flow>,
        flow_id: BlobId,
        env: Arc<StepflowEnvironment>,
    ) -> Self {
        Self {
            run_id,
            root_run_id: run_id,
            flow,
            flow_id,
            env,
            subflow_submitter: None,
        }
    }

    /// Create a new RunContext for a sub-flow within an existing run tree.
    ///
    /// The `root_run_id` and `subflow_submitter` are inherited from the parent context.
    /// The environment is shared.
    pub fn for_subflow(&self, run_id: Uuid, flow: Arc<Flow>, flow_id: BlobId) -> Self {
        Self {
            run_id,
            root_run_id: self.root_run_id,
            flow,
            flow_id,
            env: self.env.clone(),
            subflow_submitter: self.subflow_submitter.clone(),
        }
    }

    /// Add a subflow submitter (builder pattern).
    pub fn with_submitter(mut self, submitter: SubflowSubmitter) -> Self {
        self.subflow_submitter = Some(submitter);
        self
    }

    /// Get the flow for this context.
    pub fn flow(&self) -> &Arc<Flow> {
        &self.flow
    }

    /// Get the flow ID for this context.
    ///
    /// This is the [`BlobId`] derived from the content hash of the flow definition,
    /// i.e., a content-addressed identifier for the flow itself (not the run ID).
    pub fn flow_id(&self) -> &BlobId {
        &self.flow_id
    }

    /// Get the shared environment.
    pub fn env(&self) -> &Arc<StepflowEnvironment> {
        &self.env
    }

    /// Get a reference to the metadata store.
    pub fn metadata_store(&self) -> &Arc<dyn MetadataStore> {
        self.env.metadata_store()
    }

    /// Get a reference to the blob store.
    pub fn blob_store(&self) -> &Arc<dyn BlobStore> {
        self.env.blob_store()
    }

    /// Get the working directory.
    pub fn working_directory(&self) -> &Path {
        self.env.working_directory()
    }

    /// Get the subflow submitter if available.
    ///
    /// When present, this can be used for direct in-process subflow submission
    /// to the parent executor's loop. This is more efficient than going through
    /// the external API.
    ///
    /// Note: The `execute_flow` and `execute_batch` methods automatically use
    /// the subflow submitter when available, so most code doesn't need to
    /// access this directly.
    pub fn subflow_submitter(&self) -> Option<&SubflowSubmitter> {
        self.subflow_submitter.as_ref()
    }

    // ========================================================================
    // Flow execution methods
    // ========================================================================

    /// Executes a nested workflow and waits for its completion.
    ///
    /// This requires a subflow submitter to be available in the run context.
    pub async fn execute_flow(
        &self,
        flow: Arc<Flow>,
        flow_id: BlobId,
        input: ValueRef,
        overrides: Option<WorkflowOverrides>,
    ) -> crate::Result<FlowResult> {
        let submitter = self.subflow_submitter().ok_or_else(|| {
            error_stack::report!(crate::PluginError::Execution)
                .attach_printable("No subflow submitter available")
        })?;

        let mut results = self
            .execute_via_channel(submitter, flow, flow_id, vec![input], overrides)
            .await?;
        Ok(results.pop().unwrap())
    }

    /// Executes a flow by blob ID - combines blob retrieval, deserialization, and execution.
    ///
    /// This is a convenience method that handles the common pattern of:
    /// 1. Retrieving a flow blob by ID from the blob store
    /// 2. Deserializing the flow from blob data
    /// 3. Executing the flow with given input and optional overrides
    pub async fn execute_flow_by_id(
        &self,
        flow_id: &BlobId,
        input: ValueRef,
        overrides: Option<WorkflowOverrides>,
    ) -> crate::Result<FlowResult> {
        use error_stack::ResultExt as _;

        // Retrieve the flow from the blob store
        let blob_data = self
            .blob_store()
            .get_blob(flow_id)
            .await
            .change_context(crate::PluginError::Execution)?;

        // Deserialize the flow from blob data
        let flow = blob_data
            .as_flow()
            .ok_or_else(|| error_stack::report!(crate::PluginError::Execution))?
            .clone();

        // Execute the flow
        self.execute_flow(flow, flow_id.clone(), input, overrides)
            .await
    }

    /// Execute a batch of workflows.
    ///
    /// This requires a subflow submitter to be available in the run context.
    pub async fn execute_batch(
        &self,
        flow: Arc<Flow>,
        flow_id: BlobId,
        inputs: Vec<ValueRef>,
        _max_concurrency: Option<usize>,
        overrides: Option<WorkflowOverrides>,
    ) -> crate::Result<Vec<FlowResult>> {
        let submitter = self.subflow_submitter().ok_or_else(|| {
            error_stack::report!(crate::PluginError::Execution)
                .attach_printable("No subflow submitter available")
        })?;

        // TODO: Pass max_concurrency to subflow submission
        self.execute_via_channel(submitter, flow, flow_id, inputs, overrides)
            .await
    }

    /// Internal helper to execute subflow via channel-based submission.
    async fn execute_via_channel(
        &self,
        submitter: &SubflowSubmitter,
        flow: Arc<Flow>,
        flow_id: BlobId,
        inputs: Vec<ValueRef>,
        overrides: Option<WorkflowOverrides>,
    ) -> crate::Result<Vec<FlowResult>> {
        use error_stack::ResultExt as _;

        // Submit the subflow
        let run_id = submitter
            .submit(
                flow,
                flow_id,
                inputs,
                std::collections::HashMap::new(), // TODO: support variables
                overrides,
                None, // TODO: support max_concurrency
            )
            .await
            .change_context(crate::PluginError::Execution)?;

        // Wait for completion using the unified metadata store notification
        self.metadata_store()
            .wait_for_completion(run_id)
            .await
            .change_context(crate::PluginError::Execution)?;

        // Get results from metadata store
        let items = self
            .metadata_store()
            .get_item_results(run_id, ResultOrder::ByIndex)
            .await
            .change_context(crate::PluginError::Execution)?;

        // Extract FlowResults
        items
            .into_iter()
            .enumerate()
            .map(|(idx, item)| {
                item.result.ok_or_else(|| {
                    error_stack::report!(crate::PluginError::Execution).attach_printable(format!(
                        "Subflow item at index {} has no result (status: {:?})",
                        idx, item.status
                    ))
                })
            })
            .collect()
    }
}
