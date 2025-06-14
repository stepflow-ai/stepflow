use std::borrow::Cow;
use std::sync::Arc;

use futures::future::BoxFuture;
use stepflow_core::status::ExecutionStatus;
use stepflow_core::workflow::FlowHash;
use stepflow_core::{
    FlowResult,
    blob::BlobId,
    workflow::{Component, Flow, ValueRef},
};

use uuid::Uuid;

use crate::StateError;

/// Trait for storing and retrieving state data including blobs.
///
/// This trait provides the foundation for both blob storage and future
/// execution state persistence (journaling, checkpointing, etc.).
pub trait StateStore: Send + Sync {
    /// Store JSON data as a blob and return its content-based ID.
    ///
    /// The blob ID is generated as a SHA-256 hash of the JSON content,
    /// providing deterministic IDs and automatic deduplication.
    ///
    /// # Arguments
    /// * `data` - The JSON data to store as a blob
    ///
    /// # Returns
    /// The blob ID for the stored data
    fn put_blob(&self, data: ValueRef) -> BoxFuture<'_, error_stack::Result<BlobId, StateError>>;

    /// Retrieve JSON data by blob ID.
    ///
    /// # Arguments
    /// * `blob_id` - The blob ID to retrieve
    ///
    /// # Returns
    /// The JSON data associated with the blob ID, or an error if not found
    fn get_blob(
        &self,
        blob_id: &BlobId,
    ) -> BoxFuture<'_, error_stack::Result<ValueRef, StateError>>;

    /// Record the result of a step execution.
    ///
    /// This method stores the execution result for a specific step within a workflow
    /// execution, enabling workflow recovery and debugging capabilities.
    ///
    /// # Arguments
    /// * `execution_id` - The unique identifier for the workflow execution
    /// * `step_idx` - The index of the step within the workflow (0-based)
    /// * `step_id` - The identifier of the step within the workflow
    /// * `result` - The execution result to store
    ///
    /// # Returns
    /// Success if the result was stored, or an error if storage failed
    fn record_step_result(
        &self,
        execution_id: Uuid,
        step_result: StepResult<'_>,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

    /// Retrieve the result of a step execution by step index.
    ///
    /// # Arguments
    /// * `execution_id` - The unique identifier for the workflow execution
    /// * `step_idx` - The index of the step within the workflow (0-based)
    ///
    /// # Returns
    /// The execution result if found, or an error if not found
    fn get_step_result_by_index(
        &self,
        execution_id: Uuid,
        step_idx: usize,
    ) -> BoxFuture<'_, error_stack::Result<FlowResult, StateError>>;

    /// Retrieve the result of a step execution by step ID.
    ///
    /// # Arguments
    /// * `execution_id` - The unique identifier for the workflow execution
    /// * `step_id` - The identifier of the step within the workflow
    ///
    /// # Returns
    /// The execution result if found, or an error if not found
    fn get_step_result_by_id(
        &self,
        execution_id: Uuid,
        step_id: &str,
    ) -> BoxFuture<'_, error_stack::Result<FlowResult, StateError>>;

    /// List all step results for a workflow execution, ordered by step index.
    ///
    /// This is useful for workflow recovery and debugging to see which steps
    /// have completed and their results in workflow order.
    ///
    /// # Arguments
    /// * `execution_id` - The unique identifier for the workflow execution
    ///
    /// # Returns
    /// A vector of (step_index, step_id, result) tuples, ordered by step_index
    fn list_step_results(
        &self,
        execution_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Vec<StepResult<'static>>, StateError>>;

    // Workflow Management Methods

    /// Store a workflow by its content hash.
    ///
    /// # Arguments
    /// * `workflow` - The workflow to store
    ///
    /// # Returns
    /// The content hash of the stored workflow
    fn store_workflow(
        &self,
        workflow: Arc<Flow>,
    ) -> BoxFuture<'_, error_stack::Result<FlowHash, StateError>>;

    /// Retrieve a workflow by its content hash.
    ///
    /// # Arguments
    /// * `workflow_hash` - The content hash of the workflow
    ///
    /// # Returns
    /// The workflow if found, or an error if not found
    fn get_workflow(
        &self,
        workflow_hash: &FlowHash,
    ) -> BoxFuture<'_, error_stack::Result<Option<Arc<Flow>>, StateError>>;

    /// Create or update a named endpoint with optional label.
    ///
    /// # Arguments
    /// * `name` - The endpoint name
    /// * `label` - The label name (None for default version)
    /// * `workflow_hash` - The content hash of the workflow
    ///
    /// # Returns
    /// Success if the endpoint was created/updated
    fn create_endpoint(
        &self,
        name: &str,
        label: Option<&str>,
        workflow_hash: FlowHash,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

    /// Get an endpoint by name and optional label.
    ///
    /// # Arguments
    /// * `name` - The endpoint name
    /// * `label` - The label name (None for default version)
    ///
    /// # Returns
    /// The endpoint if found, or an error if not found
    fn get_endpoint(
        &self,
        name: &str,
        label: Option<&str>,
    ) -> BoxFuture<'_, error_stack::Result<Option<Endpoint>, StateError>>;

    /// List all endpoints, optionally filtered by name.
    ///
    /// # Arguments
    /// * `name_filter` - Optional name filter to list all versions of a specific endpoint
    ///
    /// # Returns
    /// A vector of all endpoints (all versions if name_filter is provided, otherwise all endpoints)
    fn list_endpoints(
        &self,
        name_filter: Option<&str>,
    ) -> BoxFuture<'_, error_stack::Result<Vec<Endpoint>, StateError>>;

    /// Delete an endpoint by name and optional label.
    ///
    /// # Arguments
    /// * `name` - The endpoint name
    /// * `label` - The label name (None for default version, "*" to delete all versions)
    ///
    /// # Returns
    /// Success if the endpoint was deleted
    fn delete_endpoint(
        &self,
        name: &str,
        label: Option<&str>,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

    /// Create a new execution record.
    ///
    /// # Arguments
    /// * `execution_id` - The unique identifier for the execution
    /// * `endpoint_name` - Optional endpoint name if executing an endpoint
    /// * `endpoint_label` - Optional endpoint label if executing a labeled endpoint
    /// * `workflow_hash` - Workflow hash
    /// * `debug_mode` - Whether execution is in debug mode
    /// * `input` - Input data as JSON
    ///
    /// # Returns
    /// Success if the execution was created
    fn create_execution(
        &self,
        execution_id: Uuid,
        endpoint_name: Option<&str>,
        endpoint_label: Option<&str>,
        workflow_hash: FlowHash,
        debug_mode: bool,
        input: ValueRef,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

    /// Update execution status.
    ///
    /// # Arguments
    /// * `execution_id` - The execution identifier
    /// * `status` - The new status
    /// * `result` - Optional result data as JSON
    ///
    /// # Returns
    /// Success if the execution was updated
    fn update_execution_status(
        &self,
        execution_id: Uuid,
        status: ExecutionStatus,
        result: Option<ValueRef>,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

    /// Get execution details.
    ///
    /// # Arguments
    /// * `execution_id` - The execution identifier
    ///
    /// # Returns
    /// The execution details if found
    fn get_execution(
        &self,
        execution_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Option<ExecutionDetails>, StateError>>;

    /// List executions with optional filtering.
    ///
    /// # Arguments
    /// * `filters` - Optional filters for the query
    ///
    /// # Returns
    /// A vector of execution summaries
    fn list_executions(
        &self,
        filters: &ExecutionFilters,
    ) -> BoxFuture<'_, error_stack::Result<Vec<ExecutionSummary>, StateError>>;

    // Step Status Management

    /// Initialize step info for an execution.
    fn initialize_step_info(
        &self,
        execution_id: Uuid,
        steps: &[StepInfo],
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

    /// Update the status of a step.
    fn update_step_status(
        &self,
        execution_id: Uuid,
        step_index: usize,
        status: stepflow_core::status::StepStatus,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

    /// Get all step info for an execution.
    fn get_step_info_for_execution(
        &self,
        execution_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Vec<StepInfo>, StateError>>;

    /// Get runnable steps for an execution based on current status.
    /// Note: This method returns steps based on persistent status only.
    /// Dependency checking should be done by the caller using workflow analysis.
    fn get_runnable_steps(
        &self,
        execution_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Vec<StepInfo>, StateError>>;
}

/// The step result.
#[derive(PartialEq, Debug, Clone)]
pub struct StepResult<'a> {
    step_idx: usize,
    step_id: Cow<'a, str>,
    result: FlowResult,
}

impl<'a> StepResult<'a> {
    /// Create a new step result.
    pub fn new(step_idx: usize, step_id: impl Into<Cow<'a, str>>, result: FlowResult) -> Self {
        Self {
            step_idx,
            step_id: step_id.into(),
            result,
        }
    }

    /// Convert to an owned version of the step result.
    pub fn to_owned(&self) -> StepResult<'static> {
        StepResult {
            step_idx: self.step_idx,
            step_id: Cow::Owned(self.step_id.to_string()),
            result: self.result.clone(),
        }
    }

    /// Get the step index.
    pub fn step_idx(&self) -> usize {
        self.step_idx
    }

    /// Get the step ID.
    pub fn step_id(&self) -> &str {
        &self.step_id
    }

    /// Get the step result.
    pub fn result(&self) -> &FlowResult {
        &self.result
    }

    /// Consume self and return the step result.
    pub fn into_result(self) -> FlowResult {
        self.result
    }
}

impl PartialOrd for StepResult<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.step_idx.partial_cmp(&other.step_idx)
    }
}

/// An endpoint represents a named workflow that can be executed, with an optional label.
#[derive(Debug, Clone, PartialEq)]
pub struct Endpoint {
    pub name: String,
    pub label: Option<String>, // None represents the default version
    pub workflow_hash: FlowHash,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Summary information about a workflow execution.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionSummary {
    pub execution_id: Uuid,
    pub endpoint_name: Option<String>,
    pub endpoint_label: Option<String>,
    pub workflow_hash: FlowHash,
    pub status: ExecutionStatus,
    pub debug_mode: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Detailed execution information including input and result.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionDetails {
    pub execution_id: Uuid,
    pub endpoint_name: Option<String>,
    pub endpoint_label: Option<String>,
    pub workflow_hash: FlowHash,
    pub status: ExecutionStatus,
    pub debug_mode: bool,
    pub input: ValueRef,
    pub result: Option<ValueRef>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Filters for listing executions.
#[derive(Debug, Clone, Default)]
pub struct ExecutionFilters {
    pub status: Option<ExecutionStatus>,
    pub endpoint_name: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Execution details with resolved input and result blobs.
#[derive(Debug)]
pub struct ExecutionWithBlobs {
    pub execution: ExecutionDetails,
    pub input: Option<ValueRef>,
    pub result: Option<ValueRef>,
}

/// Comprehensive execution step details for server inspection.
#[derive(Debug)]
pub struct ExecutionStepDetails {
    pub execution: ExecutionDetails,
    pub workflow: Option<Arc<stepflow_core::workflow::Flow>>,
    pub step_results: Vec<StepResult<'static>>,
    pub input: Option<ValueRef>,
}

/// Complete data needed for debug session creation.
#[derive(Debug)]
pub struct DebugSessionData {
    pub execution: ExecutionDetails,
    pub workflow: Arc<stepflow_core::workflow::Flow>,
    pub input: ValueRef,
    pub step_results: Vec<StepResult<'static>>,
}

/// Step information for a workflow execution.
#[derive(Debug, Clone, PartialEq)]
pub struct StepInfo {
    /// Execution ID this step belongs to
    pub execution_id: Uuid,
    /// Index of the step in the workflow
    pub step_index: usize,
    /// Step ID
    pub step_id: String,
    /// Component name/URL
    pub component: Component,
    /// Current status of the step
    pub status: stepflow_core::status::StepStatus,
    /// When the step was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the step was last updated
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
