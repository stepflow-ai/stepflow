use axum::Router;
use std::sync::Arc;
use stepflow_execution::StepFlowExecutor;
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use utoipa_swagger_ui::SwaggerUi;

use crate::api::create_api_router;

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
    pub fn create_app_router(&self, executor: Arc<StepFlowExecutor>, port: u16) -> Router {
        // Create the main API router with state using utoipa-axum for consistency
        let (api_router, mut api_doc) = create_api_router().split_for_parts();

        api_doc.servers = Some(vec![
            utoipa::openapi::ServerBuilder::new()
                .url(format!("http://localhost:{port}"))
                .description(Some("Localhost development server"))
                .build(),
        ]);

        // Add state to the router
        let api_router = api_router.with_state(executor);

        // Create the full app router
        let mut app = Router::new().nest("/api/v1/", api_router.into());

        // Add swagger if requested
        if self.include_swagger {
            app = app.merge(SwaggerUi::new("/swagger-ui").url("/api/v1/openapi.json", api_doc));
        }

        // Setup CORS if requested.
        let cors_layer = if self.include_cors {
            Some(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any),
            )
        } else {
            None
        };

        // Apply the layers.
        app = app.layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .option_layer(cors_layer),
        );
        app
    }
}

/// Start the HTTP server using axum + utoipa
pub async fn start_server(
    port: u16,
    executor: Arc<StepFlowExecutor>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Create the app with default configuration (includes swagger and CORS)
    let app = AppConfig::default().create_app_router(executor, port);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;

    tracing::info!("🚀 StepFlow server starting on http://localhost:{}", port);
    tracing::info!(
        "📖 Swagger UI available at http://localhost:{}/swagger-ui",
        port
    );
    tracing::info!(
        "📄 OpenAPI spec available at http://localhost:{}/api/v1/openapi.json",
        port
    );

    axum::serve(listener, app).await?;

    Ok(())
}
