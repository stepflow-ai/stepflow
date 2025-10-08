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
use pingora::prelude::*;
use pingora::proxy::Session;
use pingora::upstreams::peer::HttpPeer;
use pingora_core::Result as PingoraResult;
use pingora_http::RequestHeader;
use pingora_proxy::{ProxyHttp, http_proxy_service};
use std::sync::LazyLock;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

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
            method = %request_header.method,
            path = %request_header.uri.path(),
            backend = %backend.address,
            "Routing request"
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
            debug!(address = %address, "Incremented connection count");
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
            method = %request_header.method,
            path = %request_header.uri.path(),
            status = response.map(|r| r.status.as_u16()).unwrap_or(0),
            "Request completed"
        );

        // Decrement connection count when request completes
        if let Some(ref address) = ctx.backend_address {
            let pool_guard = BACKEND_POOL.load();
            let pool = pool_guard.read().await;
            pool.decrement_connections(address);
            debug!(address = %address, "Decremented connection count");
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
        debug!(instance_id = %instance_id, "Instance ID header found");

        if let Some(backend) = pool.get_by_instance_id(instance_id) {
            info!(
                instance_id = %instance_id,
                address = %backend.address,
                "Routing to specific instance"
            );
            return Ok(backend.clone());
        } else {
            warn!(instance_id = %instance_id, "Instance not found in healthy backends");
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
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("Starting Stepflow Load Balancer");

    // Get upstream service configuration from environment
    let upstream_service = std::env::var("UPSTREAM_SERVICE")
        .unwrap_or_else(|_| "component-server.stepflow-demo.svc.cluster.local:8080".to_string());

    info!(upstream_service = %upstream_service, "Upstream service configuration");

    // Start backend discovery service in a separate thread with its own runtime
    let upstream_service_clone = upstream_service.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let discovery = DiscoveryService::new(upstream_service_clone);
            if let Err(e) = discovery.run().await {
                error!(error = %e, "Discovery service error");
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
