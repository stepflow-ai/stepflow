use serde_json::Value;
use utoipa::OpenApi;

/// Test that validates the OpenAPI schema generation
#[tokio::test]
async fn test_openapi_schema_generation() {
    use stepflow_server::{
        components::ComponentsApi, debug::DebugApi, executions::ExecutionsApi, health::HealthApi,
        workflows::WorkflowApi,
    };

    // Generate OpenAPI specs for each API
    let health_spec = HealthApi::openapi();
    let workflow_spec = WorkflowApi::openapi();
    let executions_spec = ExecutionsApi::openapi();
    let debug_spec = DebugApi::openapi();
    let components_spec = ComponentsApi::openapi();

    // Validate that each spec has required fields
    assert_eq!(health_spec.info.title, "stepflow-server");
    assert!(!health_spec.paths.paths.is_empty());

    assert_eq!(workflow_spec.info.title, "stepflow-server");
    assert!(!workflow_spec.paths.paths.is_empty());

    assert_eq!(executions_spec.info.title, "stepflow-server");
    assert!(!executions_spec.paths.paths.is_empty());

    assert_eq!(debug_spec.info.title, "stepflow-server");
    assert!(!debug_spec.paths.paths.is_empty());

    assert_eq!(components_spec.info.title, "stepflow-server");
    assert!(!components_spec.paths.paths.is_empty());
}

#[tokio::test]
async fn test_combined_openapi_schema() {
    use utoipa::OpenApi;

    #[derive(OpenApi)]
    #[openapi(
        nest(
            (path = "/api/v1", api = stepflow_server::health::HealthApi),
            (path = "/api/v1", api = stepflow_server::workflows::WorkflowApi),
            (path = "/api/v1", api = stepflow_server::executions::ExecutionsApi),
            (path = "/api/v1", api = stepflow_server::debug::DebugApi),
            (path = "/api/v1", api = stepflow_server::components::ComponentsApi)
        ),
        info(
            title = "StepFlow API",
            version = "1.0.0",
            description = "REST API for StepFlow workflow execution"
        )
    )]
    struct TestApiDoc;

    let combined_spec = TestApiDoc::openapi();

    // Validate combined spec structure
    assert_eq!(combined_spec.info.title, "StepFlow API");
    assert_eq!(combined_spec.info.version, "1.0.0");
    assert_eq!(
        combined_spec.info.description,
        Some("REST API for StepFlow workflow execution".to_string())
    );

    // Check that all expected paths are present
    let paths = &combined_spec.paths.paths;

    // Health endpoint
    assert!(paths.contains_key("/api/v1/health"));

    // Workflow endpoints
    assert!(paths.contains_key("/api/v1/workflows"));
    assert!(paths.contains_key("/api/v1/workflows/{workflow_hash}"));
    assert!(paths.contains_key("/api/v1/workflows/{workflow_hash}/dependencies"));

    // Named workflow endpoints
    assert!(paths.contains_key("/api/v1/workflows/names"));
    assert!(paths.contains_key("/api/v1/workflows/by-name/{name}"));
    assert!(paths.contains_key("/api/v1/workflows/by-name/{name}/latest"));
    assert!(paths.contains_key("/api/v1/workflows/by-name/{name}/execute"));
    assert!(paths.contains_key("/api/v1/workflows/by-name/{name}/labels"));
    assert!(paths.contains_key("/api/v1/workflows/by-name/{name}/labels/{label}"));
    assert!(paths.contains_key("/api/v1/workflows/by-name/{name}/labels/{label}/execute"));
    assert!(paths.contains_key("/api/v1/workflows/{workflow_hash}/execute"));

    // Execution tracking endpoints
    assert!(paths.contains_key("/api/v1/executions"));
    assert!(paths.contains_key("/api/v1/executions/{execution_id}"));
    assert!(paths.contains_key("/api/v1/executions/{execution_id}/workflow"));
    assert!(paths.contains_key("/api/v1/executions/{execution_id}/steps"));

    // Ad-hoc execution endpoints (now under /executions)
    assert!(paths.contains_key("/api/v1/executions"));

    // Debug execution endpoints
    assert!(paths.contains_key("/api/v1/executions/{execution_id}/debug/step"));
    assert!(paths.contains_key("/api/v1/executions/{execution_id}/debug/continue"));
    assert!(paths.contains_key("/api/v1/executions/{execution_id}/debug/runnable"));

    // Component endpoints
    assert!(paths.contains_key("/api/v1/components"));
}

#[tokio::test]
async fn test_openapi_schema_serialization() {
    use utoipa::OpenApi;

    #[derive(OpenApi)]
    #[openapi(
        nest(
            (path = "/api/v1", api = stepflow_server::health::HealthApi),
            (path = "/api/v1", api = stepflow_server::workflows::WorkflowApi),
            (path = "/api/v1", api = stepflow_server::executions::ExecutionsApi),
            (path = "/api/v1", api = stepflow_server::debug::DebugApi),
            (path = "/api/v1", api = stepflow_server::components::ComponentsApi)
        ),
        info(
            title = "StepFlow API",
            version = "1.0.0",
            description = "REST API for StepFlow workflow execution"
        )
    )]
    struct TestApiDoc;

    let spec = TestApiDoc::openapi();

    // Test that the schema can be serialized to JSON
    let json_result = serde_json::to_string_pretty(&spec);
    assert!(json_result.is_ok());

    let json_string = json_result.unwrap();
    assert!(!json_string.is_empty());

    // Test that the JSON can be parsed back
    let parsed_result: Result<Value, _> = serde_json::from_str(&json_string);
    assert!(parsed_result.is_ok());

    let parsed_spec = parsed_result.unwrap();

    // Validate key fields in the parsed JSON
    assert_eq!(parsed_spec["info"]["title"], "StepFlow API");
    assert_eq!(parsed_spec["info"]["version"], "1.0.0");
    assert!(parsed_spec["paths"].is_object());
    assert!(parsed_spec["components"].is_object());
}

#[tokio::test]
async fn test_schema_components_exist() {
    use utoipa::OpenApi;

    #[derive(OpenApi)]
    #[openapi(
        nest(
            (path = "/api/v1", api = stepflow_server::health::HealthApi),
            (path = "/api/v1", api = stepflow_server::workflows::WorkflowApi),
            (path = "/api/v1", api = stepflow_server::executions::ExecutionsApi),
            (path = "/api/v1", api = stepflow_server::debug::DebugApi),
            (path = "/api/v1", api = stepflow_server::components::ComponentsApi)
        ),
        info(
            title = "StepFlow API",
            version = "1.0.0",
            description = "REST API for StepFlow workflow execution"
        )
    )]
    struct TestApiDoc;

    let spec = TestApiDoc::openapi();
    let components = &spec.components;

    assert!(components.is_some());
    let schemas = &components.as_ref().unwrap().schemas;

    // Check that key schema components are present
    let expected_schemas = vec![
        "HealthResponse",
        "StoreWorkflowRequest",
        "StoreWorkflowResponse",
        "WorkflowResponse",
        "WorkflowDependenciesResponse",
        "CreateLabelRequest",
        "WorkflowLabelResponse",
        "ExecuteWorkflowRequest",
        "CreateExecutionResponse",
        "ExecutionSummaryResponse",
        "ExecutionDetailsResponse",
        "ListExecutionsResponse",
        "StepExecutionResponse",
        "ListStepExecutionsResponse",
        "CreateExecutionRequest",
        "DebugStepRequest",
        "DebugStepResponse",
        "DebugRunnableResponse",
        "ComponentInfoResponse",
        "ListComponentsResponse",
    ];

    for schema_name in expected_schemas {
        assert!(
            schemas.contains_key(schema_name),
            "Missing schema: {}",
            schema_name
        );
    }
}

#[tokio::test]
async fn test_http_methods_in_openapi() {
    use utoipa::OpenApi;

    #[derive(OpenApi)]
    #[openapi(
        nest(
            (path = "/api/v1", api = stepflow_server::health::HealthApi),
            (path = "/api/v1", api = stepflow_server::workflows::WorkflowApi),
            (path = "/api/v1", api = stepflow_server::executions::ExecutionsApi),
            (path = "/api/v1", api = stepflow_server::debug::DebugApi),
            (path = "/api/v1", api = stepflow_server::components::ComponentsApi)
        ),
        info(
            title = "StepFlow API",
            version = "1.0.0",
            description = "REST API for StepFlow workflow execution"
        )
    )]
    struct TestApiDoc;

    let spec = TestApiDoc::openapi();
    let paths = &spec.paths.paths;

    // Test that specific paths have the expected HTTP methods

    // Health endpoint - GET
    let health_path = paths.get("/api/v1/health").unwrap();
    assert!(health_path.get.is_some());
    assert!(health_path.post.is_none());
    assert!(health_path.put.is_none());
    assert!(health_path.delete.is_none());

    // Workflows - GET and POST
    let workflows_path = paths.get("/api/v1/workflows").unwrap();
    assert!(workflows_path.get.is_some());
    assert!(workflows_path.post.is_some());
    assert!(workflows_path.put.is_none());

    // Specific workflow - GET and DELETE
    let workflow_hash_path = paths.get("/api/v1/workflows/{workflow_hash}").unwrap();
    assert!(workflow_hash_path.get.is_some());
    assert!(workflow_hash_path.delete.is_some());
    assert!(workflow_hash_path.post.is_none());

    // Named workflows - GET for specific workflow name
    let workflow_name_path = paths.get("/api/v1/workflows/by-name/{name}").unwrap();
    assert!(workflow_name_path.get.is_some());
    assert!(workflow_name_path.post.is_none());

    // Workflow labels - PUT and DELETE for specific label
    let workflow_label_path = paths
        .get("/api/v1/workflows/by-name/{name}/labels/{label}")
        .unwrap();
    assert!(workflow_label_path.get.is_some());
    assert!(workflow_label_path.put.is_some());
    assert!(workflow_label_path.delete.is_some());

    // Named workflow execution - POST
    let workflow_execute_path = paths
        .get("/api/v1/workflows/by-name/{name}/execute")
        .unwrap();
    assert!(workflow_execute_path.post.is_some());
    assert!(workflow_execute_path.get.is_none());

    // Ad-hoc execution - POST (now under /executions)
    let executions_path = paths.get("/api/v1/executions").unwrap();
    assert!(executions_path.post.is_some());
    assert!(executions_path.get.is_some()); // Also supports GET for listing

    // Components - GET
    let components_path = paths.get("/api/v1/components").unwrap();
    assert!(components_path.get.is_some());
    assert!(components_path.post.is_none());
}

#[tokio::test]
async fn test_openapi_response_schemas() {
    use utoipa::OpenApi;

    #[derive(OpenApi)]
    #[openapi(
        nest(
            (path = "/api/v1", api = stepflow_server::health::HealthApi),
            (path = "/api/v1", api = stepflow_server::workflows::WorkflowApi),
            (path = "/api/v1", api = stepflow_server::executions::ExecutionsApi),
            (path = "/api/v1", api = stepflow_server::debug::DebugApi),
            (path = "/api/v1", api = stepflow_server::components::ComponentsApi)
        ),
        info(
            title = "StepFlow API",
            version = "1.0.0",
            description = "REST API for StepFlow workflow execution"
        )
    )]
    struct TestApiDoc;

    let spec = TestApiDoc::openapi();
    let paths = &spec.paths.paths;

    // Test that endpoints have proper response definitions

    // Health endpoint should have 200 response
    let health_path = paths.get("/api/v1/health").unwrap();
    let health_get = health_path.get.as_ref().unwrap();
    assert!(health_get.responses.responses.contains_key("200"));

    // Workflows POST should have 200 response
    let workflows_path = paths.get("/api/v1/workflows").unwrap();
    let workflows_post = workflows_path.post.as_ref().unwrap();
    assert!(workflows_post.responses.responses.contains_key("200"));

    // Workflow names GET should have 200 response
    let workflow_names_path = paths.get("/api/v1/workflows/names").unwrap();
    let workflow_names_get = workflow_names_path.get.as_ref().unwrap();
    assert!(workflow_names_get.responses.responses.contains_key("200"));

    // Executions GET should have 200 response
    let executions_path = paths.get("/api/v1/executions").unwrap();
    let executions_get = executions_path.get.as_ref().unwrap();
    assert!(executions_get.responses.responses.contains_key("200"));

    // Create execution POST should have 200 response (now under /executions)
    let create_execution_path = paths.get("/api/v1/executions").unwrap();
    let create_execution_post = create_execution_path.post.as_ref().unwrap();
    assert!(
        create_execution_post
            .responses
            .responses
            .contains_key("200")
    );

    // Components GET should have 200 response
    let components_path = paths.get("/api/v1/components").unwrap();
    let components_get = components_path.get.as_ref().unwrap();
    assert!(components_get.responses.responses.contains_key("200"));
}
