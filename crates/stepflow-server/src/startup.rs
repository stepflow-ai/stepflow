use axum::Router;
use std::sync::Arc;
use stepflow_execution::StepFlowExecutor;
use tower_http::cors::{Any, CorsLayer};
use utoipa_axum::{router::OpenApiRouter, routes};
use utoipa_swagger_ui::SwaggerUi;

use crate::{components, debug, endpoints, executions, health, workflows};

pub struct AppConfig {
    pub include_swagger: bool,
    pub include_cors: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            include_swagger: true,
            include_cors: true,
        }
    }
}

impl AppConfig {
    /// Create the application router with the current configuration
    pub fn create_app_router(&self, executor: Arc<StepFlowExecutor>) -> Router {
        // Create the main API router with state using utoipa-axum for consistency
        let (api_router, api_doc) = OpenApiRouter::new()
            // Health routes.
            .routes(routes!(health::health_check))
            // Workflow routes.
            .routes(routes!(workflows::store_workflow))
            .routes(routes!(workflows::list_workflows))
            .routes(routes!(workflows::get_workflow))
            .routes(routes!(workflows::delete_workflow))
            .routes(routes!(workflows::get_workflow_dependencies))
            // Endpoint routes.
            .routes(routes!(endpoints::create_endpoint))
            .routes(routes!(endpoints::list_endpoints))
            .routes(routes!(endpoints::get_endpoint))
            .routes(routes!(endpoints::get_endpoint_workflow))
            .routes(routes!(endpoints::get_endpoint_workflow_dependencies))
            .routes(routes!(endpoints::delete_endpoint))
            .routes(routes!(endpoints::execute_endpoint))
            // Execution routes.
            .routes(routes!(executions::create_execution))
            .routes(routes!(executions::get_execution))
            .routes(routes!(executions::get_execution_workflow))
            .routes(routes!(executions::list_executions))
            .routes(routes!(executions::get_execution_steps))
            // Execution debugging routes.
            .routes(routes!(debug::debug_execute_step))
            .routes(routes!(debug::debug_continue))
            .routes(routes!(debug::debug_get_runnable))
            // Component routes.
            .routes(routes!(components::list_components))
            .split_for_parts();

        // Add state to the router
        let api_router = api_router.with_state(executor);

        // Create the full app router
        let mut app = Router::new().nest("/api/v1", api_router);

        // Add swagger if requested
        if self.include_swagger {
            app = app.merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", api_doc));
        }

        // Add CORS if requested
        if self.include_cors {
            app = app.layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any),
            );
        }

        app
    }
}

/// Start the HTTP server using axum + utoipa
pub async fn start_server(
    port: u16,
    executor: Arc<StepFlowExecutor>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Create the app with default configuration (includes swagger and CORS)
    let app = AppConfig::default().create_app_router(executor);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;

    tracing::info!("🚀 StepFlow server starting on http://localhost:{}", port);
    tracing::info!(
        "📖 Swagger UI available at http://localhost:{}/swagger-ui",
        port
    );
    tracing::info!(
        "📄 OpenAPI spec available at http://localhost:{}/api-docs/openapi.json",
        port
    );

    axum::serve(listener, app).await?;

    Ok(())
}
