use poem_openapi::{OpenApi, payload::Json};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use stepflow_core::{
    FlowResult,
    workflow::{FlowRef, ValueRef},
};
use stepflow_execution::{DebugSession, StepFlowExecutor};
use tokio::sync::Mutex;
use uuid::Uuid;

use super::api_type::ApiType;
use super::common::{ApiResult, ErrorResponse};

/// Request to create a debug session
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateDebugSessionRequest {
    /// The workflow to debug
    pub workflow: FlowRef,
    /// Input data for the workflow
    pub input: ValueRef,
}

/// API wrapper for CreateDebugSessionRequest
pub type ApiCreateDebugSessionRequest = ApiType<CreateDebugSessionRequest>;

/// Response for debug session creation
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DebugSessionResponse {
    /// The debug session ID (UUID string)
    pub session_id: String,
    /// The execution ID for this debug session
    pub execution_id: String,
    /// Current status of the session
    pub status: String,
}

/// API wrapper for DebugSessionResponse
pub type ApiDebugSessionResponse = ApiType<DebugSessionResponse>;

/// Request to execute steps in debug mode
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExecuteStepsRequest {
    /// Step IDs to execute
    pub step_ids: Vec<String>,
}

/// API wrapper for ExecuteStepsRequest
pub type ApiExecuteStepsRequest = ApiType<ExecuteStepsRequest>;

/// Response for step execution in debug mode
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DebugStepExecutionResponse {
    /// The step ID
    pub step_id: String,
    /// Whether the step executed successfully
    pub success: bool,
    /// The result of step execution (if successful)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<FlowResult>,
    /// Error message (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// API wrapper for DebugStepExecutionResponse
pub type ApiDebugStepExecutionResponse = ApiType<DebugStepExecutionResponse>;

/// Debug step status information
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DebugStepStatus {
    /// Step index in the workflow
    pub index: usize,
    /// Step ID
    pub id: String,
    /// Component name
    pub component: String,
    /// Current step state
    pub state: String,
}

/// API wrapper for DebugStepStatus
pub type ApiDebugStepStatus = ApiType<DebugStepStatus>;

/// Debug step inspection details
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DebugStepInspection {
    /// Step index in the workflow
    pub index: usize,
    /// Step ID
    pub id: String,
    /// Component name
    pub component: String,
    /// Step input
    pub input: ValueRef,
    /// Skip condition (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_if: Option<serde_json::Value>,
    /// Error handling action
    pub on_error: String,
    /// Current step state
    pub state: String,
}

/// API wrapper for DebugStepInspection
pub type ApiDebugStepInspection = ApiType<DebugStepInspection>;

/// Response for listing debug steps
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListDebugStepsResponse {
    /// List of step statuses
    pub steps: Vec<DebugStepStatus>,
}

/// API wrapper for ListDebugStepsResponse
pub type ApiListDebugStepsResponse = ApiType<ListDebugStepsResponse>;

/// Response for continue execution
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ContinueExecutionResponse {
    /// Steps that were executed
    pub executed_steps: Vec<DebugStepExecutionResponse>,
    /// Final workflow result
    pub final_result: FlowResult,
}

/// API wrapper for ContinueExecutionResponse
pub type ApiContinueExecutionResponse = ApiType<ContinueExecutionResponse>;

pub struct DebugApi {
    executor: Arc<StepFlowExecutor>,
    debug_sessions: Arc<Mutex<HashMap<Uuid, DebugSession>>>,
}

impl DebugApi {
    pub fn new(executor: Arc<StepFlowExecutor>) -> Self {
        Self {
            executor,
            debug_sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[OpenApi]
impl DebugApi {
    /// Create a new debug session for step-by-step workflow execution
    #[oai(path = "/debug/sessions", method = "post")]
    pub async fn create_debug_session(
        &self,
        req: Json<ApiCreateDebugSessionRequest>,
    ) -> ApiResult<ApiDebugSessionResponse> {
        let state_store = self.executor.state_store();
        let workflow_arc = req.0.0.workflow.into_arc();

        // Create debug session
        let debug_session = match DebugSession::new(
            self.executor.clone(),
            workflow_arc,
            req.0.0.input,
            state_store.clone(),
        )
        .await
        {
            Ok(session) => session,
            Err(_) => {
                return ApiResult::InternalServerError(Json(ApiType(ErrorResponse {
                    code: 500,
                    message: "Failed to create debug session".to_string(),
                    details: None,
                })));
            }
        };

        let session_id = Uuid::new_v4();
        let execution_id = debug_session.execution_id();

        // Store debug session
        {
            let mut sessions = self.debug_sessions.lock().await;
            sessions.insert(session_id, debug_session);
        }

        ApiResult::Ok(Json(ApiType(DebugSessionResponse {
            session_id: session_id.to_string(),
            execution_id: execution_id.to_string(),
            status: "created".to_string(),
        })))
    }

    /// List all steps in a debug session
    #[oai(path = "/debug/sessions/:session_id/steps", method = "get")]
    pub async fn list_debug_steps(
        &self,
        session_id: poem::web::Path<String>,
    ) -> ApiResult<ApiListDebugStepsResponse> {
        let session_uuid = match Uuid::parse_str(&session_id.0) {
            Ok(id) => id,
            Err(_) => {
                return ApiResult::BadRequest(Json(ApiType(ErrorResponse {
                    code: 400,
                    message: "Invalid session ID format".to_string(),
                    details: None,
                })));
            }
        };

        let sessions = self.debug_sessions.lock().await;
        let debug_session = match sessions.get(&session_uuid) {
            Some(session) => session,
            None => {
                return ApiResult::NotFound(Json(ApiType(ErrorResponse {
                    code: 404,
                    message: "Debug session not found".to_string(),
                    details: None,
                })));
            }
        };

        let all_steps = debug_session.list_all_steps().await;
        let debug_steps: Vec<DebugStepStatus> = all_steps
            .into_iter()
            .map(|step| DebugStepStatus {
                index: step.index,
                id: step.id,
                component: step.component,
                state: format!("{:?}", step.state).to_lowercase(),
            })
            .collect();

        ApiResult::Ok(Json(ApiType(ListDebugStepsResponse { steps: debug_steps })))
    }

    /// Get currently runnable steps in a debug session
    #[oai(path = "/debug/sessions/:session_id/runnable", method = "get")]
    pub async fn get_runnable_steps(
        &self,
        session_id: poem::web::Path<String>,
    ) -> ApiResult<ApiListDebugStepsResponse> {
        let session_uuid = match Uuid::parse_str(&session_id.0) {
            Ok(id) => id,
            Err(_) => {
                return ApiResult::BadRequest(Json(ApiType(ErrorResponse {
                    code: 400,
                    message: "Invalid session ID format".to_string(),
                    details: None,
                })));
            }
        };

        let sessions = self.debug_sessions.lock().await;
        let debug_session = match sessions.get(&session_uuid) {
            Some(session) => session,
            None => {
                return ApiResult::NotFound(Json(ApiType(ErrorResponse {
                    code: 404,
                    message: "Debug session not found".to_string(),
                    details: None,
                })));
            }
        };

        let runnable_steps = debug_session.get_runnable_steps();
        let debug_steps: Vec<DebugStepStatus> = runnable_steps
            .into_iter()
            .map(|step| DebugStepStatus {
                index: step.index,
                id: step.id,
                component: step.component,
                state: "runnable".to_string(),
            })
            .collect();

        ApiResult::Ok(Json(ApiType(ListDebugStepsResponse { steps: debug_steps })))
    }

    /// Execute specific steps in a debug session
    #[oai(path = "/debug/sessions/:session_id/execute", method = "post")]
    pub async fn execute_debug_steps(
        &self,
        session_id: poem::web::Path<String>,
        req: Json<ApiExecuteStepsRequest>,
    ) -> ApiResult<Vec<ApiDebugStepExecutionResponse>> {
        let session_uuid = match Uuid::parse_str(&session_id.0) {
            Ok(id) => id,
            Err(_) => {
                return ApiResult::BadRequest(Json(ApiType(ErrorResponse {
                    code: 400,
                    message: "Invalid session ID format".to_string(),
                    details: None,
                })));
            }
        };

        let mut sessions = self.debug_sessions.lock().await;
        let debug_session = match sessions.get_mut(&session_uuid) {
            Some(session) => session,
            None => {
                return ApiResult::NotFound(Json(ApiType(ErrorResponse {
                    code: 404,
                    message: "Debug session not found".to_string(),
                    details: None,
                })));
            }
        };

        let step_results = match debug_session
            .execute_multiple_steps(&req.0.0.step_ids)
            .await
        {
            Ok(results) => results,
            Err(_) => {
                return ApiResult::InternalServerError(Json(ApiType(ErrorResponse {
                    code: 500,
                    message: "Failed to execute steps".to_string(),
                    details: None,
                })));
            }
        };

        let response_steps: Vec<ApiDebugStepExecutionResponse> = step_results
            .into_iter()
            .map(|result| {
                let success = result.is_success();
                ApiType(DebugStepExecutionResponse {
                    step_id: result.step_id,
                    success,
                    result: if success { Some(result.result) } else { None },
                    error: result.error,
                })
            })
            .collect();

        ApiResult::Ok(Json(response_steps))
    }

    /// Continue debug session execution to completion
    #[oai(path = "/debug/sessions/:session_id/continue", method = "post")]
    pub async fn continue_debug_execution(
        &self,
        session_id: poem::web::Path<String>,
    ) -> ApiResult<ApiContinueExecutionResponse> {
        let session_uuid = match Uuid::parse_str(&session_id.0) {
            Ok(id) => id,
            Err(_) => {
                return ApiResult::BadRequest(Json(ApiType(ErrorResponse {
                    code: 400,
                    message: "Invalid session ID format".to_string(),
                    details: None,
                })));
            }
        };

        let mut sessions = self.debug_sessions.lock().await;
        let debug_session = match sessions.get_mut(&session_uuid) {
            Some(session) => session,
            None => {
                return ApiResult::NotFound(Json(ApiType(ErrorResponse {
                    code: 404,
                    message: "Debug session not found".to_string(),
                    details: None,
                })));
            }
        };

        let (executed_steps, final_result) = match debug_session.continue_to_end().await {
            Ok(result) => result,
            Err(_) => {
                return ApiResult::InternalServerError(Json(ApiType(ErrorResponse {
                    code: 500,
                    message: "Failed to continue execution".to_string(),
                    details: None,
                })));
            }
        };

        let executed_step_responses: Vec<DebugStepExecutionResponse> = executed_steps
            .into_iter()
            .map(|(step_id, result)| DebugStepExecutionResponse {
                step_id,
                success: true,
                result: Some(result.clone()),
                error: None,
            })
            .collect();

        ApiResult::Ok(Json(ApiType(ContinueExecutionResponse {
            executed_steps: executed_step_responses,
            final_result,
        })))
    }

    /// Inspect a specific step in a debug session
    #[oai(path = "/debug/sessions/:session_id/steps/:step_id", method = "get")]
    pub async fn inspect_debug_step(
        &self,
        session_id: poem::web::Path<String>,
        step_id: poem::web::Path<String>,
    ) -> ApiResult<ApiDebugStepInspection> {
        let session_uuid = match Uuid::parse_str(&session_id.0) {
            Ok(id) => id,
            Err(_) => {
                return ApiResult::BadRequest(Json(ApiType(ErrorResponse {
                    code: 400,
                    message: "Invalid session ID format".to_string(),
                    details: None,
                })));
            }
        };

        let sessions = self.debug_sessions.lock().await;
        let debug_session = match sessions.get(&session_uuid) {
            Some(session) => session,
            None => {
                return ApiResult::NotFound(Json(ApiType(ErrorResponse {
                    code: 404,
                    message: "Debug session not found".to_string(),
                    details: None,
                })));
            }
        };

        let inspection = match debug_session.inspect_step(&step_id.0).await {
            Ok(inspection) => inspection,
            Err(_) => {
                return ApiResult::NotFound(Json(ApiType(ErrorResponse {
                    code: 404,
                    message: format!("Step '{}' not found", step_id.0),
                    details: None,
                })));
            }
        };

        ApiResult::Ok(Json(ApiType(DebugStepInspection {
            index: inspection.index,
            id: inspection.id,
            component: inspection.component,
            input: inspection.input,
            skip_if: inspection
                .skip_if
                .as_ref()
                .map(|expr| serde_json::to_value(expr).unwrap_or(serde_json::Value::Null)),
            on_error: format!("{:?}", inspection.on_error),
            state: format!("{:?}", inspection.state).to_lowercase(),
        })))
    }

    /// Delete a debug session
    #[oai(path = "/debug/sessions/:session_id", method = "delete")]
    pub async fn delete_debug_session(
        &self,
        session_id: poem::web::Path<String>,
    ) -> ApiResult<serde_json::Value> {
        let session_uuid = match Uuid::parse_str(&session_id.0) {
            Ok(id) => id,
            Err(_) => {
                return ApiResult::BadRequest(Json(ApiType(ErrorResponse {
                    code: 400,
                    message: "Invalid session ID format".to_string(),
                    details: None,
                })));
            }
        };

        let mut sessions = self.debug_sessions.lock().await;
        match sessions.remove(&session_uuid) {
            Some(_) => ApiResult::Ok(Json(serde_json::json!({
                "message": format!("Debug session '{}' deleted successfully", session_id.0)
            }))),
            None => ApiResult::NotFound(Json(ApiType(ErrorResponse {
                code: 404,
                message: "Debug session not found".to_string(),
                details: None,
            }))),
        }
    }
}
