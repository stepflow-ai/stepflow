use std::borrow::Cow;
use std::{future::Future, pin::Pin};

use stepflow_core::status::ExecutionStatus;
use stepflow_core::{
    FlowResult,
    blob::BlobId,
    workflow::{Flow, ValueRef},
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
    fn put_blob(
        &self,
        data: ValueRef,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<BlobId, StateError>> + Send + '_>>;

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
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<ValueRef, StateError>> + Send + '_>>;

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
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<(), StateError>> + Send + '_>>;

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
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<FlowResult, StateError>> + Send + '_>>;

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
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<FlowResult, StateError>> + Send + '_>>;

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
    ) -> Pin<
        Box<
            dyn Future<Output = error_stack::Result<Vec<StepResult<'static>>, StateError>>
                + Send
                + '_,
        >,
    >;

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
        workflow: &Flow,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<String, StateError>> + Send + '_>>;

    /// Retrieve a workflow by its content hash.
    ///
    /// # Arguments
    /// * `workflow_hash` - The content hash of the workflow
    ///
    /// # Returns
    /// The workflow if found, or an error if not found
    fn get_workflow(
        &self,
        workflow_hash: &str,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<Flow, StateError>> + Send + '_>>;

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
        workflow_hash: &str,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<(), StateError>> + Send + '_>>;

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
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<Endpoint, StateError>> + Send + '_>>;

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
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<Vec<Endpoint>, StateError>> + Send + '_>>;

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
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<(), StateError>> + Send + '_>>;

    /// Create a new execution record.
    ///
    /// # Arguments
    /// * `execution_id` - The unique identifier for the execution
    /// * `endpoint_name` - Optional endpoint name if executing an endpoint
    /// * `endpoint_label` - Optional endpoint label if executing a labeled endpoint
    /// * `workflow_hash` - Optional workflow hash
    /// * `debug_mode` - Whether execution is in debug mode
    /// * `input_blob_id` - Optional blob ID for the input data
    ///
    /// # Returns
    /// Success if the execution was created
    fn create_execution(
        &self,
        execution_id: Uuid,
        endpoint_name: Option<&str>,
        endpoint_label: Option<&str>,
        workflow_hash: Option<&str>,
        debug_mode: bool,
        input_blob_id: Option<&BlobId>,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<(), StateError>> + Send + '_>>;

    /// Update execution status.
    ///
    /// # Arguments
    /// * `execution_id` - The execution identifier
    /// * `status` - The new status
    /// * `result_blob_id` - Optional blob ID for the result data
    ///
    /// # Returns
    /// Success if the execution was updated
    fn update_execution_status(
        &self,
        execution_id: Uuid,
        status: ExecutionStatus,
        result_blob_id: Option<&BlobId>,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<(), StateError>> + Send + '_>>;

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
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<ExecutionDetails, StateError>> + Send + '_>>;

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
    ) -> Pin<
        Box<
            dyn Future<Output = error_stack::Result<Vec<ExecutionSummary>, StateError>> + Send + '_,
        >,
    >;

    // Compound Query Methods (for server optimization)

    /// Get an endpoint with its associated workflow in a single operation.
    ///
    /// This is optimized for server endpoints that need both pieces of data.
    ///
    /// # Arguments
    /// * `name` - The endpoint name
    /// * `label` - The label name (None for default version)
    ///
    /// # Returns
    /// A tuple of (endpoint, workflow) if found
    fn get_endpoint_with_workflow(
        &self,
        name: &str,
        label: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<(Endpoint, Flow), StateError>> + Send + '_>>;

    /// Get an execution with its associated workflow in a single operation.
    ///
    /// This is optimized for server endpoints that need both pieces of data.
    ///
    /// # Arguments
    /// * `execution_id` - The execution identifier
    ///
    /// # Returns
    /// A tuple of (execution_details, optional_workflow) if found
    fn get_execution_with_workflow(
        &self,
        execution_id: Uuid,
    ) -> Pin<
        Box<
            dyn Future<Output = error_stack::Result<(ExecutionDetails, Option<Flow>), StateError>>
                + Send
                + '_,
        >,
    >;

    /// Get execution details with input and result blobs resolved.
    ///
    /// This is optimized for server endpoints that need complete execution information.
    ///
    /// # Arguments
    /// * `execution_id` - The execution identifier
    ///
    /// # Returns
    /// Execution details with resolved blobs
    fn get_execution_with_blobs(
        &self,
        execution_id: Uuid,
    ) -> Pin<
        Box<dyn Future<Output = error_stack::Result<ExecutionWithBlobs, StateError>> + Send + '_>,
    >;

    /// Get comprehensive execution step details for debugging and inspection.
    ///
    /// This is optimized for server endpoints that need workflow, execution, step results, and input.
    ///
    /// # Arguments
    /// * `execution_id` - The execution identifier
    ///
    /// # Returns
    /// Complete execution step details
    fn get_execution_step_details(
        &self,
        execution_id: Uuid,
    ) -> Pin<
        Box<dyn Future<Output = error_stack::Result<ExecutionStepDetails, StateError>> + Send + '_>,
    >;

    /// Get all data needed for debug session creation in a single operation.
    ///
    /// This is optimized for debug endpoints that need to reconstruct execution state.
    ///
    /// # Arguments
    /// * `execution_id` - The execution identifier
    ///
    /// # Returns
    /// Complete debug session data
    fn get_debug_session_data(
        &self,
        execution_id: Uuid,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<DebugSessionData, StateError>> + Send + '_>>;

    // Atomic Operations (for consistency)

    /// Create an endpoint with its workflow in an atomic operation.
    ///
    /// This stores the workflow and creates the endpoint atomically.
    ///
    /// # Arguments
    /// * `name` - The endpoint name
    /// * `label` - The label name (None for default version)
    /// * `workflow` - The workflow to store
    ///
    /// # Returns
    /// A tuple of (workflow_hash, endpoint) if successful
    fn create_endpoint_with_workflow(
        &self,
        name: &str,
        label: Option<&str>,
        workflow: &Flow,
    ) -> Pin<
        Box<dyn Future<Output = error_stack::Result<(String, Endpoint), StateError>> + Send + '_>,
    >;

    /// Create an execution with input storage in an atomic operation.
    ///
    /// This stores the input as a blob and creates the execution atomically.
    ///
    /// # Arguments
    /// * `execution_id` - The unique identifier for the execution
    /// * `params` - Parameters for execution creation
    /// * `input` - Optional input data to store as blob
    ///
    /// # Returns
    /// The created execution details
    fn create_execution_with_input<'a>(
        &'a self,
        execution_id: Uuid,
        params: CreateExecutionParams<'a>,
        input: Option<ValueRef>,
    ) -> Pin<Box<dyn Future<Output = error_stack::Result<ExecutionDetails, StateError>> + Send + 'a>>;

    // Optimized Query Methods (for value resolution)

    /// Try to get a step result by step ID, returning None if not found.
    ///
    /// This is optimized for value resolution during execution where missing results are expected.
    ///
    /// # Arguments
    /// * `execution_id` - The unique identifier for the workflow execution
    /// * `step_id` - The identifier of the step within the workflow
    ///
    /// # Returns
    /// The execution result if found, None if not found, or error for other failures
    fn try_get_step_result_by_id(
        &self,
        execution_id: Uuid,
        step_id: &str,
    ) -> Pin<
        Box<dyn Future<Output = error_stack::Result<Option<FlowResult>, StateError>> + Send + '_>,
    >;

    /// Get multiple step results by their indices in a single operation.
    ///
    /// This is optimized for batch step result retrieval.
    ///
    /// # Arguments
    /// * `execution_id` - The unique identifier for the workflow execution
    /// * `indices` - The step indices to retrieve (0-based)
    ///
    /// # Returns
    /// A vector of optional results, indexed by the input indices
    fn get_step_results_by_indices(
        &self,
        execution_id: Uuid,
        indices: &[usize],
    ) -> Pin<
        Box<
            dyn Future<Output = error_stack::Result<Vec<Option<FlowResult>>, StateError>>
                + Send
                + '_,
        >,
    >;
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
    pub workflow_hash: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Summary information about a workflow execution.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionSummary {
    pub execution_id: Uuid,
    pub endpoint_name: Option<String>,
    pub endpoint_label: Option<String>,
    pub workflow_hash: Option<String>,
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
    pub workflow_hash: Option<String>,
    pub status: ExecutionStatus,
    pub debug_mode: bool,
    pub input_blob_id: Option<BlobId>,
    pub result_blob_id: Option<BlobId>,
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
    pub workflow: Option<stepflow_core::workflow::Flow>,
    pub step_results: Vec<StepResult<'static>>,
    pub input: Option<ValueRef>,
}

/// Complete data needed for debug session creation.
#[derive(Debug)]
pub struct DebugSessionData {
    pub execution: ExecutionDetails,
    pub workflow: stepflow_core::workflow::Flow,
    pub input: ValueRef,
    pub step_results: Vec<StepResult<'static>>,
}

/// Parameters for creating an execution.
#[derive(Debug, Clone)]
pub struct CreateExecutionParams<'a> {
    pub endpoint_name: Option<&'a str>,
    pub endpoint_label: Option<&'a str>,
    pub workflow_hash: Option<&'a str>,
    pub debug_mode: bool,
}
