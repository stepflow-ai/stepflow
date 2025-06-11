use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_analysis::StepDependency;
use stepflow_core::status::ExecutionStatus;
use stepflow_core::workflow::{Flow, FlowHash, ValueRef};
use stepflow_execution::StepFlowExecutor;
use stepflow_state::Endpoint;
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

/// Request to create or update an endpoint
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateEndpointRequest {
    /// The workflow to associate with this endpoint
    pub workflow: Arc<Flow>,
}

/// Request to execute an endpoint
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExecuteEndpointRequest {
    /// Input data for the workflow
    pub input: ValueRef,
    /// Whether to run in debug mode
    #[serde(default)]
    pub debug: bool,
}

/// Response for endpoint operations
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EndpointResponse {
    /// The endpoint name
    pub name: String,
    /// The endpoint label (None for default version)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The workflow hash associated with this endpoint
    pub workflow_hash: FlowHash,
    /// When the endpoint was created
    pub created_at: String, // RFC3339 format
    /// When the endpoint was last updated
    pub updated_at: String, // RFC3339 format
}

/// Response for listing endpoints
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListEndpointsResponse {
    /// List of endpoints
    pub endpoints: Vec<EndpointResponse>,
}

/// Response containing a workflow definition and its hash
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkflowResponse {
    /// The workflow definition
    pub workflow: Arc<Flow>,
    /// The workflow hash
    pub workflow_hash: FlowHash,
}

/// Response for execute operations
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExecuteResponse {
    /// The execution ID
    pub execution_id: String,
    /// The result of the workflow execution (if completed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<stepflow_core::FlowResult>,
    /// The execution status
    pub status: ExecutionStatus,
    /// Whether this execution is in debug mode
    pub debug: bool,
}

/// Response for workflow dependencies
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WorkflowDependenciesResponse {
    /// The workflow hash
    pub workflow_hash: FlowHash,
    /// The workflow dependencies
    pub dependencies: Vec<StepDependency>,
}

/// Query parameters for endpoints with optional label
#[derive(Debug, Deserialize, ToSchema)]
pub struct EndpointQuery {
    /// Optional label for versioned endpoints
    pub label: Option<String>,
}

/// Query parameters for listing endpoints
#[derive(Debug, Deserialize, ToSchema)]
pub struct ListEndpointsQuery {
    /// Optional name filter - if provided, lists all versions of the named endpoint
    pub name: Option<String>,
}

/// Endpoints API
#[derive(OpenApi)]
#[openapi(
    paths(
        create_endpoint,
        list_endpoints,
        get_endpoint,
        get_endpoint_workflow,
        get_endpoint_workflow_dependencies,
        delete_endpoint,
        execute_endpoint
    ),
    components(schemas(
        CreateEndpointRequest,
        ExecuteEndpointRequest,
        EndpointResponse,
        ListEndpointsResponse,
        WorkflowResponse,
        ExecuteResponse,
        WorkflowDependenciesResponse,
        EndpointQuery,
        ListEndpointsQuery
    ))
)]
pub struct EndpointsApi;

/// Create or update a named endpoint with optional label
#[utoipa::path(
    put,
    path = "/endpoints/{name}",
    params(
        ("name" = String, Path, description = "Endpoint name")
    ),
    request_body = CreateEndpointRequest,
    responses(
        (status = 200, description = "Endpoint created or updated successfully", body = EndpointResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn create_endpoint(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(name): Path<String>,
    Query(query): Query<EndpointQuery>,
    Json(req): Json<CreateEndpointRequest>,
) -> Result<Json<EndpointResponse>, StatusCode> {
    let state_store = executor.state_store();

    // Store the workflow in the state store
    let workflow = req.workflow;
    let workflow_hash = Flow::hash(&workflow);

    state_store
        .store_workflow(workflow.clone())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Create or update the endpoint
    state_store
        .create_endpoint(&name, query.label.as_deref(), &workflow_hash.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Retrieve the created endpoint to return
    let endpoint = state_store
        .get_endpoint(&name, query.label.as_deref())
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok(Json(EndpointResponse::from(endpoint)))
}

/// List all endpoints or all versions of a specific endpoint
#[utoipa::path(
    get,
    path = "/endpoints",
    responses(
        (status = 200, description = "Endpoints listed successfully", body = ListEndpointsResponse),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn list_endpoints(
    State(executor): State<Arc<StepFlowExecutor>>,
    Query(query): Query<ListEndpointsQuery>,
) -> Result<Json<ListEndpointsResponse>, StatusCode> {
    let state_store = executor.state_store();

    let endpoints = state_store
        .list_endpoints(query.name.as_deref())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let endpoint_responses: Vec<EndpointResponse> =
        endpoints.into_iter().map(EndpointResponse::from).collect();

    Ok(Json(ListEndpointsResponse {
        endpoints: endpoint_responses,
    }))
}

/// Get endpoint details by name and optional label
#[utoipa::path(
    get,
    path = "/endpoints/{name}",
    params(
        ("name" = String, Path, description = "Endpoint name")
    ),
    responses(
        (status = 200, description = "Endpoint retrieved successfully", body = EndpointResponse),
        (status = 404, description = "Endpoint not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_endpoint(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(name): Path<String>,
    Query(query): Query<EndpointQuery>,
) -> Result<Json<EndpointResponse>, StatusCode> {
    let state_store = executor.state_store();

    let endpoint = state_store
        .get_endpoint(&name, query.label.as_deref())
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok(Json(EndpointResponse::from(endpoint)))
}

/// Get the workflow definition for an endpoint
#[utoipa::path(
    get,
    path = "/endpoints/{name}/workflow",
    params(
        ("name" = String, Path, description = "Endpoint name")
    ),
    responses(
        (status = 200, description = "Endpoint workflow retrieved successfully", body = WorkflowResponse),
        (status = 404, description = "Endpoint not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_endpoint_workflow(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(name): Path<String>,
    Query(query): Query<EndpointQuery>,
) -> Result<Json<WorkflowResponse>, StatusCode> {
    let state_store = executor.state_store();

    // Get the endpoint to retrieve the workflow hash
    let endpoint = state_store
        .get_endpoint(&name, query.label.as_deref())
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    // Get the workflow from the state store
    let workflow = state_store
        .get_workflow(&endpoint.workflow_hash)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok(Json(WorkflowResponse {
        workflow,
        workflow_hash: FlowHash::from(endpoint.workflow_hash.as_str()),
    }))
}

/// Get the workflow dependencies for an endpoint
#[utoipa::path(
    get,
    path = "/endpoints/{name}/dependencies",
    params(
        ("name" = String, Path, description = "Endpoint name")
    ),
    responses(
        (status = 200, description = "Endpoint workflow dependencies retrieved successfully", body = WorkflowDependenciesResponse),
        (status = 404, description = "Endpoint not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_endpoint_workflow_dependencies(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(name): Path<String>,
    Query(query): Query<EndpointQuery>,
) -> Result<Json<WorkflowDependenciesResponse>, StatusCode> {
    let state_store = executor.state_store();

    // Get the endpoint to retrieve the workflow hash
    let endpoint = state_store
        .get_endpoint(&name, query.label.as_deref())
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    // Get the workflow from the state store
    let workflow = state_store
        .get_workflow(&endpoint.workflow_hash)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    // TODO: We should cache the analysis based on the workflow ID.
    let workflow_hash = FlowHash::from(endpoint.workflow_hash.as_str());
    let analysis =
        stepflow_analysis::analyze_workflow_dependencies(workflow, workflow_hash.clone())
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(WorkflowDependenciesResponse {
        workflow_hash,
        dependencies: analysis.dependencies,
    }))
}

/// Delete an endpoint by name and optional label
#[utoipa::path(
    delete,
    path = "/endpoints/{name}",
    params(
        ("name" = String, Path, description = "Endpoint name")
    ),
    responses(
        (status = 200, description = "Endpoint deleted successfully"),
        (status = 404, description = "Endpoint not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn delete_endpoint(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(name): Path<String>,
    Query(query): Query<EndpointQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let state_store = executor.state_store();

    // For non-wildcard deletes, check if endpoint exists first
    if query.label.as_deref() != Some("*")
        && state_store
            .get_endpoint(&name, query.label.as_deref())
            .await
            .is_err()
    {
        return Err(StatusCode::NOT_FOUND);
    }

    // Delete the endpoint(s)
    state_store
        .delete_endpoint(&name, query.label.as_deref())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let message = if query.label.as_deref() == Some("*") {
        format!("All versions of endpoint '{}' deleted successfully", name)
    } else if let Some(ref l) = query.label {
        format!("Endpoint '{}:{}' deleted successfully", name, l)
    } else {
        format!("Endpoint '{}' deleted successfully", name)
    };

    Ok(Json(serde_json::json!({ "message": message })))
}

/// Execute a named endpoint with optional label
#[utoipa::path(
    post,
    path = "/endpoints/{name}/execute",
    params(
        ("name" = String, Path, description = "Endpoint name")
    ),
    request_body = ExecuteEndpointRequest,
    responses(
        (status = 200, description = "Endpoint executed successfully", body = ExecuteResponse),
        (status = 404, description = "Endpoint not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn execute_endpoint(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(name): Path<String>,
    Query(query): Query<EndpointQuery>,
    Json(req): Json<ExecuteEndpointRequest>,
) -> Result<Json<ExecuteResponse>, StatusCode> {
    let state_store = executor.state_store();

    // Get the endpoint to retrieve the workflow hash
    let endpoint = state_store
        .get_endpoint(&name, query.label.as_deref())
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    // Get the workflow from the state store
    let workflow = state_store
        .get_workflow(&endpoint.workflow_hash)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let execution_id = Uuid::new_v4();

    // Store input as blob
    let input_blob_id = Some(
        state_store
            .put_blob(req.input.clone())
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    );

    // Create execution record
    state_store
        .create_execution(
            execution_id,
            Some(&name),            // Include endpoint name
            query.label.as_deref(), // Include endpoint label
            Some(&endpoint.workflow_hash),
            req.debug,
            input_blob_id.as_ref(),
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let debug_mode = req.debug;
    let input = req.input;

    if debug_mode {
        // In debug mode, return immediately without executing
        // The execution will be controlled via debug endpoints
        let _ = state_store
            .update_execution_status(execution_id, ExecutionStatus::Running, None)
            .await;

        return Ok(Json(ExecuteResponse {
            execution_id: execution_id.to_string(),
            result: None,
            status: ExecutionStatus::Running,
            debug: debug_mode,
        }));
    }

    // Execute the workflow using the Context trait methods
    use stepflow_plugin::Context as _;

    // Submit the workflow for execution
    let workflow_hash = FlowHash::from(endpoint.workflow_hash.as_str());
    let submitted_execution_id = executor
        .submit_flow(workflow, workflow_hash, input)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Wait for the result (synchronous execution for the HTTP endpoint)
    let flow_result = match executor.flow_result(submitted_execution_id).await {
        Ok(result) => result,
        Err(_) => {
            // Update execution status to failed
            let _ = state_store
                .update_execution_status(execution_id, ExecutionStatus::Failed, None)
                .await;

            return Err(StatusCode::INTERNAL_SERVER_ERROR);
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

            Ok(Json(ExecuteResponse {
                execution_id: execution_id.to_string(),
                result: Some(flow_result),
                status: ExecutionStatus::Completed,
                debug: debug_mode,
            }))
        }
        stepflow_core::FlowResult::Failed { .. } | stepflow_core::FlowResult::Skipped => {
            // Update execution status to failed
            let _ = state_store
                .update_execution_status(execution_id, ExecutionStatus::Failed, None)
                .await;

            Ok(Json(ExecuteResponse {
                execution_id: execution_id.to_string(),
                result: Some(flow_result),
                status: ExecutionStatus::Failed,
                debug: debug_mode,
            }))
        }
    }
}

// Conversion implementations
impl From<Endpoint> for EndpointResponse {
    fn from(endpoint: Endpoint) -> Self {
        Self {
            name: endpoint.name,
            label: endpoint.label,
            workflow_hash: FlowHash::from(endpoint.workflow_hash.as_str()),
            created_at: endpoint.created_at.to_rfc3339(),
            updated_at: endpoint.updated_at.to_rfc3339(),
        }
    }
}
