use poem_openapi::{OpenApi, payload::Json};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_core::{
    FlowResult,
    workflow::ValueRef,
};
use stepflow_execution::StepFlowExecutor;
use stepflow_state::{ExecutionDetails, ExecutionStatus, ExecutionSummary};
use uuid::Uuid;

use super::api_type::ApiType;
use super::common::{ApiWorkflowResponse, WorkflowResponse};
use super::error::{IntoPoemError as _, ServerResult, invalid_uuid};

/// Execution summary for API responses
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExecutionSummaryResponse {
    /// The execution ID (UUID string)
    pub execution_id: String,
    /// The endpoint name (if executed via endpoint)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint_name: Option<String>,
    /// The workflow hash
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_hash: Option<String>,
    /// Current status of the execution
    pub status: ExecutionStatus,
    /// Whether execution is in debug mode
    pub debug_mode: bool,
    /// When the execution was created (ISO 8601 string)
    pub created_at: String,
    /// When the execution was completed (ISO 8601 string, if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
}

/// API wrapper for ExecutionSummaryResponse
pub type ApiExecutionSummaryResponse = ApiType<ExecutionSummaryResponse>;

/// Detailed execution information for API responses
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExecutionDetailsResponse {
    /// The execution ID (UUID string)
    pub execution_id: String,
    /// The endpoint name (if executed via endpoint)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint_name: Option<String>,
    /// The workflow hash
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_hash: Option<String>,
    /// Current status of the execution
    pub status: ExecutionStatus,
    /// Whether execution is in debug mode
    pub debug_mode: bool,
    /// Input data (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<ValueRef>,
    /// Result data (if completed and available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<FlowResult>,
    /// When the execution was created (ISO 8601 string)
    pub created_at: String,
    /// When the execution was completed (ISO 8601 string, if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
}

/// API wrapper for ExecutionDetailsResponse
pub type ApiExecutionDetailsResponse = ApiType<ExecutionDetailsResponse>;

/// Response for listing executions
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListExecutionsResponse {
    /// List of execution summaries
    pub executions: Vec<ExecutionSummaryResponse>,
}

/// API wrapper for ListExecutionsResponse
pub type ApiListExecutionsResponse = ApiType<ListExecutionsResponse>;

/// Response for step execution details
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StepExecutionResponse {
    /// Step index in the workflow
    pub step_index: usize,
    /// Step ID (if provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_id: Option<String>,
    /// The result of the step execution
    pub result: FlowResult,
}

/// API wrapper for StepExecutionResponse
pub type ApiStepExecutionResponse = ApiType<StepExecutionResponse>;

/// Response for listing step executions
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListStepExecutionsResponse {
    /// List of step execution results
    pub steps: Vec<StepExecutionResponse>,
}

/// API wrapper for ListStepExecutionsResponse
pub type ApiListStepExecutionsResponse = ApiType<ListStepExecutionsResponse>;


pub struct ExecutionsApi {
    executor: Arc<StepFlowExecutor>,
}

impl ExecutionsApi {
    pub fn new(executor: Arc<StepFlowExecutor>) -> Self {
        Self { executor }
    }
}

#[OpenApi]
impl ExecutionsApi {
    /// Get execution details by ID
    #[oai(path = "/executions/:execution_id", method = "get")]
    pub async fn get_execution(
        &self,
        execution_id: poem::web::Path<String>,
    ) -> ServerResult<Json<ApiExecutionDetailsResponse>> {
        let state_store = self.executor.state_store();

        // Parse UUID from string
        let uuid = Uuid::parse_str(&execution_id.0).map_err(|_| invalid_uuid(&execution_id.0))?;

        // Get execution details
        let details = state_store.get_execution(uuid).await.into_poem()?;
        let response = ExecutionDetailsResponse::from(details);

        // TODO: Populate input and result from blobs if available

        Ok(Json(ApiType(response)))
    }

    /// Get the workflow definition for an execution
    #[oai(path = "/executions/:execution_id/workflow", method = "get")]
    pub async fn get_execution_workflow(
        &self,
        execution_id: poem::web::Path<String>,
    ) -> ServerResult<Json<ApiWorkflowResponse>> {
        let state_store = self.executor.state_store();

        // Parse UUID from string
        let uuid = Uuid::parse_str(&execution_id.0).map_err(|_| invalid_uuid(&execution_id.0))?;

        // Get execution details to retrieve the workflow hash
        let execution = state_store.get_execution(uuid).await.into_poem()?;

        // Get the workflow hash
        let workflow_hash = execution.workflow_hash.ok_or_else(|| {
            use super::error::not_found;
            not_found("No workflow hash available for this execution")
        })?;

        // Get the workflow from the state store
        let workflow = state_store.get_workflow(&workflow_hash).await.into_poem()?;
        use stepflow_core::workflow::FlowRef;
        let flow_ref = FlowRef::new(workflow);

        Ok(Json(ApiType(WorkflowResponse {
            workflow: flow_ref,
            workflow_hash,
        })))
    }

    /// List executions with optional filtering
    #[oai(path = "/executions", method = "get")]
    pub async fn list_executions(&self) -> ServerResult<Json<ApiListExecutionsResponse>> {
        let state_store = self.executor.state_store();

        // TODO: Add query parameters for filtering (status, endpoint_name, limit, offset)
        let filters = stepflow_state::ExecutionFilters::default();

        let executions = state_store.list_executions(&filters).await.into_poem()?;
        let execution_responses: Vec<ExecutionSummaryResponse> = executions
            .into_iter()
            .map(ExecutionSummaryResponse::from)
            .collect();

        Ok(Json(ApiType(ListExecutionsResponse {
            executions: execution_responses,
        })))
    }

    /// Get step-level execution details for a specific execution
    #[oai(path = "/executions/:execution_id/steps", method = "get")]
    pub async fn get_execution_steps(
        &self,
        execution_id: poem::web::Path<String>,
    ) -> ServerResult<Json<ApiListStepExecutionsResponse>> {
        let state_store = self.executor.state_store();

        // Parse UUID from string
        let uuid = Uuid::parse_str(&execution_id.0).map_err(|_| invalid_uuid(&execution_id.0))?;

        // Get step results for the execution
        let step_results = state_store.list_step_results(uuid).await.into_poem()?;
        let step_responses: Vec<StepExecutionResponse> = step_results
            .into_iter()
            .map(StepExecutionResponse::from)
            .collect();

        Ok(Json(ApiType(ListStepExecutionsResponse {
            steps: step_responses,
        })))
    }
}

// Conversion implementations
impl From<ExecutionSummary> for ExecutionSummaryResponse {
    fn from(summary: ExecutionSummary) -> Self {
        Self {
            execution_id: summary.execution_id.to_string(),
            endpoint_name: summary.endpoint_name,
            workflow_hash: summary.workflow_hash,
            status: summary.status,
            debug_mode: summary.debug_mode,
            created_at: summary.created_at.to_rfc3339(),
            completed_at: summary.completed_at.map(|dt| dt.to_rfc3339()),
        }
    }
}

impl From<ExecutionDetails> for ExecutionDetailsResponse {
    fn from(details: ExecutionDetails) -> Self {
        Self {
            execution_id: details.execution_id.to_string(),
            endpoint_name: details.endpoint_name,
            workflow_hash: details.workflow_hash,
            status: details.status,
            debug_mode: details.debug_mode,
            input: None,  // Will be populated separately if needed
            result: None, // Will be populated separately if needed
            created_at: details.created_at.to_rfc3339(),
            completed_at: details.completed_at.map(|dt| dt.to_rfc3339()),
        }
    }
}

impl From<stepflow_state::StepResult<'_>> for StepExecutionResponse {
    fn from(step_result: stepflow_state::StepResult<'_>) -> Self {
        Self {
            step_index: step_result.step_idx(),
            step_id: if step_result.step_id().is_empty() {
                None
            } else {
                Some(step_result.step_id().to_string())
            },
            result: step_result.result().clone(),
        }
    }
}
