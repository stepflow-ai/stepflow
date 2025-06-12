use poem_openapi::{OpenApi, payload::Json};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_core::status::ExecutionStatus;
use stepflow_core::{FlowResult, workflow::ValueRef};
use stepflow_execution::StepFlowExecutor;
use stepflow_state::{ExecutionDetails, ExecutionSummary};
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
    /// When the execution was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the execution was completed (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
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
    /// When the execution was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the execution was completed (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
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
    /// Component name/URL that this step executes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    /// Current status of the step
    pub state: stepflow_core::status::StepStatus,
    /// The result of the step execution (if completed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<FlowResult>,
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
        use error_stack::ResultExt as _;
        use std::collections::HashMap;

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

        // Get step results for completed steps
        let step_results = state_store.list_step_results(uuid).await.into_poem()?;
        let mut completed_steps: HashMap<usize, stepflow_state::StepResult<'_>> = HashMap::new();
        for step_result in step_results {
            completed_steps.insert(step_result.step_idx(), step_result);
        }

        // Create unified response with both status and results
        let mut step_responses = Vec::new();

        // Create Arc wrapper for workflow (needed for debug session)
        let workflow_arc = std::sync::Arc::new(workflow);

        // Get step status through WorkflowExecutor (consistent interface for all step info)
        let step_statuses = {
            // Get input for workflow executor
            let input = match execution.input_blob_id {
                Some(blob_id) => state_store.get_blob(&blob_id).await.into_poem()?,
                None => stepflow_core::workflow::ValueRef::new(serde_json::Value::Null),
            };

            // Create workflow executor to get step status
            let workflow_executor = stepflow_execution::WorkflowExecutor::new(
                self.executor.clone(),
                workflow_arc.clone(),
                uuid,
                input,
                state_store.clone(),
            )
            .change_context(super::error::ServerError::DebugSessionCreationFailed)
            .into_poem()?;

            // Use unified interface for step status (works for both debug and non-debug)
            let statuses = workflow_executor.list_all_steps().await;
            let mut status_map = HashMap::new();
            for status in statuses {
                status_map.insert(status.step_index, status.status);
            }
            status_map
        };

        // Build unified responses
        for (idx, step) in workflow_arc.steps.iter().enumerate() {
            let state = step_statuses
                .get(&idx)
                .copied()
                .unwrap_or(stepflow_core::status::StepStatus::Blocked);
            let result = completed_steps.get(&idx).map(|sr| sr.result().clone());

            step_responses.push(StepExecutionResponse {
                step_index: idx,
                step_id: if step.id.is_empty() {
                    None
                } else {
                    Some(step.id.clone())
                },
                component: Some(step.component.to_string()),
                state,
                result,
            });
        }

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
            created_at: summary.created_at,
            completed_at: summary.completed_at,
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
            created_at: details.created_at,
            completed_at: details.completed_at,
        }
    }
}

// Note: StepExecutionResponse is now created manually in get_execution_steps
// to combine both status and results information

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;
    use std::sync::Arc;
    use stepflow_core::{FlowResult, workflow::ValueRef};
    use stepflow_execution::StepFlowExecutor;
    use stepflow_state::{ExecutionDetails, ExecutionSummary, InMemoryStateStore};
    use uuid::Uuid;

    /// Helper to create a test executor for testing
    async fn create_test_executor() -> Arc<StepFlowExecutor> {
        let state_store = Arc::new(InMemoryStateStore::new());
        StepFlowExecutor::new(state_store)
    }

    #[test]
    fn test_execution_summary_response_structure() {
        let now = Utc::now();
        let execution_id = Uuid::new_v4();

        let summary = ExecutionSummary {
            execution_id,
            endpoint_name: Some("test_endpoint".to_string()),
            endpoint_label: None,
            workflow_hash: Some("abc123".to_string()),
            status: ExecutionStatus::Completed,
            debug_mode: false,
            created_at: now,
            completed_at: Some(now),
        };

        let response = ExecutionSummaryResponse::from(summary);

        assert_eq!(response.execution_id, execution_id.to_string());
        assert_eq!(response.endpoint_name, Some("test_endpoint".to_string()));
        assert_eq!(response.workflow_hash, Some("abc123".to_string()));
        assert!(matches!(response.status, ExecutionStatus::Completed));
        assert!(!response.debug_mode);

        // Verify timestamps are DateTime types
        assert_eq!(response.created_at, now);
        assert_eq!(response.completed_at, Some(now));
    }

    #[test]
    fn test_execution_summary_response_no_endpoint() {
        let now = Utc::now();
        let execution_id = Uuid::new_v4();

        let summary = ExecutionSummary {
            execution_id,
            endpoint_name: None, // Ad-hoc execution
            endpoint_label: None,
            workflow_hash: Some("abc123".to_string()),
            status: ExecutionStatus::Running,
            debug_mode: true,
            created_at: now,
            completed_at: None, // Still running
        };

        let response = ExecutionSummaryResponse::from(summary);

        assert_eq!(response.execution_id, execution_id.to_string());
        assert_eq!(response.endpoint_name, None);
        assert_eq!(response.workflow_hash, Some("abc123".to_string()));
        assert!(matches!(response.status, ExecutionStatus::Running));
        assert!(response.debug_mode);
        assert!(response.completed_at.is_none());
    }

    #[test]
    fn test_execution_details_response_structure() {
        let now = Utc::now();
        let execution_id = Uuid::new_v4();

        let details = ExecutionDetails {
            execution_id,
            endpoint_name: Some("test_endpoint".to_string()),
            endpoint_label: None,
            workflow_hash: Some("abc123".to_string()),
            status: ExecutionStatus::Failed,
            debug_mode: false,
            input_blob_id: None,
            result_blob_id: None,
            created_at: now,
            completed_at: Some(now),
        };

        let response = ExecutionDetailsResponse::from(details);

        assert_eq!(response.execution_id, execution_id.to_string());
        assert_eq!(response.endpoint_name, Some("test_endpoint".to_string()));
        assert_eq!(response.workflow_hash, Some("abc123".to_string()));
        assert!(matches!(response.status, ExecutionStatus::Failed));
        assert!(!response.debug_mode);
        assert!(response.input.is_none());
        assert!(response.result.is_none());

        // Verify timestamps are DateTime types
        assert_eq!(response.created_at, now);
        assert_eq!(response.completed_at, Some(now));
    }

    #[test]
    fn test_step_execution_response_structure() {
        let result = FlowResult::Success {
            result: ValueRef::new(json!("test result")),
        };

        let response = StepExecutionResponse {
            step_index: 0,
            step_id: Some("test_step".to_string()),
            component: Some("builtin://test".to_string()),
            state: stepflow_core::status::StepStatus::Completed,
            result: Some(result.clone()),
        };

        assert_eq!(response.step_index, 0);
        assert_eq!(response.step_id, Some("test_step".to_string()));
        assert_eq!(response.component, Some("builtin://test".to_string()));
        assert_eq!(response.state, stepflow_core::status::StepStatus::Completed);
        assert!(matches!(response.result, Some(FlowResult::Success { .. })));

        // Test serialization
        let json = serde_json::to_string(&response).unwrap();
        let deserialized: StepExecutionResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.step_index, 0);
        assert_eq!(deserialized.step_id, Some("test_step".to_string()));
    }

    #[test]
    fn test_step_execution_response_no_step_id() {
        let result = FlowResult::Skipped;

        let response = StepExecutionResponse {
            step_index: 1,
            step_id: None, // No step ID provided
            component: Some("builtin://skip".to_string()),
            state: stepflow_core::status::StepStatus::Skipped,
            result: Some(result.clone()),
        };

        assert_eq!(response.step_index, 1);
        assert_eq!(response.step_id, None);
        assert_eq!(response.component, Some("builtin://skip".to_string()));
        assert_eq!(response.state, stepflow_core::status::StepStatus::Skipped);
        assert!(matches!(response.result, Some(FlowResult::Skipped)));
    }

    #[test]
    fn test_list_executions_response_structure() {
        let now = Utc::now();
        let execution_id1 = Uuid::new_v4();
        let execution_id2 = Uuid::new_v4();

        let response = ListExecutionsResponse {
            executions: vec![
                ExecutionSummaryResponse {
                    execution_id: execution_id1.to_string(),
                    endpoint_name: Some("endpoint1".to_string()),
                    workflow_hash: Some("hash1".to_string()),
                    status: ExecutionStatus::Completed,
                    debug_mode: false,
                    created_at: now,
                    completed_at: Some(now),
                },
                ExecutionSummaryResponse {
                    execution_id: execution_id2.to_string(),
                    endpoint_name: None,
                    workflow_hash: Some("hash2".to_string()),
                    status: ExecutionStatus::Running,
                    debug_mode: true,
                    created_at: now,
                    completed_at: None,
                },
            ],
        };

        assert_eq!(response.executions.len(), 2);
        assert_eq!(
            response.executions[0].execution_id,
            execution_id1.to_string()
        );
        assert_eq!(
            response.executions[0].endpoint_name,
            Some("endpoint1".to_string())
        );
        assert_eq!(
            response.executions[1].execution_id,
            execution_id2.to_string()
        );
        assert_eq!(response.executions[1].endpoint_name, None);

        // Test serialization
        let json = serde_json::to_string(&response).unwrap();
        let deserialized: ListExecutionsResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.executions.len(), 2);
    }

    #[test]
    fn test_list_step_executions_response_structure() {
        let response = ListStepExecutionsResponse {
            steps: vec![
                StepExecutionResponse {
                    step_index: 0,
                    step_id: Some("step1".to_string()),
                    component: Some("builtin://transform".to_string()),
                    state: stepflow_core::status::StepStatus::Completed,
                    result: Some(FlowResult::Success {
                        result: ValueRef::new(json!("result1")),
                    }),
                },
                StepExecutionResponse {
                    step_index: 1,
                    step_id: Some("step2".to_string()),
                    component: Some("builtin://validate".to_string()),
                    state: stepflow_core::status::StepStatus::Skipped,
                    result: Some(FlowResult::Skipped),
                },
            ],
        };

        assert_eq!(response.steps.len(), 2);
        assert_eq!(response.steps[0].step_index, 0);
        assert_eq!(response.steps[0].step_id, Some("step1".to_string()));
        assert_eq!(response.steps[1].step_index, 1);
        assert_eq!(response.steps[1].step_id, Some("step2".to_string()));

        // Test serialization
        let json = serde_json::to_string(&response).unwrap();
        let deserialized: ListStepExecutionsResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.steps.len(), 2);
    }

    #[tokio::test]
    async fn test_executions_api_creation() {
        let executor = create_test_executor().await;
        let api = ExecutionsApi::new(executor.clone());

        // Verify the API was created correctly
        assert!(Arc::ptr_eq(&api.executor, &executor));
    }

    #[test]
    fn test_uuid_parsing() {
        let valid_uuid = "550e8400-e29b-41d4-a716-446655440000";
        let parsed = Uuid::parse_str(valid_uuid);
        assert!(parsed.is_ok());

        let invalid_uuid = "not-a-uuid";
        let parsed = Uuid::parse_str(invalid_uuid);
        assert!(parsed.is_err());
    }

    // Note: Integration tests for actual execution operations would require more complex setup
    // with state store operations. These tests focus on data structure validation.
}
