use async_trait::async_trait;
use log::{debug, error, info, warn};
use once_cell::sync::Lazy;
use pingora::prelude::*;
use pingora::proxy::Session;
use pingora::upstreams::peer::HttpPeer;
use pingora_core::Result as PingoraResult;
use pingora_http::RequestHeader;
use pingora_proxy::{http_proxy_service, ProxyHttp};
use std::sync::Arc;
use tokio::sync::RwLock;

mod backend;
mod discovery;

use backend::{Backend, BackendPool};
use discovery::DiscoveryService;

/// Global backend pool shared across all requests
static BACKEND_POOL: Lazy<Arc<RwLock<BackendPool>>> =
    Lazy::new(|| Arc::new(RwLock::new(BackendPool::new())));

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
            "Routing {} {} to {}",
            request_header.method,
            request_header.uri.path(),
            backend.address
        );

        // Store backend address in context for connection tracking
        ctx.backend_address = Some(backend.address.to_string());

        // Create peer for selected backend
        let peer = Box::new(HttpPeer::new(
            backend.address,
            false, // TLS
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
            let pool = BACKEND_POOL.read().await;
            pool.increment_connections(address);
            debug!("Incremented connection count for {}", address);
        }
        Ok(())
    }

    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        upstream_request: &mut RequestHeader,
        _ctx: &mut Self::CTX,
    ) -> PingoraResult<()> {
        // Add headers for debugging
        upstream_request
            .insert_header("X-Forwarded-By", "stepflow-pingora-lb")?;

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
            upstream_response
                .insert_header("X-Accel-Buffering", "no")?;
            upstream_response
                .insert_header("Cache-Control", "no-cache")?;
        }

        // Add debug headers
        if let Some(instance_id) = session.req_header()
            .headers
            .get("Stepflow-Instance-Id")
            .and_then(|v| v.to_str().ok())
        {
            upstream_response
                .insert_header("X-Stepflow-Routed-Instance", instance_id)?;
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
            "{} {} - Status: {}",
            request_header.method,
            request_header.uri.path(),
            response.map(|r| r.status.as_u16()).unwrap_or(0)
        );

        // Decrement connection count when request completes
        if let Some(ref address) = ctx.backend_address {
            let pool = BACKEND_POOL.read().await;
            pool.decrement_connections(address);
            debug!("Decremented connection count for {}", address);
        }
    }
}

/// Select backend based on Stepflow-Instance-Id header or load balancing
async fn select_backend(request_header: &RequestHeader) -> PingoraResult<Backend> {
    let pool = BACKEND_POOL.read().await;

    // Check for explicit instance routing
    if let Some(instance_id) = request_header
        .headers
        .get("Stepflow-Instance-Id")
        .and_then(|v| v.to_str().ok())
    {
        debug!("Instance ID header found: {}", instance_id);

        if let Some(backend) = pool.get_by_instance_id(instance_id) {
            info!("Routing to instance: {} at {}", instance_id, backend.address);
            return Ok(backend.clone());
        } else {
            warn!("Instance {} not found in healthy backends", instance_id);
            return Err(Error::new_str("Instance not available"));
        }
    }

    // No instance preference - use load balancing
    debug!("No instance ID, using load balancing");

    pool.least_connections_backend()
        .cloned()
        .ok_or_else(|| Error::new_str("No healthy backends available"))
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("Starting Stepflow Pingora Load Balancer");

    // Get upstream service configuration from environment
    let upstream_service = std::env::var("UPSTREAM_SERVICE")
        .unwrap_or_else(|_| "component-server.stepflow-demo.svc.cluster.local:8080".to_string());

    info!("Upstream service: {}", upstream_service);

    // Start backend discovery service in a separate thread with its own runtime
    let upstream_service_clone = upstream_service.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let discovery = DiscoveryService::new(upstream_service_clone);
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
    lb.add_tcp("0.0.0.0:8080");

    // Add service to server
    server.add_service(lb);

    info!("Load balancer listening on 0.0.0.0:8080");

    // Run server
    server.run_forever();
}
