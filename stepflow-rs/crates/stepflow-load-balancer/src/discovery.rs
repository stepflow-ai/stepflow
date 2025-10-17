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

use crate::BACKEND_POOL;
use crate::backend::Backend;
use log::{debug, error, info, warn};
use serde::Deserialize;
use std::net::{SocketAddr, ToSocketAddrs as _};
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug, Deserialize)]
struct HealthResponse {
    status: String,
    #[serde(rename = "instanceId")]
    instance_id: String,
}

/// Service for discovering and health-checking backend instances
pub struct DiscoveryService {
    upstream_service: String,
    client: reqwest::Client,
}

impl DiscoveryService {
    pub fn new(upstream_service: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();

        Self {
            upstream_service,
            client,
        }
    }

    /// Run discovery loop
    pub async fn run(&self) -> anyhow::Result<()> {
        info!(
            "Starting backend discovery: upstream_service={}",
            self.upstream_service
        );

        loop {
            match self.discover_backends().await {
                Ok(backends) => {
                    info!("Discovered healthy backends: count={}", backends.len());

                    // Update global backend pool
                    let pool_guard = BACKEND_POOL.load();
                    let mut pool = pool_guard.write().await;
                    pool.update_backends(backends);

                    debug!("Backend pool updated");
                }
                Err(e) => {
                    error!("Backend discovery failed: {}", e);
                }
            }

            // Wait before next discovery
            sleep(Duration::from_secs(10)).await;
        }
    }

    /// Discover backends by resolving service DNS and health checking
    async fn discover_backends(&self) -> anyhow::Result<Vec<Backend>> {
        // Resolve service to get backend addresses
        let addresses: Vec<SocketAddr> = self.upstream_service.to_socket_addrs()?.collect();

        debug!(
            "Resolved addresses: count={}, service={}",
            addresses.len(),
            self.upstream_service
        );

        // Health check each address
        let mut backends = Vec::new();

        for addr in addresses {
            match self.health_check(&addr).await {
                Ok(instance_id) => {
                    info!(
                        "Backend is healthy: address={}, instance_id={}",
                        addr, instance_id
                    );
                    backends.push(Backend::new(instance_id, addr));
                }
                Err(e) => {
                    warn!("Backend health check failed: address={}, error={}", addr, e);
                }
            }
        }

        Ok(backends)
    }

    /// Perform health check and extract instance ID
    async fn health_check(&self, addr: &SocketAddr) -> anyhow::Result<String> {
        let url = format!("http://{}/health", addr);

        let response = self.client.get(&url).send().await?.error_for_status()?;

        // Get response text for debugging
        let response_text = response.text().await?;
        debug!(
            "Health check response: address={}, response={}",
            addr, response_text
        );

        // Parse JSON
        let health: HealthResponse = serde_json::from_str(&response_text).map_err(|e| {
            anyhow::anyhow!(
                "Failed to parse health response: {}. Body: {}",
                e,
                response_text
            )
        })?;

        if health.status != "healthy" {
            anyhow::bail!("Backend reports unhealthy status");
        }

        Ok(health.instance_id)
    }
}
