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

use arc_swap::ArcSwap;
use async_trait::async_trait;
use log::{debug, error, info, warn};
use pingora::prelude::*;
use pingora::proxy::Session;
use pingora::upstreams::peer::HttpPeer;
use pingora_core::Result as PingoraResult;
use pingora_http::RequestHeader;
use pingora_proxy::{ProxyHttp, http_proxy_service};
use std::sync::LazyLock;
use stepflow_observability::{ObservabilityConfig, init_observability};
use tokio::sync::RwLock;

mod backend;
mod discovery;

use backend::{Backend, BackendPool};
use discovery::DiscoveryService;

/// Global backend pool shared across all requests
static BACKEND_POOL: LazyLock<ArcSwap<RwLock<BackendPool>>> =
    LazyLock::new(|| ArcSwap::from_pointee(RwLock::new(BackendPool::new())));

/// Stepflow load balancer proxy service
pub struct StepflowLoadBalancer;

/// Context to track which backend was selected for this request
pub struct RequestContext {
    backend_address: Option<String>,
}

#[async_trait]
impl ProxyHttp for StepflowLoadBalancer {
    type CTX = RequestContext;

    fn new_ctx(&self) -> Self::CTX {
        RequestContext {
            backend_address: None,
        }
    }

    async fn upstream_peer(
        &self,
        session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> PingoraResult<Box<HttpPeer>> {
        let request_header = session.req_header();

        // Select backend based on instance ID or load balancing
        let backend = select_backend(request_header).await?;

        info!(
            "Routing request: method={}, path={}, backend={}",
            request_header.method,
            request_header.uri.path(),
            backend.address
        );

        // Store backend address in context for connection tracking
        ctx.backend_address = Some(backend.address.to_string());

        // Create peer for selected backend
        let peer = Box::new(HttpPeer::new(
            backend.address,
            false,         // TLS
            String::new(), // SNI
        ));

        Ok(peer)
    }

    async fn connected_to_upstream(
        &self,
        _session: &mut Session,
        _reused: bool,
        _peer: &HttpPeer,
        _fd: std::os::unix::io::RawFd,
        _digest: Option<&pingora::protocols::Digest>,
        ctx: &mut Self::CTX,
    ) -> PingoraResult<()> {
        // Increment connection count when successfully connected to upstream
        if let Some(ref address) = ctx.backend_address {
            let pool_guard = BACKEND_POOL.load();
            let pool = pool_guard.read().await;
            pool.increment_connections(address);
            debug!("Incremented connection count: address={}", address);
        }
        Ok(())
    }

    async fn upstream_request_filter(
        &self,
        session: &mut Session,
        upstream_request: &mut RequestHeader,
        _ctx: &mut Self::CTX,
    ) -> PingoraResult<()> {
        // Propagate W3C trace context headers for distributed tracing
        let incoming_headers = &session.req_header().headers;

        // Forward traceparent header (W3C Trace Context)
        if let Some(traceparent) = incoming_headers.get("traceparent") {
            upstream_request.insert_header("traceparent", traceparent)?;
            debug!("Propagating traceparent header: {:?}", traceparent);
        }

        // Forward tracestate header (W3C Trace Context)
        if let Some(tracestate) = incoming_headers.get("tracestate") {
            upstream_request.insert_header("tracestate", tracestate)?;
            debug!("Propagating tracestate header: {:?}", tracestate);
        }

        // Forward baggage header (OpenTelemetry)
        if let Some(baggage) = incoming_headers.get("baggage") {
            upstream_request.insert_header("baggage", baggage)?;
            debug!("Propagating baggage header: {:?}", baggage);
        }

        // Add header for debugging
        upstream_request.insert_header("X-Forwarded-By", "stepflow-load-balancer")?;

        Ok(())
    }

    async fn response_filter(
        &self,
        session: &mut Session,
        upstream_response: &mut pingora_http::ResponseHeader,
        _ctx: &mut Self::CTX,
    ) -> PingoraResult<()> {
        // Check if this is an SSE stream
        let is_sse = upstream_response
            .headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.starts_with("text/event-stream"))
            .unwrap_or(false);

        if is_sse {
            debug!("SSE stream detected, ensuring no buffering");

            // Ensure no buffering for SSE streams
            upstream_response.insert_header("X-Accel-Buffering", "no")?;
            upstream_response.insert_header("Cache-Control", "no-cache")?;
        }

        // Add debug headers
        if let Some(instance_id) = session
            .req_header()
            .headers
            .get("Stepflow-Instance-Id")
            .and_then(|v| v.to_str().ok())
        {
            upstream_response.insert_header("X-Stepflow-Routed-Instance", instance_id)?;
        }

        Ok(())
    }

    async fn logging(
        &self,
        session: &mut Session,
        _e: Option<&pingora_core::Error>,
        ctx: &mut Self::CTX,
    ) {
        let request_header = session.req_header();
        let response = session.response_written();

        info!(
            "Request completed: method={}, path={}, status={}",
            request_header.method,
            request_header.uri.path(),
            response.map(|r| r.status.as_u16()).unwrap_or(0)
        );

        // Decrement connection count when request completes
        if let Some(ref address) = ctx.backend_address {
            let pool_guard = BACKEND_POOL.load();
            let pool = pool_guard.read().await;
            pool.decrement_connections(address);
            debug!("Decremented connection count: address={}", address);
        }
    }
}

/// Select backend based on Stepflow-Instance-Id header or load balancing
async fn select_backend(request_header: &RequestHeader) -> PingoraResult<Backend> {
    let pool_guard = BACKEND_POOL.load();
    let pool = pool_guard.read().await;

    // Check for explicit instance routing
    if let Some(instance_id) = request_header
        .headers
        .get("Stepflow-Instance-Id")
        .and_then(|v| v.to_str().ok())
    {
        debug!("Instance ID header found: instance_id={}", instance_id);

        if let Some(backend) = pool.get_by_instance_id(instance_id) {
            info!(
                "Routing to specific instance: instance_id={}, address={}",
                instance_id, backend.address
            );
            return Ok(backend.clone());
        } else {
            warn!(
                "Instance not found in healthy backends: instance_id={}",
                instance_id
            );
            return Err(Error::new_str("Instance not available"));
        }
    }

    // No instance preference - use load balancing
    debug!("No instance ID, using load balancing");

    pool.least_connections_backend()
        .cloned()
        .ok_or_else(|| Error::new_str("No healthy backends available"))
}

/// Stepflow Load Balancer
#[derive(clap::Parser, Debug)]
#[command(name = "stepflow-load-balancer")]
#[command(about = "Stepflow Load Balancer", long_about = None)]
struct Args {
    /// Observability configuration
    #[command(flatten)]
    observability: ObservabilityConfig,

    /// Upstream service address
    #[arg(
        long,
        default_value = "component-server.stepflow-demo.svc.cluster.local:8080",
        env = "UPSTREAM_SERVICE"
    )]
    upstream_service: String,

    /// Address to bind the load balancer
    #[arg(long, default_value = "0.0.0.0:8080", env = "BIND_ADDRESS")]
    bind_address: String,
}

fn main() -> anyhow::Result<()> {
    use clap::Parser as _;

    let args = Args::parse();

    // Initialize observability
    // Load balancer doesn't execute workflows, so disable run diagnostic
    let binary_config = stepflow_observability::BinaryObservabilityConfig {
        service_name: "stepflow-load-balancer",
        include_run_diagnostic: false,
    };
    let _guard = init_observability(&args.observability, binary_config)
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;

    info!("Starting Stepflow Load Balancer");
    info!(
        "Upstream service configuration: upstream_service={}",
        args.upstream_service
    );

    // Start backend discovery service in a separate thread with its own runtime
    let upstream_service = args.upstream_service.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let discovery = DiscoveryService::new(upstream_service);
            if let Err(e) = discovery.run().await {
                error!("Discovery service error: {}", e);
            }
        });
    });

    // Create Pingora server
    let mut server = Server::new(None)?;
    server.bootstrap();

    // Create load balancer service
    let mut lb = http_proxy_service(&server.configuration, StepflowLoadBalancer);
    lb.add_tcp(&args.bind_address);

    // Add service to server
    server.add_service(lb);

    info!("Load balancer listening on {}", args.bind_address);

    // Run server
    server.run_forever();
}
