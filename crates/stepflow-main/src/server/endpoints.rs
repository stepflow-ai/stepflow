use poem_openapi::{OpenApi, payload::Json};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_core::workflow::{FlowRef, ValueRef, WorkflowGraph};
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
    /// When the endpoint was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the endpoint was last updated
    pub updated_at: chrono::DateTime<chrono::Utc>,
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

/// Response for workflow graph operations
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowGraphResponse {
    /// The workflow hash this graph corresponds to
    pub workflow_hash: String,
    /// The analyzed workflow graph with ports and edges
    pub graph: WorkflowGraph,
}

/// API wrapper for WorkflowGraphResponse
pub type ApiWorkflowGraphResponse = ApiType<WorkflowGraphResponse>;

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

    /// Get the workflow graph (ports and edges) for an endpoint
    #[oai(path = "/endpoints/:name/graph", method = "get")]
    pub async fn get_endpoint_workflow_graph(
        &self,
        name: poem::web::Path<String>,
        label: poem::web::Query<Option<String>>,
    ) -> ServerResult<Json<ApiWorkflowGraphResponse>> {
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

        // Analyze the workflow to extract ports and edges
        let graph = workflow.analyze_ports_and_edges();

        Ok(Json(ApiType(WorkflowGraphResponse {
            workflow_hash: endpoint.workflow_hash,
            graph,
        })))
    }

    /// Get the workflow graph (ports and edges) by workflow hash
    #[oai(path = "/workflows/:workflow_hash/graph", method = "get")]
    pub async fn get_workflow_graph(
        &self,
        workflow_hash: poem::web::Path<String>,
    ) -> ServerResult<Json<ApiWorkflowGraphResponse>> {
        let state_store = self.executor.state_store();

        // Get the workflow from the state store
        let workflow = state_store
            .get_workflow(&workflow_hash.0)
            .await
            .into_poem()?;

        // Analyze the workflow to extract ports and edges
        let graph = workflow.analyze_ports_and_edges();

        Ok(Json(ApiType(WorkflowGraphResponse {
            workflow_hash: workflow_hash.0,
            graph,
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
            created_at: endpoint.created_at,
            updated_at: endpoint.updated_at,
        }
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
    fn test_create_endpoint_request_serialization() {
        let workflow = create_test_workflow();
        let request = CreateEndpointRequest {
            workflow: FlowRef::new(workflow),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: CreateEndpointRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(
            deserialized.workflow.name,
            Some("test_workflow".to_string())
        );
    }

    #[test]
    fn test_execute_endpoint_request_structure() {
        let input = create_test_input();
        let request = ExecuteEndpointRequest {
            input: input.clone(),
            debug: true,
        };

        assert!(request.debug);
        assert_eq!(request.input.as_ref()["message"], "test input");

        // Test serialization
        let json = serde_json::to_string(&request).unwrap();
        let deserialized: ExecuteEndpointRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.debug, true);
        assert_eq!(deserialized.input.as_ref()["message"], "test input");
    }

    #[test]
    fn test_execute_endpoint_request_defaults() {
        let input = create_test_input();
        
        // Test with explicit false
        let request1 = ExecuteEndpointRequest {
            input: input.clone(),
            debug: false,
        };
        assert!(!request1.debug);

        // Test deserialization with missing debug field (should default to false)
        let json_no_debug = serde_json::json!({
            "input": {"message": "test"}
        });
        let deserialized: ExecuteEndpointRequest = 
            serde_json::from_value(json_no_debug).unwrap();
        assert!(!deserialized.debug);
    }

    #[test]
    fn test_endpoint_response_structure() {
        use chrono::Utc;

        let now = Utc::now();
        let endpoint = Endpoint {
            name: "test_endpoint".to_string(),
            label: Some("v1.0".to_string()),
            workflow_hash: "abc123".to_string(),
            created_at: now,
            updated_at: now,
        };

        let response = EndpointResponse::from(endpoint);

        assert_eq!(response.name, "test_endpoint");
        assert_eq!(response.label, Some("v1.0".to_string()));
        assert_eq!(response.workflow_hash, "abc123");
        
        // Verify timestamps are DateTime types
        assert_eq!(response.created_at, now);
        assert_eq!(response.updated_at, now);
    }

    #[test]
    fn test_endpoint_response_no_label() {
        use chrono::Utc;

        let now = Utc::now();
        let endpoint = Endpoint {
            name: "test_endpoint".to_string(),
            label: None, // No label for default version
            workflow_hash: "abc123".to_string(),
            created_at: now,
            updated_at: now,
        };

        let response = EndpointResponse::from(endpoint);

        assert_eq!(response.name, "test_endpoint");
        assert_eq!(response.label, None);
        assert_eq!(response.workflow_hash, "abc123");
    }

    #[test]
    fn test_list_endpoints_response_structure() {
        let response = ListEndpointsResponse {
            endpoints: vec![
                EndpointResponse {
                    name: "endpoint1".to_string(),
                    label: None,
                    workflow_hash: "hash1".to_string(),
                    created_at: chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z").unwrap().with_timezone(&chrono::Utc),
                    updated_at: chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z").unwrap().with_timezone(&chrono::Utc),
                },
                EndpointResponse {
                    name: "endpoint2".to_string(),
                    label: Some("v1.0".to_string()),
                    workflow_hash: "hash2".to_string(),
                    created_at: chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z").unwrap().with_timezone(&chrono::Utc),
                    updated_at: chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z").unwrap().with_timezone(&chrono::Utc),
                },
            ],
        };

        assert_eq!(response.endpoints.len(), 2);
        assert_eq!(response.endpoints[0].name, "endpoint1");
        assert_eq!(response.endpoints[0].label, None);
        assert_eq!(response.endpoints[1].name, "endpoint2");
        assert_eq!(response.endpoints[1].label, Some("v1.0".to_string()));

        // Test serialization
        let json = serde_json::to_string(&response).unwrap();
        let deserialized: ListEndpointsResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.endpoints.len(), 2);
    }

    #[tokio::test]
    async fn test_endpoints_api_creation() {
        let executor = create_test_executor().await;
        let api = EndpointsApi::new(executor.clone());

        // Verify the API was created correctly
        assert!(Arc::ptr_eq(&api.executor, &executor));
    }

    #[test]
    fn test_workflow_graph_response_structure() {
        use stepflow_core::workflow::{WorkflowGraph, StepPorts, Port};

        let graph = WorkflowGraph::new()
            .with_workflow_ports(
                StepPorts::new()
                    .add_input(Port::new("input"))
                    .add_output(Port::new("output"))
            );

        let response = WorkflowGraphResponse {
            workflow_hash: "abc123".to_string(),
            graph: graph.clone(),
        };

        assert_eq!(response.workflow_hash, "abc123");
        assert_eq!(response.graph.workflow_ports.inputs.len(), 1);
        assert_eq!(response.graph.workflow_ports.outputs.len(), 1);
        assert_eq!(response.graph.workflow_ports.inputs[0].name, "input");
        assert_eq!(response.graph.workflow_ports.outputs[0].name, "output");

        // Test serialization
        let json = serde_json::to_string(&response).unwrap();
        let deserialized: WorkflowGraphResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.workflow_hash, "abc123");
        assert_eq!(deserialized.graph.workflow_ports.inputs.len(), 1);
        assert_eq!(deserialized.graph.workflow_ports.outputs.len(), 1);
    }

    // Note: Integration tests for actual endpoint operations would require more complex setup
    // with state store operations. These tests focus on data structure validation.
}
