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

use crate::StepflowEnvironment;
use crate::subflow::SubflowSubmitter;
use std::path::Path;
use std::sync::Arc;
use stepflow_core::{
    BlobId, FlowResult,
    workflow::{Flow, StepId, ValueRef, WorkflowOverrides},
};
use stepflow_dtos::ResultOrder;
use stepflow_state::StateStore;
use uuid::Uuid;

/// Run hierarchy context for execution.
///
/// This captures the run tree context for a component execution,
/// allowing bidirectional message handlers to know which run tree
/// they're serving. It optionally includes a submitter for in-process
/// sub-flow submission.
///
/// `RunContext` is cheap to clone - it contains two UUIDs and an optional
/// `SubflowSubmitter` (which itself is just an mpsc::Sender + two UUIDs).
#[derive(Clone, Debug)]
pub struct RunContext {
    /// The current run ID.
    pub run_id: Uuid,
    /// The root run ID for this execution tree.
    ///
    /// For top-level runs, this equals `run_id`.
    /// For sub-flows, this is the original run that started the tree.
    pub root_run_id: Uuid,
    /// Optional submitter for in-process sub-flow submission.
    ///
    /// When present, sub-flows can be submitted directly to the parent
    /// executor without going through the external API.
    pub subflow_submitter: Option<SubflowSubmitter>,
}

impl RunContext {
    /// Create a new RunContext for a root (top-level) run.
    ///
    /// Sets `root_run_id` equal to `run_id`.
    pub fn for_root(run_id: Uuid) -> Self {
        Self {
            run_id,
            root_run_id: run_id,
            subflow_submitter: None,
        }
    }

    /// Create a new RunContext for a sub-flow within an existing run tree.
    ///
    /// The `root_run_id` is inherited from the parent context.
    pub fn for_subflow(&self, run_id: Uuid) -> Self {
        Self {
            run_id,
            root_run_id: self.root_run_id,
            subflow_submitter: self.subflow_submitter.clone(),
        }
    }

    /// Create a new RunContext with a subflow submitter.
    pub fn with_submitter(mut self, submitter: SubflowSubmitter) -> Self {
        self.subflow_submitter = Some(submitter);
        self
    }
}

/// Execution context for workflow runs.
///
/// This struct provides access to:
/// - The shared environment (state store, working directory, plugin router)
/// - Run-specific information (run ID, step, flow)
/// - Subflow submission for nested flow execution
///
/// `ExecutionContext` is passed to plugins during step execution and provides
/// everything needed to execute a component, including the ability to submit
/// nested sub-flows.
///
/// Every `ExecutionContext` has a `RunContext` because execution always happens
/// within the context of a run. The `RunContext` provides run hierarchy information
/// and optionally a subflow submitter for nested execution.
#[derive(Clone)]
pub struct ExecutionContext {
    /// The shared environment.
    env: Arc<StepflowEnvironment>,
    /// The current step being executed (contains flow reference and step index).
    step: Option<StepId>,
    /// Flow reference for workflow-level contexts (when step is None).
    flow: Option<Arc<Flow>>,
    /// Flow ID (content hash) for the current flow.
    flow_id: Option<BlobId>,
    /// Run context for bidirectional communication (run hierarchy, subflow submission).
    /// Always present - execution always happens within a run.
    run_context: Arc<RunContext>,
}

impl ExecutionContext {
    /// Create a new ExecutionContext for a specific step.
    pub fn for_step(
        env: Arc<StepflowEnvironment>,
        step: StepId,
        flow_id: BlobId,
        run_context: Arc<RunContext>,
    ) -> Self {
        Self {
            env,
            step: Some(step),
            flow: None,
            flow_id: Some(flow_id),
            run_context,
        }
    }

    /// Create a new ExecutionContext for testing without a flow.
    ///
    /// This is only for unit tests where a full Flow is not available.
    /// In production code, use `for_step()` or `for_workflow_with_flow()`.
    pub fn for_testing(env: Arc<StepflowEnvironment>, run_context: Arc<RunContext>) -> Self {
        Self {
            env,
            step: None,
            flow: None,
            flow_id: None,
            run_context,
        }
    }

    /// Create a new ExecutionContext for workflow-level operations (no specific step).
    pub fn for_workflow(env: Arc<StepflowEnvironment>, run_context: Arc<RunContext>) -> Self {
        Self {
            env,
            step: None,
            flow: None,
            flow_id: None,
            run_context,
        }
    }

    /// Create a new ExecutionContext for workflow-level operations with flow metadata access.
    pub fn for_workflow_with_flow(
        env: Arc<StepflowEnvironment>,
        flow: Arc<Flow>,
        flow_id: BlobId,
        run_context: Arc<RunContext>,
    ) -> Self {
        Self {
            env,
            step: None,
            flow: Some(flow),
            flow_id: Some(flow_id),
            run_context,
        }
    }

    /// Create a new ExecutionContext with a step, reusing the same environment and run context.
    pub fn with_step(&self, step: StepId) -> Self {
        Self {
            env: self.env.clone(),
            step: Some(step),
            flow: None, // Flow is accessible via step.flow
            flow_id: self.flow_id.clone(),
            run_context: self.run_context.clone(),
        }
    }

    /// Get the run context.
    pub fn run_context(&self) -> &Arc<RunContext> {
        &self.run_context
    }

    /// Get the execution ID for this context.
    pub fn run_id(&self) -> Uuid {
        self.run_context.run_id
    }

    /// Get the step for this context, if available.
    pub fn step(&self) -> Option<&StepId> {
        self.step.as_ref()
    }

    /// Get the step name for this context, if available.
    pub fn step_id(&self) -> Option<&str> {
        self.step.as_ref().map(|s| s.step_name())
    }

    /// Get the flow for this context, if available.
    pub fn flow(&self) -> Option<&Arc<Flow>> {
        self.step.as_ref().map(|s| &s.flow).or(self.flow.as_ref())
    }

    /// Get the flow ID for this context, if available.
    pub fn flow_id(&self) -> Option<&BlobId> {
        self.flow_id.as_ref()
    }

    /// Get the shared environment.
    pub fn env(&self) -> &Arc<StepflowEnvironment> {
        &self.env
    }

    /// Get a reference to the state store.
    pub fn state_store(&self) -> &Arc<dyn StateStore> {
        self.env.state_store()
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
        self.run_context.subflow_submitter.as_ref()
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
    /// 1. Retrieving a flow blob by ID from the state store
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
            .state_store()
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

        // Wait for completion using the unified state store notification
        self.state_store()
            .wait_for_completion(run_id)
            .await
            .change_context(crate::PluginError::Execution)?;

        // Get results from state store
        let items = self
            .state_store()
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
