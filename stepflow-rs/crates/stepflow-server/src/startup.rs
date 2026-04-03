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
use stepflow_state::{ActiveRecoveries, ActiveRecoveriesExt as _, OrchestratorId};
use tokio_util::sync::CancellationToken;
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

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
    /// Include CORS headers.
    pub include_cors: bool,
    /// Print JSON port announcement (`{"port":N}`) to stdout.
    pub announce_port: bool,
    /// If `true`, the gRPC services will return `UNAVAILABLE` for unknown
    /// tasks until [`ActiveRecoveries::set_startup_pending`] is called with `false`.
    ///
    /// Set this when the server will perform startup recovery so that workers
    /// don't receive `NOT_FOUND` during the window between server start and
    /// recovery completion.
    pub startup_recovery_pending: bool,
}

impl Default for ServiceOptions {
    fn default() -> Self {
        Self {
            port: None,
            bind_address: "127.0.0.1".into(),
            orchestrator_id: None,
            include_grpc: true,
            include_cors: false,
            announce_port: false,
            startup_recovery_pending: false,
        }
    }
}

/// A running Stepflow HTTP + gRPC service.
///
/// Created via [`StepflowService::new`], which binds the port, creates the
/// environment, starts the server, and initializes plugins. The server is
/// already accepting connections when `new` returns — this is required so
/// that pull-based worker subprocesses can connect during plugin initialization.
///
/// All services (HTTP REST and gRPC) are multiplexed on a single port using
/// tonic's transport server with `accept_http1(true)`, which handles both
/// HTTP/1.1 (REST) and HTTP/2 (gRPC) natively.
///
/// Use [`shutdown`](Self::shutdown) for graceful shutdown, or let the service
/// run until the process exits.
pub struct StepflowService {
    port: u16,
    env: Arc<StepflowEnvironment>,
    cancel_token: CancellationToken,
    server_handle: tokio::task::JoinHandle<Result<(), tonic::transport::Error>>,
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

        // Create environment without initializing plugins — the server must
        // be accepting connections before workers are spawned.
        //
        // For the gRPC address advertised to workers, use a connectable address.
        // If bind_address is a wildcard (0.0.0.0 / ::), fall back to 127.0.0.1.
        let grpc_address = if options.include_grpc {
            let advertise_host =
                if options.bind_address == "0.0.0.0" || options.bind_address == "::" {
                    "127.0.0.1"
                } else {
                    &options.bind_address
                };
            Some(format!("{advertise_host}:{port}"))
        } else {
            None
        };
        let env = create_environment(config, &listener, orchestrator_id, grpc_address).await?;

        // Set the startup recovery gate BEFORE building routes / starting the
        // server.  While this flag is set, gRPC services return UNAVAILABLE
        // for unknown tasks instead of NOT_FOUND, giving the startup recovery
        // pass time to re-register in-flight tasks.  The caller (main.rs)
        // clears this after recovery completes.
        //
        // ActiveRecoveries must be installed early (before plugin init) so it
        // is available when the gRPC server starts accepting connections.
        if options.startup_recovery_pending {
            if !env.contains::<ActiveRecoveries>() {
                env.insert(ActiveRecoveries::new());
            }
            env.active_recoveries().set_startup_pending(true);
        }

        // Build the combined HTTP + gRPC router served by tonic.
        // Tonic's transport server handles both HTTP/1.1 (REST) and HTTP/2
        // (gRPC) on the same port via accept_http1(true).
        let http_app = options.create_app_router(env.clone(), port);

        let mut grpc_routes = if options.include_grpc {
            let grpc_server = env
                .get::<Arc<StepflowGrpcServer>>()
                .expect("StepflowGrpcServer must be in the environment");
            grpc_server.build_grpc_routes(&env)
        } else {
            // Empty routes — still needed as the base for the tonic router
            tonic::service::Routes::default()
        };

        // Merge HTTP routes into the tonic routes so everything is served
        // on a single port. We take the inner router, merge, and put it back
        // because axum::Router::merge takes ownership.
        let merged = std::mem::take(grpc_routes.axum_router_mut()).merge(http_app);
        *grpc_routes.axum_router_mut() = merged;

        log::info!(
            "Stepflow server starting on http://{}:{}",
            options.bind_address,
            port
        );

        // Start the server — tonic handles both HTTP/1.1 and HTTP/2.
        let cancel_token = CancellationToken::new();
        let shutdown_token = cancel_token.clone();
        let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
        let server_handle = tokio::spawn(async move {
            tonic::transport::Server::builder()
                .accept_http1(true)
                .add_routes(grpc_routes)
                .serve_with_incoming_shutdown(incoming, async move {
                    shutdown_token.cancelled().await;
                })
                .await
        });

        // Now initialize plugins — the server is already accepting
        // connections, so worker subprocesses can connect immediately.
        if let Err(e) = stepflow_plugin::initialize_environment(&env).await {
            // Clean up the server task before propagating the error
            cancel_token.cancel();
            let _ = server_handle.await;
            return Err(e.change_context(ConfigError::Configuration));
        }

        // Announce the port — the server is accepting connections.
        // Plugin readiness is enforced at flow execution time: the step runner
        // waits for all plugins to report their components before resolving routes.
        if options.announce_port {
            #[allow(clippy::print_stdout)]
            {
                println!("{{\"port\":{port}}}");
            }
        }

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
    ///
    /// Propagates server errors and panics so callers can fail fast.
    pub async fn wait(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self.server_handle.await {
            Ok(result) => result.map_err(Into::into),
            Err(e) if e.is_cancelled() => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Forcefully stop the server and release all resources.
    ///
    /// Unlike [`shutdown`] + [`wait`], this aborts the server task immediately
    /// rather than draining existing connections.  Aborting the server task
    /// drops the `Arc<StepflowEnvironment>` references it holds, which lets
    /// the environment (and its plugins / worker subprocesses) be cleaned up
    /// once all other references are dropped.
    ///
    /// This is used by `stepflow test` to ensure each test file's worker
    /// subprocesses are killed before the next test file starts, preventing
    /// cross-test interference.
    pub fn abort(self) {
        self.cancel_token.cancel();
        self.server_handle.abort();
        // Dropping `self` releases `self.env`, the final Arc reference.
    }
}

impl ServiceOptions {
    /// Create the HTTP application router from these options.
    ///
    /// This is used internally by [`StepflowService::new`], but is also
    /// available for tests that build an environment manually.
    pub fn create_app_router(&self, executor: Arc<StepflowEnvironment>, _port: u16) -> Router {
        // Proto-generated REST routes from google.api.http annotations,
        // sharing the same business logic as the gRPC services.
        let grpc_rest_router = create_grpc_rest_router(executor);

        let mut app = Router::new().nest("/api/v1", grpc_rest_router);

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
        .create_environment(
            orchestrator_id,
            Some(orchestrator_url),
            grpc_address,
            true, // skip plugin init — server must be accepting first
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
