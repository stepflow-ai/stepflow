use poem_openapi::{OpenApi, payload::Json};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_core::{
    FlowResult,
    workflow::{FlowRef, ValueRef},
};
use stepflow_execution::{StepFlowExecutor, StepStatus};
use uuid::Uuid;

use super::api_type::ApiType;
use super::common::{ApiExecuteResponse, ExecuteResponse};
use super::error::{IntoPoemError as _, ServerError, ServerResult, not_implemented};
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

/// Response for debug state information
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DebugStateResponse {
    /// All step statuses
    pub steps: std::collections::HashMap<String, StepStatus>,
}

/// API wrapper for DebugStateResponse
pub type ApiDebugStateResponse = ApiType<DebugStateResponse>;

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
                .update_execution_status(
                    execution_id,
                    stepflow_state::ExecutionStatus::Running,
                    None,
                )
                .await;

            return Ok(Json(ApiExecuteResponse::new(ExecuteResponse {
                execution_id: execution_id.to_string(),
                result: None,
                status: stepflow_state::ExecutionStatus::Running,
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
                    .update_execution_status(
                        execution_id,
                        stepflow_state::ExecutionStatus::Failed,
                        None,
                    )
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
                        stepflow_state::ExecutionStatus::Completed,
                        None, // TODO: Store result as blob
                    )
                    .await;

                Ok(Json(ApiExecuteResponse::new(ExecuteResponse {
                    execution_id: execution_id.to_string(),
                    result: Some(flow_result),
                    status: stepflow_state::ExecutionStatus::Completed,
                    debug: false,
                })))
            }
            stepflow_core::FlowResult::Failed { .. } | stepflow_core::FlowResult::Skipped => {
                // Update execution status to failed
                let _ = state_store
                    .update_execution_status(
                        execution_id,
                        stepflow_state::ExecutionStatus::Failed,
                        None,
                    )
                    .await;

                Ok(Json(ApiExecuteResponse::new(ExecuteResponse {
                    execution_id: execution_id.to_string(),
                    result: Some(flow_result),
                    status: stepflow_state::ExecutionStatus::Failed,
                    debug: false,
                })))
            }
        }
    }

    /// Execute specific steps in debug mode
    #[oai(path = "/executions/:execution_id/debug/step", method = "post")]
    pub async fn debug_execute_step(
        &self,
        _execution_id: poem::web::Path<String>,
        _req: Json<ApiDebugStepRequest>,
    ) -> ServerResult<Json<ApiDebugStepResponse>> {
        // TODO: Implement debug step execution
        // This would interact with the debug session for the execution
        Err(not_implemented("Debug step execution"))
    }

    /// Continue debug execution to completion
    #[oai(path = "/executions/:execution_id/debug/continue", method = "post")]
    pub async fn debug_continue(
        &self,
        _execution_id: poem::web::Path<String>,
    ) -> ServerResult<Json<ApiExecuteResponse>> {
        // TODO: Implement debug continue
        // This would complete the execution of the debug session
        Err(not_implemented("Debug continue"))
    }

    /// Get runnable steps in debug mode
    #[oai(path = "/executions/:execution_id/debug/runnable", method = "get")]
    pub async fn debug_get_runnable(
        &self,
        _execution_id: poem::web::Path<String>,
    ) -> ServerResult<Json<ApiDebugRunnableResponse>> {
        // TODO: Implement get runnable steps
        // This would return steps that can be executed next
        Err(not_implemented("Debug get runnable"))
    }

    /// Get debug state information
    #[oai(path = "/executions/:execution_id/debug/state", method = "get")]
    pub async fn debug_get_state(
        &self,
        _execution_id: poem::web::Path<String>,
    ) -> ServerResult<Json<ApiDebugStateResponse>> {
        // TODO: Implement get debug state
        // This would return the current state of all steps
        Err(not_implemented("Debug get state"))
    }
}
