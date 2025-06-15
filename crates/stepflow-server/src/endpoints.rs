use axum::{
    extract::{Path, Query, State},
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

use crate::error::{ErrorResponse, ServerError};

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
    /// All available examples (includes both examples and test cases)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub all_examples: Vec<stepflow_core::workflow::ExampleInput>,
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
) -> Result<Json<EndpointResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    // Store the workflow in the state store
    let workflow = req.workflow;
    let workflow_hash = Flow::hash(&workflow);

    state_store.store_workflow(workflow.clone()).await?;

    // Create or update the endpoint
    state_store
        .create_endpoint(&name, query.label.as_deref(), workflow_hash)
        .await?;

    // Retrieve the created endpoint to return
    let endpoint = state_store
        .get_endpoint(&name, query.label.as_deref())
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::EndpointNotFound(name.clone())))?;

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
) -> Result<Json<ListEndpointsResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    let endpoints = state_store.list_endpoints(query.name.as_deref()).await?;

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
) -> Result<Json<EndpointResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    let endpoint = state_store
        .get_endpoint(&name, query.label.as_deref())
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::EndpointNotFound(name.clone())))?;

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
        (status = 404, description = "Endpoint or workflow not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_endpoint_workflow(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(name): Path<String>,
    Query(query): Query<EndpointQuery>,
) -> Result<Json<WorkflowResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    // Get the endpoint to retrieve the workflow hash
    let endpoint = state_store
        .get_endpoint(&name, query.label.as_deref())
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::EndpointNotFound(name.clone())))?;

    // Get the workflow from the state store
    let workflow = state_store
        .get_workflow(&endpoint.workflow_hash)
        .await?
        .ok_or_else(|| {
            error_stack::report!(ServerError::WorkflowNotFound(
                endpoint.workflow_hash.clone()
            ))
        })?;

    Ok(Json(WorkflowResponse {
        all_examples: workflow.get_all_examples(),
        workflow,
        workflow_hash: endpoint.workflow_hash,
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
        (status = 404, description = "Endpoint or workflow not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_endpoint_workflow_dependencies(
    State(executor): State<Arc<StepFlowExecutor>>,
    Path(name): Path<String>,
    Query(query): Query<EndpointQuery>,
) -> Result<Json<WorkflowDependenciesResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    // Get the endpoint to retrieve the workflow hash
    let endpoint = state_store
        .get_endpoint(&name, query.label.as_deref())
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::EndpointNotFound(name.clone())))?;

    // Get the workflow from the state store
    let workflow = state_store
        .get_workflow(&endpoint.workflow_hash)
        .await?
        .ok_or_else(|| {
            error_stack::report!(ServerError::WorkflowNotFound(
                endpoint.workflow_hash.clone()
            ))
        })?;

    // TODO: We should cache the analysis based on the workflow ID.
    let analysis =
        stepflow_analysis::analyze_workflow_dependencies(workflow, endpoint.workflow_hash.clone())?;

    Ok(Json(WorkflowDependenciesResponse {
        workflow_hash: endpoint.workflow_hash,
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
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let state_store = executor.state_store();

    // For non-wildcard deletes, check if endpoint exists first
    if query.label.as_deref() != Some("*")
        && state_store
            .get_endpoint(&name, query.label.as_deref())
            .await
            .is_err()
    {
        return Err(error_stack::report!(ServerError::EndpointLabelNotFound {
            name,
            label: query.label.clone()
        })
        .into());
    }

    // Delete the endpoint(s)
    state_store
        .delete_endpoint(&name, query.label.as_deref())
        .await?;

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
) -> Result<Json<ExecuteResponse>, ErrorResponse> {
    let state_store = executor.state_store();

    // Get the endpoint to retrieve the workflow hash
    let endpoint = state_store
        .get_endpoint(&name, query.label.as_deref())
        .await?
        .ok_or_else(|| error_stack::report!(ServerError::EndpointNotFound(name.clone())))?;

    // Get the workflow from the state store
    let workflow = state_store
        .get_workflow(&endpoint.workflow_hash)
        .await?
        .ok_or_else(|| {
            error_stack::report!(ServerError::WorkflowNotFound(
                endpoint.workflow_hash.clone()
            ))
        })?;

    let execution_id = Uuid::new_v4();

    // Create execution record
    state_store
        .create_execution(
            execution_id,
            Some(&name),            // Include endpoint name
            query.label.as_deref(), // Include endpoint label
            endpoint.workflow_hash.clone(),
            req.debug,
            req.input.clone(),
        )
        .await?;

    let debug_mode = req.debug;
    let input = req.input;

    if debug_mode {
        // In debug mode, return immediately without executing
        // The execution will be controlled via debug endpoints
        state_store
            .update_execution_status(execution_id, ExecutionStatus::Running, None)
            .await?;

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
    let submitted_execution_id = executor
        .submit_flow(workflow, endpoint.workflow_hash, input)
        .await?;

    // Wait for the result (synchronous execution for the HTTP endpoint)
    let flow_result = executor.flow_result(submitted_execution_id).await?;

    // Check if the workflow execution was successful
    match &flow_result {
        stepflow_core::FlowResult::Success { result } => {
            // Update execution status to completed
            state_store
                .update_execution_status(
                    execution_id,
                    ExecutionStatus::Completed,
                    Some(result.clone()),
                )
                .await?;

            Ok(Json(ExecuteResponse {
                execution_id: execution_id.to_string(),
                result: Some(flow_result),
                status: ExecutionStatus::Completed,
                debug: debug_mode,
            }))
        }
        stepflow_core::FlowResult::Failed { .. } | stepflow_core::FlowResult::Skipped => {
            // Update execution status to failed
            state_store
                .update_execution_status(execution_id, ExecutionStatus::Failed, None)
                .await?;

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
            workflow_hash: endpoint.workflow_hash,
            created_at: endpoint.created_at.to_rfc3339(),
            updated_at: endpoint.updated_at.to_rfc3339(),
        }
    }
}
