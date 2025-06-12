use poem_openapi::{OpenApi, payload::Json};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_core::status::ExecutionStatus;
use stepflow_core::{
    FlowResult,
    workflow::{FlowRef, ValueRef},
};
use stepflow_execution::StepFlowExecutor;
use uuid::Uuid;

use super::api_type::ApiType;
use super::common::{ApiExecuteResponse, ExecuteResponse};
use super::error::{IntoPoemError as _, ServerError, ServerResult};
use error_stack::ResultExt as _;

/// Request to execute a workflow ad-hoc
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExecuteRequest {
    /// The workflow to execute
    pub workflow: FlowRef,
    /// Input data for the workflow
    pub input: ValueRef,
    /// Whether to run in debug mode (pauses execution for step-by-step control)
    #[serde(default)]
    pub debug: bool,
}

/// API wrapper for ExecuteRequest
pub type ApiExecuteRequest = ApiType<ExecuteRequest>;

/// Request to execute specific steps in debug mode
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DebugStepRequest {
    /// Step IDs to execute
    pub step_ids: Vec<String>,
}

/// API wrapper for DebugStepRequest
pub type ApiDebugStepRequest = ApiType<DebugStepRequest>;

/// Response from debug step executions
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DebugStepResponse {
    /// Results of executed steps
    pub results: std::collections::HashMap<String, FlowResult>,
}

/// API wrapper for DebugStepResponse
pub type ApiDebugStepResponse = ApiType<DebugStepResponse>;

/// Response for runnable steps in debug mode
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DebugRunnableResponse {
    /// Steps that can be executed
    pub runnable_steps: Vec<String>,
}

/// API wrapper for DebugRunnableResponse
pub type ApiDebugRunnableResponse = ApiType<DebugRunnableResponse>;

pub struct ExecutionApi {
    executor: Arc<StepFlowExecutor>,
}

impl ExecutionApi {
    pub fn new(executor: Arc<StepFlowExecutor>) -> Self {
        Self { executor }
    }
}

#[OpenApi]
impl ExecutionApi {
    /// Execute a workflow ad-hoc
    #[oai(path = "/execute", method = "post")]
    pub async fn execute(
        &self,
        req: Json<ApiExecuteRequest>,
    ) -> ServerResult<Json<ApiExecuteResponse>> {
        let execution_id = Uuid::new_v4();
        let state_store = self.executor.state_store();

        // Store the workflow in the state store
        let workflow = req.0.workflow.as_flow();
        let workflow_hash = state_store
            .store_workflow(workflow)
            .await
            .change_context(ServerError::WorkflowStoreFailed)
            .into_poem()?;

        // Store input as blob
        let input_blob_id = Some(
            state_store
                .put_blob(req.0.input.clone())
                .await
                .change_context(ServerError::InputStoreFailed)
                .into_poem()?,
        );

        // Create execution record
        state_store
            .create_execution(
                execution_id,
                None, // No endpoint name for ad-hoc execution
                None, // No endpoint label for ad-hoc execution
                Some(&workflow_hash),
                req.0.debug,
                input_blob_id.as_ref(),
            )
            .await
            .change_context(ServerError::ExecutionRecordFailed)
            .into_poem()?;

        // Execute the workflow using the Context trait methods
        use stepflow_plugin::Context as _;

        let flow_arc = req.0.workflow.clone().into_arc();

        if req.0.debug {
            // In debug mode, return immediately without executing
            // The execution will be controlled via debug endpoints
            let _ = state_store
                .update_execution_status(execution_id, ExecutionStatus::Running, None)
                .await;

            return Ok(Json(ApiExecuteResponse::new(ExecuteResponse {
                execution_id: execution_id.to_string(),
                result: None,
                status: ExecutionStatus::Running,
                debug: true,
            })));
        }

        // Submit the workflow for execution
        let submitted_execution_id = self
            .executor
            .submit_flow(flow_arc, req.0.input.clone())
            .await
            .change_context(ServerError::WorkflowSubmissionFailed)
            .into_poem()?;

        // Wait for the result (synchronous execution for the HTTP endpoint)
        let flow_result = match self.executor.flow_result(submitted_execution_id).await {
            Ok(result) => result,
            Err(err) => {
                // Update execution status to failed
                let _ = state_store
                    .update_execution_status(execution_id, ExecutionStatus::Failed, None)
                    .await;

                return Err(err.into_poem());
            }
        };

        // Check if the workflow execution was successful
        match &flow_result {
            stepflow_core::FlowResult::Success { .. } => {
                // Update execution status to completed
                let _ = state_store
                    .update_execution_status(
                        execution_id,
                        ExecutionStatus::Completed,
                        None, // TODO: Store result as blob
                    )
                    .await;

                Ok(Json(ApiExecuteResponse::new(ExecuteResponse {
                    execution_id: execution_id.to_string(),
                    result: Some(flow_result),
                    status: ExecutionStatus::Completed,
                    debug: false,
                })))
            }
            stepflow_core::FlowResult::Failed { .. } | stepflow_core::FlowResult::Skipped => {
                // Update execution status to failed
                let _ = state_store
                    .update_execution_status(execution_id, ExecutionStatus::Failed, None)
                    .await;

                Ok(Json(ApiExecuteResponse::new(ExecuteResponse {
                    execution_id: execution_id.to_string(),
                    result: Some(flow_result),
                    status: ExecutionStatus::Failed,
                    debug: false,
                })))
            }
        }
    }

    /// Execute specific steps in debug mode
    #[oai(path = "/executions/:execution_id/debug/step", method = "post")]
    pub async fn debug_execute_step(
        &self,
        execution_id: poem::web::Path<String>,
        req: Json<ApiDebugStepRequest>,
    ) -> ServerResult<Json<ApiDebugStepResponse>> {
        use error_stack::ResultExt as _;
        use std::collections::HashMap;

        let state_store = self.executor.state_store();

        // Parse UUID from string
        let uuid = Uuid::parse_str(&execution_id.0).map_err(|_| {
            use super::error::invalid_uuid;
            invalid_uuid(&execution_id.0)
        })?;

        // Get execution details to ensure it's in debug mode
        let execution = state_store.get_execution(uuid).await.into_poem()?;
        if !execution.debug_mode {
            use super::error::bad_request;
            return Err(bad_request("Execution is not in debug mode"));
        }

        // Get the workflow hash
        let workflow_hash = execution.workflow_hash.ok_or_else(|| {
            use super::error::not_found;
            not_found("No workflow hash available for this execution")
        })?;

        // Get the workflow from the state store
        let workflow = state_store.get_workflow(&workflow_hash).await.into_poem()?;
        let workflow_arc = std::sync::Arc::new(workflow);

        // Get input for debug session
        let input = match execution.input_blob_id {
            Some(blob_id) => state_store.get_blob(&blob_id).await.into_poem()?,
            None => stepflow_core::workflow::ValueRef::new(serde_json::Value::Null),
        };

        // Create debug session
        let mut debug_session = stepflow_execution::DebugSession::new_with_execution_id(
            self.executor.clone(),
            workflow_arc,
            input,
            state_store.clone(),
            uuid,
        )
        .await
        .change_context(super::error::ServerError::DebugSessionCreationFailed)
        .into_poem()?;

        // Execute the requested steps
        let step_results = debug_session
            .execute_multiple_steps(&req.0.0.step_ids)
            .await
            .change_context(super::error::ServerError::DebugStepExecutionFailed)
            .into_poem()?;

        // Convert to response format
        let mut results = HashMap::new();
        for result in step_results {
            results.insert(result.step_id.clone(), result.result);
        }

        Ok(Json(ApiType(DebugStepResponse { results })))
    }

    /// Continue debug execution to completion
    #[oai(path = "/executions/:execution_id/debug/continue", method = "post")]
    pub async fn debug_continue(
        &self,
        execution_id: poem::web::Path<String>,
    ) -> ServerResult<Json<ApiExecuteResponse>> {
        use error_stack::ResultExt as _;

        let state_store = self.executor.state_store();

        // Parse UUID from string
        let uuid = Uuid::parse_str(&execution_id.0).map_err(|_| {
            use super::error::invalid_uuid;
            invalid_uuid(&execution_id.0)
        })?;

        // Get execution details to ensure it's in debug mode
        let execution = state_store.get_execution(uuid).await.into_poem()?;
        if !execution.debug_mode {
            use super::error::bad_request;
            return Err(bad_request("Execution is not in debug mode"));
        }

        // Get the workflow hash
        let workflow_hash = execution.workflow_hash.ok_or_else(|| {
            use super::error::not_found;
            not_found("No workflow hash available for this execution")
        })?;

        // Get the workflow from the state store
        let workflow = state_store.get_workflow(&workflow_hash).await.into_poem()?;
        let workflow_arc = std::sync::Arc::new(workflow);

        // Get input for debug session
        let input = match execution.input_blob_id {
            Some(blob_id) => state_store.get_blob(&blob_id).await.into_poem()?,
            None => stepflow_core::workflow::ValueRef::new(serde_json::Value::Null),
        };

        // Create debug session
        let mut debug_session = stepflow_execution::DebugSession::new_with_execution_id(
            self.executor.clone(),
            workflow_arc,
            input,
            state_store.clone(),
            uuid,
        )
        .await
        .change_context(super::error::ServerError::DebugSessionCreationFailed)
        .into_poem()?;

        // Continue execution to completion
        let (_executed_steps, final_result) = debug_session
            .continue_to_end()
            .await
            .change_context(super::error::ServerError::DebugContinueFailed)
            .into_poem()?;

        // Update execution status
        let final_status = match &final_result {
            stepflow_core::FlowResult::Success { .. } => ExecutionStatus::Completed,
            stepflow_core::FlowResult::Failed { .. } | stepflow_core::FlowResult::Skipped => {
                ExecutionStatus::Failed
            }
        };

        let _ = state_store
            .update_execution_status(uuid, final_status, None)
            .await;

        Ok(Json(ApiType(ExecuteResponse {
            execution_id: execution_id.0,
            result: Some(final_result),
            status: final_status,
            debug: true,
        })))
    }

    /// Get runnable steps in debug mode
    #[oai(path = "/executions/:execution_id/debug/runnable", method = "get")]
    pub async fn debug_get_runnable(
        &self,
        execution_id: poem::web::Path<String>,
    ) -> ServerResult<Json<ApiDebugRunnableResponse>> {
        use error_stack::ResultExt as _;

        let state_store = self.executor.state_store();

        // Parse UUID from string
        let uuid = Uuid::parse_str(&execution_id.0).map_err(|_| {
            use super::error::invalid_uuid;
            invalid_uuid(&execution_id.0)
        })?;

        // Get execution details to ensure it's in debug mode
        let execution = state_store.get_execution(uuid).await.into_poem()?;
        if !execution.debug_mode {
            use super::error::bad_request;
            return Err(bad_request("Execution is not in debug mode"));
        }

        // Get the workflow hash
        let workflow_hash = execution.workflow_hash.ok_or_else(|| {
            use super::error::not_found;
            not_found("No workflow hash available for this execution")
        })?;

        // Get the workflow from the state store
        let workflow = state_store.get_workflow(&workflow_hash).await.into_poem()?;
        let workflow_arc = std::sync::Arc::new(workflow);

        // Get input for debug session
        let input = match execution.input_blob_id {
            Some(blob_id) => state_store.get_blob(&blob_id).await.into_poem()?,
            None => stepflow_core::workflow::ValueRef::new(serde_json::Value::Null),
        };

        // Create debug session
        let debug_session = stepflow_execution::DebugSession::new_with_execution_id(
            self.executor.clone(),
            workflow_arc,
            input,
            state_store.clone(),
            uuid,
        )
        .await
        .change_context(super::error::ServerError::DebugSessionCreationFailed)
        .into_poem()?;

        // Get runnable steps
        let runnable_steps = debug_session.get_runnable_steps();
        let runnable_step_ids: Vec<String> =
            runnable_steps.into_iter().map(|step| step.id).collect();

        Ok(Json(ApiType(DebugRunnableResponse {
            runnable_steps: runnable_step_ids,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;
    use stepflow_core::workflow::{Component, ErrorAction, Flow, FlowRef, Step, ValueRef};
    use stepflow_execution::StepFlowExecutor;
    use stepflow_state::InMemoryStateStore;
    use url::Url;

    /// Helper to create a test executor for testing
    async fn create_test_executor() -> Arc<StepFlowExecutor> {
        let state_store = Arc::new(InMemoryStateStore::new());
        StepFlowExecutor::new(state_store)
    }

    /// Helper to create a simple test workflow
    fn create_test_workflow() -> Flow {
        Flow {
            name: Some("test_workflow".to_string()),
            description: Some("Test workflow for API testing".to_string()),
            version: None,
            input_schema: None,
            output_schema: None,
            steps: vec![Step {
                id: "test_step".to_string(),
                component: Component::new(Url::parse("builtin://messages").unwrap()),
                input: ValueRef::new(json!("Hello from test")),
                input_schema: None,
                output_schema: None,
                skip_if: None,
                on_error: ErrorAction::Fail,
            }],
            output: serde_json::Value::Null,
            test: None,
        }
    }

    /// Helper to create test input data
    fn create_test_input() -> ValueRef {
        ValueRef::new(json!({
            "message": "test input"
        }))
    }

    #[test]
    fn test_execute_request_serialization() {
        let workflow = create_test_workflow();
        let input = create_test_input();

        let request = ExecuteRequest {
            workflow: FlowRef::new(workflow),
            input,
            debug: false,
        };

        // Should be able to serialize/deserialize
        let json = serde_json::to_string(&request).unwrap();
        let deserialized: ExecuteRequest = serde_json::from_str(&json).unwrap();

        assert!(!deserialized.debug);
        assert_eq!(
            deserialized.workflow.name,
            Some("test_workflow".to_string())
        );
    }

    #[test]
    fn test_execute_request_with_debug() {
        let workflow = create_test_workflow();
        let input = create_test_input();

        let request = ExecuteRequest {
            workflow: FlowRef::new(workflow),
            input,
            debug: true,
        };

        assert!(request.debug);
        assert_eq!(request.workflow.name, Some("test_workflow".to_string()));
    }

    // Note: Debug-related tests have been moved to debug.rs to match the
    // relocation of debug functionality to the dedicated DebugApi.

    #[tokio::test]
    async fn test_execution_api_creation() {
        let executor = create_test_executor().await;
        let api = ExecutionApi::new(executor.clone());

        // Verify the API was created correctly
        assert!(Arc::ptr_eq(&api.executor, &executor));
    }

    // Note: Integration tests for actual execution would require more complex setup
    // with mock plugins and workflows. These tests focus on data structure validation.
}
