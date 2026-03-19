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

use std::future::Future;
use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use error_stack::ResultExt as _;
use stepflow_config::{ConfigError, StepflowConfig};
use stepflow_grpc::{
    ComponentsServiceImpl, FlowsServiceImpl, HealthServiceImpl, RunsServiceImpl, StepflowGrpcServer,
};
use stepflow_plugin::StepflowEnvironment;
use stepflow_state::OrchestratorId;
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::api::{create_api_router, finalize_openapi};

/// Options for building a [`StepflowService`].
pub struct ServiceOptions {
    /// Port to bind. `None` uses port 0 (OS-assigned dynamic port).
    pub port: Option<u16>,
    /// Bind address. Defaults to `"127.0.0.1"`.
    pub bind_address: String,
    /// Orchestrator ID for distributed lease management.
    pub orchestrator_id: Option<OrchestratorId>,
    /// Multiplex gRPC services on the same port as HTTP.
    pub include_grpc: bool,
    /// Include Swagger UI endpoint.
    pub include_swagger: bool,
    /// Include CORS headers.
    pub include_cors: bool,
    /// Print JSON port announcement (`{"port":N}`) to stdout.
    pub announce_port: bool,
}

impl Default for ServiceOptions {
    fn default() -> Self {
        Self {
            port: None,
            bind_address: "127.0.0.1".into(),
            orchestrator_id: None,
            include_grpc: true,
            include_swagger: false,
            include_cors: false,
            announce_port: false,
        }
    }
}

/// A ready-to-run Stepflow HTTP (+ optional gRPC) service.
///
/// Created via [`StepflowService::new`], which binds the port, creates the
/// environment, and assembles the router. Use [`serve`](Self::serve) to run
/// with a caller-provided shutdown future, or [`spawn_background`](Self::spawn_background)
/// to run as a background task.
pub struct StepflowService {
    port: u16,
    bind_address: String,
    env: Arc<StepflowEnvironment>,
    listener: tokio::net::TcpListener,
    app: Router,
    announce_port: bool,
}

impl StepflowService {
    /// Build the service: bind port, create environment, assemble router.
    pub async fn new(
        config: StepflowConfig,
        mut options: ServiceOptions,
    ) -> error_stack::Result<Self, ConfigError> {
        // Take orchestrator_id out early so we can still borrow `options`
        // for `create_app_router` later without cloning.
        let orchestrator_id = options.orchestrator_id.take();

        // Bind listener
        let bind_addr = format!("{}:{}", options.bind_address, options.port.unwrap_or(0));
        let listener = tokio::net::TcpListener::bind(&bind_addr)
            .await
            .change_context(ConfigError::Configuration)
            .attach_printable_lazy(|| format!("Failed to bind {bind_addr}"))?;

        let port = listener
            .local_addr()
            .change_context(ConfigError::Configuration)?
            .port();

        // Compute the gRPC address when enabled, so pull plugins know where
        // workers should connect (set before plugin initialization).
        let grpc_address = if options.include_grpc {
            Some(format!("{}:{}", options.bind_address, port))
        } else {
            None
        };

        // Create environment (auto-configures blob API URL from bound port).
        let env = create_environment(config, &listener, orchestrator_id, grpc_address).await?;

        // Build HTTP router
        let mut app = options.create_app_router(env.clone(), port);

        // Merge gRPC routes when requested. StepflowGrpcServer is always
        // present in the environment after create_environment.
        if options.include_grpc {
            let grpc_server = env
                .get::<Arc<StepflowGrpcServer>>()
                .expect("StepflowGrpcServer must be in the environment");
            let grpc_router = grpc_server.build_grpc_router(&env);
            app = app.merge(grpc_router);
        }

        Ok(Self {
            port,
            bind_address: options.bind_address,
            env,
            listener,
            app,
            announce_port: options.announce_port,
        })
    }

    /// The actual bound port (resolved from OS if port 0 was used).
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The environment with all plugins initialized.
    pub fn environment(&self) -> &Arc<StepflowEnvironment> {
        &self.env
    }

    /// Serve until the provided shutdown future completes.
    pub async fn serve(
        self,
        shutdown: impl Future<Output = ()> + Send + 'static,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.announce_port {
            #[allow(clippy::print_stdout)]
            {
                println!("{{\"port\":{}}}", self.port);
            }
        }

        log::info!(
            "Stepflow server starting on http://{}:{}",
            self.bind_address,
            self.port
        );

        axum::serve(self.listener, self.app)
            .with_graceful_shutdown(shutdown)
            .await?;

        log::info!("Server shutdown complete");
        Ok(())
    }

    /// Spawn the server as a background task.
    ///
    /// Useful for CLI commands that need a local server for blob API and
    /// gRPC services but don't need foreground serving or shutdown handling.
    /// The task runs until the process exits.
    ///
    /// If graceful shutdown of the background server is needed in the future,
    /// consider adding a variant that accepts a `CancellationToken` or
    /// shutdown future.
    pub fn spawn_background(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            if let Err(e) = axum::serve(self.listener, self.app).await {
                log::error!("Background HTTP server error: {e}");
            }
        })
    }
}

impl ServiceOptions {
    /// Create the HTTP application router from these options.
    ///
    /// This is used internally by [`StepflowService::new`], but is also
    /// available for tests that build an environment manually.
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
/// initialization.
pub(crate) async fn create_environment(
    mut config: StepflowConfig,
    listener: &tokio::net::TcpListener,
    orchestrator_id: Option<OrchestratorId>,
    grpc_address: Option<String>,
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
        .create_environment(orchestrator_id, Some(orchestrator_url), grpc_address)
        .await
}

/// Create an Axum router with proto-generated REST routes backed by gRPC
/// service implementations.
///
/// These routes are auto-generated from `google.api.http` annotations in the
/// proto files and share the same business logic as the gRPC services.
fn create_grpc_rest_router(env: Arc<StepflowEnvironment>) -> Router {
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
