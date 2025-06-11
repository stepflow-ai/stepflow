use poem::EndpointExt as _;
use poem::{Route, Server, middleware::Cors};
use poem_openapi::OpenApiService;
use std::sync::Arc;
use stepflow_execution::StepFlowExecutor;

use super::{
    components::ComponentsApi, endpoints::EndpointsApi, execution::ExecutionApi,
    executions::ExecutionsApi, health::HealthApi,
};

/// Start the HTTP server
pub async fn start_server(
    port: u16,
    executor: Arc<StepFlowExecutor>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Create API instances
    let health_api = HealthApi;
    let execution_api = ExecutionApi::new(executor.clone());
    let executions_api = ExecutionsApi::new(executor.clone());
    let endpoints_api = EndpointsApi::new(executor.clone());
    let components_api = ComponentsApi::new(executor.clone());

    let api_service = OpenApiService::new(
        (
            health_api,
            execution_api,
            executions_api,
            endpoints_api,
            components_api,
        ),
        "StepFlow API",
        "1.0.0",
    )
    .description("REST API for StepFlow workflow execution")
    .server(format!("http://localhost:{port}"));

    let spec = api_service.spec_endpoint();

    let app = Route::new()
        .nest("/api/v1", api_service)
        .at("/openapi.json", spec)
        .with(
            Cors::new()
                .allow_origin("http://localhost:3000")
                .allow_methods(vec!["GET", "POST", "PUT", "DELETE", "OPTIONS"])
                .allow_headers(vec!["content-type"]),
        );

    tracing::info!("🚀 StepFlow server starting on http://localhost:{}", port);
    tracing::info!(
        "📖 OpenAPI spec available at http://localhost:{}/openapi.json",
        port
    );

    Server::new(poem::listener::TcpListener::bind(
        format!("0.0.0.0:{port}",),
    ))
    .run(app)
    .await?;

    Ok(())
}
