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
use tokio_util::sync::CancellationToken;
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

/// A running Stepflow HTTP (+ gRPC) service.
///
/// Created via [`StepflowService::new`], which binds the port, creates the
/// environment, starts the server, and initializes plugins. The server is
/// already accepting connections when `new` returns — this is required so
/// that pull-based worker subprocesses can connect during plugin initialization.
///
/// Use [`shutdown`](Self::shutdown) for graceful shutdown, or let the service
/// run until the process exits.
pub struct StepflowService {
    port: u16,
    env: Arc<StepflowEnvironment>,
    cancel_token: CancellationToken,
    server_handle: tokio::task::JoinHandle<Result<(), std::io::Error>>,
}

impl StepflowService {
    /// Build and start the service.
    ///
    /// This binds the port, creates the environment, starts the HTTP+gRPC
    /// server, then initializes plugins (which may spawn worker subprocesses
    /// that connect back to the now-running server).
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

        // Create environment without initializing plugins — the gRPC server
        // must be accepting connections before workers are spawned.
        let env = create_environment(config, &listener, orchestrator_id).await?;

        // Start a standalone gRPC server for worker-facing services (pull
        // transport, heartbeats, task completion). This is a separate tonic
        // server that speaks native HTTP/2, which gRPC clients require.
        // The HTTP server (axum) also merges gRPC routes for client-facing
        // use (grpcurl, etc.) but workers connect to the standalone address.
        if options.include_grpc {
            let grpc_server = env
                .get::<Arc<StepflowGrpcServer>>()
                .expect("StepflowGrpcServer must be in the environment");
            grpc_server
                .start_standalone(&env)
                .await
                .change_context(ConfigError::Configuration)?;
        }

        // Now initialize plugins — the gRPC server is already accepting
        // connections, so worker subprocesses can connect immediately.
        stepflow_plugin::initialize_environment(&env)
            .await
            .change_context(ConfigError::Configuration)?;

        // Build and start the HTTP server (also includes gRPC routes for
        // client-facing use via h2c upgrade, e.g. grpcurl).
        let mut app = options.create_app_router(env.clone(), port);
        if options.include_grpc {
            let grpc_server = env
                .get::<Arc<StepflowGrpcServer>>()
                .expect("StepflowGrpcServer must be in the environment");
            let grpc_router = grpc_server.build_grpc_router(&env);
            app = app.merge(grpc_router);
        }

        if options.announce_port {
            #[allow(clippy::print_stdout)]
            {
                println!("{{\"port\":{port}}}");
            }
        }

        log::info!(
            "Stepflow server starting on http://{}:{}",
            options.bind_address,
            port
        );

        let cancel_token = CancellationToken::new();
        let shutdown_token = cancel_token.clone();
        let server_handle = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move { shutdown_token.cancelled().await })
                .await
        });

        Ok(Self {
            port,
            env,
            cancel_token,
            server_handle,
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

    /// Trigger graceful shutdown of the server.
    ///
    /// Signals the server to stop accepting new connections and drain
    /// existing ones. Call [`wait`](Self::wait) after this to block until
    /// the server has fully stopped.
    pub fn shutdown(&self) {
        self.cancel_token.cancel();
    }

    /// Wait for the server to finish (after [`shutdown`](Self::shutdown)).
    pub async fn wait(self) -> Result<(), std::io::Error> {
        match self.server_handle.await {
            Ok(result) => result,
            Err(e) if e.is_cancelled() => Ok(()),
            Err(e) => {
                log::error!("Server task panicked: {e:?}");
                Ok(())
            }
        }
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
/// Plugin initialization is deferred — the caller must call
/// [`initialize_environment`](stepflow_plugin::initialize_environment) after
/// the server is accepting connections (so worker subprocesses can connect).
pub(crate) async fn create_environment(
    mut config: StepflowConfig,
    listener: &tokio::net::TcpListener,
    orchestrator_id: Option<OrchestratorId>,
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
        .create_environment(
            orchestrator_id,
            Some(orchestrator_url),
            None, // gRPC address set later by start_standalone
            true, // skip plugin init — gRPC server must be running first
        )
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
