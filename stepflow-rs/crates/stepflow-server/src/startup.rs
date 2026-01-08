// Copyright 2025 DataStax Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
// in compliance with the License. You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied. See the License for the specific language governing permissions and limitations under
// the License.

use axum::Router;
use std::sync::Arc;
use stepflow_execution::StepflowExecutor;
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
    pub fn create_app_router(&self, executor: Arc<StepflowExecutor>, port: u16) -> Router {
        // Create the main API router with state using utoipa-axum for consistency
        let (api_router, mut api_doc) = create_api_router().split_for_parts();

        api_doc.servers = Some(vec![
            utoipa::openapi::ServerBuilder::new()
                .url(format!("http://localhost:{port}/api/v1"))
                .description(Some("Localhost development server"))
                .build(),
        ]);

        // Add state to the router
        let api_router = api_router.with_state(executor);

        // Create the full app router
        let mut app = Router::new().nest("/api/v1/", api_router);

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
    executor: Arc<StepflowExecutor>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Create the app with default configuration (includes swagger and CORS)
    let app = AppConfig::default().create_app_router(executor, port);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;

    log::info!("🚀 Stepflow server starting on http://localhost:{}", port);
    log::info!(
        "📖 Swagger UI available at http://localhost:{}/swagger-ui",
        port
    );
    log::info!(
        "📄 OpenAPI spec available at http://localhost:{}/api/v1/openapi.json",
        port
    );

    // Use graceful shutdown to allow proper cleanup when SIGTERM/SIGINT is received
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    log::info!("Server shutdown complete");

    Ok(())
}

/// Wait for a shutdown signal (SIGTERM or SIGINT).
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install CTRL+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            log::info!("Received CTRL+C, initiating graceful shutdown");
        }
        _ = terminate => {
            log::info!("Received SIGTERM, initiating graceful shutdown");
        }
    }
}
