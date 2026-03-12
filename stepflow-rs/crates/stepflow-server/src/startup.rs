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
use axum::extract::DefaultBodyLimit;
use error_stack::ResultExt as _;
use std::sync::Arc;
use stepflow_config::{ConfigError, StepflowConfig};
use stepflow_grpc::{ComponentsServiceImpl, FlowsServiceImpl, HealthServiceImpl, RunsServiceImpl};
use stepflow_plugin::StepflowEnvironment;
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::api::{create_api_router, finalize_openapi};

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
    pub fn create_app_router(&self, executor: Arc<StepflowEnvironment>, port: u16) -> Router {
        // Create the main API router with state using aide for OpenAPI generation
        let (api_router, mut api) = create_api_router();

        // Override the default server with the actual port
        api.servers = vec![aide::openapi::Server {
            url: format!("http://localhost:{port}/api/v1"),
            description: Some("Localhost development server".into()),
            ..Default::default()
        }];

        let api_json = Arc::new(finalize_openapi(&api));

        // Add state to the router
        let api_router = api_router.with_state(executor.clone());

        // Proto-generated REST routes (mounted at /proto/api/v1 for coexistence during migration).
        // Proto annotations use relative paths (/health, /blobs, etc.) and are nested under
        // a prefix. Once validated, change the nest path to /api/v1 and remove the aide routes.
        let grpc_rest_router = create_grpc_rest_router(executor);

        // Create the full app router
        let mut app = Router::new()
            .nest("/api/v1/", api_router)
            .nest("/proto/api/v1", grpc_rest_router)
            .route(
                "/api/v1/openapi.json",
                axum::routing::get({
                    let api_json = api_json.clone();
                    move || {
                        let api_json = api_json.clone();
                        async move { axum::Json(api_json.as_ref().clone()) }
                    }
                }),
            );

        // Add swagger if requested
        if self.include_swagger {
            let swagger_handler =
                aide::swagger::Swagger::new("/api/v1/openapi.json").axum_handler();
            app = app.route("/swagger-ui", axum::routing::get(swagger_handler));
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
        // The default body limit is set to 250 MiB to support large document
        // uploads (e.g. PDFs sent as base64 in flow inputs).  The orchestrator
        // may also receive large flow results containing embedded images.
        app = app.layer(
            ServiceBuilder::new()
                .layer(DefaultBodyLimit::max(250 * 1024 * 1024))
                .layer(TraceLayer::new_for_http())
                .option_layer(cors_layer),
        );
        app
    }
}

/// Create a [`StepflowEnvironment`] from config, auto-configuring the blob API URL
/// from the listener's bound port if needed.
///
/// This ensures component workers receive the blob API URL during plugin
/// initialization. Both the CLI and HTTP server use this to avoid duplicating
/// the blob URL setup logic.
pub async fn create_environment(
    mut config: StepflowConfig,
    listener: &tokio::net::TcpListener,
    orchestrator_id: Option<stepflow_state::OrchestratorId>,
) -> error_stack::Result<Arc<StepflowEnvironment>, ConfigError> {
    let port = listener
        .local_addr()
        .change_context(ConfigError::Configuration)
        .attach_printable("Failed to get listener address")?
        .port();

    if config.blob_api.enabled && config.blob_api.url.is_none() {
        config.blob_api.url = Some(format!("http://127.0.0.1:{port}/api/v1/blobs"));
        log::info!(
            "Blob API URL auto-configured: {}",
            config.blob_api.url.as_ref().unwrap()
        );
    } else {
        log::debug!(
            "Blob API configuration: enabled={}, url={:?}",
            config.blob_api.enabled,
            config.blob_api.url
        );
    }

    // Resolve orchestrator URL: STEPFLOW_ORCHESTRATOR_URL env var,
    // falling back to localhost:<port>. Workers use this URL to call back
    // to the orchestrator (e.g., submit sub-runs, report completion).
    // In multi-orchestrator deployments, set STEPFLOW_ORCHESTRATOR_URL
    // per-instance.
    let orchestrator_url = std::env::var("STEPFLOW_ORCHESTRATOR_URL").unwrap_or_else(|_| {
        let url = format!("127.0.0.1:{port}");
        log::info!("STEPFLOW_ORCHESTRATOR_URL not set, defaulting to {url}");
        url
    });

    config
        .create_environment(orchestrator_id, Some(orchestrator_url))
        .await
}

/// Start the HTTP server using axum + aide
pub async fn start_server(
    listener: tokio::net::TcpListener,
    env: Arc<StepflowEnvironment>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let actual_port = listener.local_addr()?.port();

    // Create the app with the actual port
    let app = AppConfig::default().create_app_router(env, actual_port);

    // Emit JSON port announcement to stdout for orchestrator/subprocess management
    // This MUST be the first line of stdout output
    #[allow(clippy::print_stdout)]
    {
        println!("{{\"port\":{actual_port}}}");
    }

    log::info!(
        "🚀 Stepflow server starting on http://localhost:{}",
        actual_port
    );
    log::info!(
        "📖 Swagger UI available at http://localhost:{}/swagger-ui",
        actual_port
    );
    log::info!(
        "📄 OpenAPI spec available at http://localhost:{}/api/v1/openapi.json",
        actual_port
    );

    // Use graceful shutdown to allow proper cleanup when SIGTERM/SIGINT is received
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    log::info!("Server shutdown complete");

    Ok(())
}

/// Create an Axum router with proto-generated REST routes backed by gRPC
/// service implementations.
///
/// These routes are auto-generated from `google.api.http` annotations in the
/// proto files and share the same business logic as the gRPC services.
///
/// The returned router has routes at `/api/v1/...` — the same paths as the
/// existing aide-based API. To use it, either:
/// - Replace the aide router with this one
/// - Mount it at a different prefix (e.g., nest under `/proto`)
pub fn create_grpc_rest_router(env: Arc<StepflowEnvironment>) -> Router {
    let health_service = Arc::new(HealthServiceImpl::new());
    let components_service = Arc::new(ComponentsServiceImpl::new(env.clone()));
    let flows_service = Arc::new(FlowsServiceImpl::new(env.clone()));
    let runs_service = Arc::new(RunsServiceImpl::new(env.clone()));

    // Build per-service REST routes from proto annotations, skipping
    // blob_service_rest_router (replaced by binary-aware routes below).
    let proto_routes = stepflow_grpc::rest::health_service_rest_router(health_service)
        .merge(stepflow_grpc::rest::components_service_rest_router(
            components_service,
        ))
        .merge(stepflow_grpc::rest::flows_service_rest_router(
            flows_service,
        ))
        .merge(stepflow_grpc::rest::runs_service_rest_router(runs_service));

    // Hand-written blob routes support both JSON and binary (application/octet-stream)
    // content negotiation. They replace the tonic-rest blob routes because tonic-rest
    // doesn't support raw byte passthrough (HttpBody).
    let blob_routes = stepflow_grpc::blob_binary_routes::blob_binary_routes(env);

    proto_routes.merge(blob_routes)
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
