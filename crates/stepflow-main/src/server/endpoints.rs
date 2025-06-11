use poem_openapi::{OpenApi, payload::Json};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_core::workflow::{FlowRef, ValueRef};
use stepflow_execution::StepFlowExecutor;
use stepflow_state::Endpoint;
use uuid::Uuid;

use super::api_type::ApiType;
use super::common::{ApiExecuteResponse, ApiWorkflowResponse, ExecuteResponse, WorkflowResponse};
use super::error::{IntoPoemError as _, ServerError, ServerResult, not_found};
use error_stack::ResultExt as _;

/// Request to create or update an endpoint
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateEndpointRequest {
    /// The workflow to associate with this endpoint
    pub workflow: FlowRef,
}

/// API wrapper for CreateEndpointRequest
pub type ApiCreateEndpointRequest = ApiType<CreateEndpointRequest>;

/// Request to execute an endpoint
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExecuteEndpointRequest {
    /// Input data for the workflow
    pub input: ValueRef,
    /// Whether to run in debug mode
    #[serde(default)]
    pub debug: bool,
}

/// API wrapper for ExecuteEndpointRequest
pub type ApiExecuteEndpointRequest = ApiType<ExecuteEndpointRequest>;

/// Response for endpoint operations
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EndpointResponse {
    /// The endpoint name
    pub name: String,
    /// The endpoint label (None for default version)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The workflow hash associated with this endpoint
    pub workflow_hash: String,
    /// When the endpoint was created (ISO 8601 string)
    pub created_at: String,
    /// When the endpoint was last updated (ISO 8601 string)
    pub updated_at: String,
}

/// API wrapper for EndpointResponse
pub type ApiEndpointResponse = ApiType<EndpointResponse>;

/// Response for listing endpoints
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListEndpointsResponse {
    /// List of endpoints
    pub endpoints: Vec<EndpointResponse>,
}

/// API wrapper for ListEndpointsResponse
pub type ApiListEndpointsResponse = ApiType<ListEndpointsResponse>;



pub struct EndpointsApi {
    executor: Arc<StepFlowExecutor>,
}

impl EndpointsApi {
    pub fn new(executor: Arc<StepFlowExecutor>) -> Self {
        Self { executor }
    }
}

#[OpenApi]
impl EndpointsApi {
    /// Create or update a named endpoint with optional label
    #[oai(path = "/endpoints/:name", method = "put")]
    pub async fn create_endpoint(
        &self,
        name: poem::web::Path<String>,
        label: poem::web::Query<Option<String>>,
        req: Json<ApiCreateEndpointRequest>,
    ) -> ServerResult<Json<ApiEndpointResponse>> {
        let state_store = self.executor.state_store();

        // Store the workflow in the state store
        let workflow = req.0.0.workflow.as_flow();
        let workflow_hash = state_store
            .store_workflow(workflow)
            .await
            .change_context(ServerError::WorkflowStoreFailed)
            .into_poem()?;

        // Create or update the endpoint
        state_store
            .create_endpoint(&name.0, label.0.as_deref(), &workflow_hash)
            .await
            .change_context(ServerError::EndpointCreationFailed)
            .into_poem()?;

        // Retrieve the created endpoint to return
        let endpoint = state_store
            .get_endpoint(&name.0, label.0.as_deref())
            .await
            .into_poem()?;
        Ok(Json(ApiType(EndpointResponse::from(endpoint))))
    }

    /// List all endpoints or all versions of a specific endpoint
    #[oai(path = "/endpoints", method = "get")]
    pub async fn list_endpoints(
        &self,
        name: poem::web::Query<Option<String>>,
    ) -> ServerResult<Json<ApiListEndpointsResponse>> {
        let state_store = self.executor.state_store();

        let endpoints = state_store
            .list_endpoints(name.0.as_deref())
            .await
            .into_poem()?;
        let endpoint_responses: Vec<EndpointResponse> =
            endpoints.into_iter().map(EndpointResponse::from).collect();

        Ok(Json(ApiType(ListEndpointsResponse {
            endpoints: endpoint_responses,
        })))
    }

    /// Get endpoint details by name and optional label
    #[oai(path = "/endpoints/:name", method = "get")]
    pub async fn get_endpoint(
        &self,
        name: poem::web::Path<String>,
        label: poem::web::Query<Option<String>>,
    ) -> ServerResult<Json<ApiEndpointResponse>> {
        let state_store = self.executor.state_store();

        let endpoint = state_store
            .get_endpoint(&name.0, label.0.as_deref())
            .await
            .into_poem()?;
        Ok(Json(ApiType(EndpointResponse::from(endpoint))))
    }

    /// Get the workflow definition for an endpoint
    #[oai(path = "/endpoints/:name/workflow", method = "get")]
    pub async fn get_endpoint_workflow(
        &self,
        name: poem::web::Path<String>,
        label: poem::web::Query<Option<String>>,
    ) -> ServerResult<Json<ApiWorkflowResponse>> {
        let state_store = self.executor.state_store();

        // Get the endpoint to retrieve the workflow hash
        let endpoint = state_store
            .get_endpoint(&name.0, label.0.as_deref())
            .await
            .into_poem()?;

        // Get the workflow from the state store
        let workflow = state_store
            .get_workflow(&endpoint.workflow_hash)
            .await
            .into_poem()?;
        use stepflow_core::workflow::FlowRef;
        let flow_ref = FlowRef::new(workflow);

        Ok(Json(ApiType(WorkflowResponse {
            workflow: flow_ref,
            workflow_hash: endpoint.workflow_hash,
        })))
    }

    /// Delete an endpoint by name and optional label
    #[oai(path = "/endpoints/:name", method = "delete")]
    pub async fn delete_endpoint(
        &self,
        name: poem::web::Path<String>,
        label: poem::web::Query<Option<String>>,
    ) -> ServerResult<Json<serde_json::Value>> {
        let state_store = self.executor.state_store();

        // For non-wildcard deletes, check if endpoint exists first
        if label.0.as_deref() != Some("*")
            && state_store
                .get_endpoint(&name.0, label.0.as_deref())
                .await
                .is_err()
        {
            let identifier = if let Some(ref l) = label.0 {
                format!("{}:{}", name.0, l)
            } else {
                name.0.clone()
            };
            return Err(not_found(format!("Endpoint '{}' not found", identifier)));
        }

        // Delete the endpoint(s)
        state_store
            .delete_endpoint(&name.0, label.0.as_deref())
            .await
            .change_context(ServerError::EndpointDeletionFailed)
            .into_poem()?;

        let message = if label.0.as_deref() == Some("*") {
            format!("All versions of endpoint '{}' deleted successfully", name.0)
        } else if let Some(ref l) = label.0 {
            format!("Endpoint '{}:{}' deleted successfully", name.0, l)
        } else {
            format!("Endpoint '{}' deleted successfully", name.0)
        };
        Ok(Json(serde_json::json!({ "message": message })))
    }

    /// Execute a named endpoint with optional label
    #[oai(path = "/endpoints/:name/execute", method = "post")]
    pub async fn execute_endpoint(
        &self,
        name: poem::web::Path<String>,
        label: poem::web::Query<Option<String>>,
        req: Json<ApiExecuteEndpointRequest>,
    ) -> ServerResult<Json<ApiExecuteResponse>> {
        let state_store = self.executor.state_store();

        // Get the endpoint to retrieve the workflow hash
        let endpoint = state_store
            .get_endpoint(&name.0, label.0.as_deref())
            .await
            .into_poem()?;

        // Get the workflow from the state store
        let workflow = state_store
            .get_workflow(&endpoint.workflow_hash)
            .await
            .into_poem()?;

        let execution_id = Uuid::new_v4();

        // Store input as blob
        let input_blob_id = Some(
            state_store
                .put_blob(req.0.0.input.clone())
                .await
                .change_context(ServerError::InputStoreFailed)
                .map_err(|report| report.into_poem())?,
        );

        // Create execution record
        state_store
            .create_execution(
                execution_id,
                Some(&name.0),      // Include endpoint name
                label.0.as_deref(), // Include endpoint label
                Some(&endpoint.workflow_hash),
                req.0.0.debug,
                input_blob_id.as_ref(),
            )
            .await
            .change_context(ServerError::ExecutionRecordFailed)
            .into_poem()?;

        let debug_mode = req.0.0.debug;
        let input = req.0.0.input;

        if debug_mode {
            // In debug mode, return immediately without executing
            // The execution will be controlled via debug endpoints
            let _ = state_store
                .update_execution_status(
                    execution_id,
                    stepflow_state::ExecutionStatus::Running,
                    None,
                )
                .await;

            return Ok(Json(ApiType(ExecuteResponse {
                execution_id: execution_id.to_string(),
                result: None,
                status: stepflow_state::ExecutionStatus::Running,
                debug: debug_mode,
            })));
        }

        // Execute the workflow using the Context trait methods
        use stepflow_plugin::Context as _;

        let flow_arc = Arc::new(workflow);

        // Submit the workflow for execution
        let submitted_execution_id = self
            .executor
            .submit_flow(flow_arc, input)
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

                Ok(Json(ApiType(ExecuteResponse {
                    execution_id: execution_id.to_string(),
                    result: Some(flow_result),
                    status: stepflow_state::ExecutionStatus::Completed,
                    debug: debug_mode,
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

                Ok(Json(ApiType(ExecuteResponse {
                    execution_id: execution_id.to_string(),
                    result: Some(flow_result),
                    status: stepflow_state::ExecutionStatus::Failed,
                    debug: debug_mode,
                })))
            }
        }
    }
}

// Conversion implementations
impl From<Endpoint> for EndpointResponse {
    fn from(endpoint: Endpoint) -> Self {
        Self {
            name: endpoint.name,
            label: endpoint.label,
            workflow_hash: endpoint.workflow_hash,
            created_at: endpoint.created_at.to_rfc3339(),
            updated_at: endpoint.updated_at.to_rfc3339(),
        }
    }
}
