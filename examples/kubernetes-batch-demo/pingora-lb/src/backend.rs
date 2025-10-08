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

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use itertools::Itertools;

/// Represents a backend component server instance
#[derive(Debug, Clone)]
pub struct Backend {
    pub instance_id: String,
    pub address: SocketAddr,
    pub healthy: bool,
    pub active_connections: Arc<AtomicU64>,
}

impl Backend {
    pub fn new(instance_id: String, address: SocketAddr) -> Self {
        Self {
            instance_id,
            address,
            healthy: true,
            active_connections: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn increment_connections(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    pub fn decrement_connections(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn connection_count(&self) -> u64 {
        self.active_connections.load(Ordering::Relaxed)
    }
}

/// Pool of backend servers with load balancing
pub struct BackendPool {
    backends: HashMap<String, Backend>,
    round_robin_counter: Arc<AtomicUsize>,
}

impl BackendPool {
    pub fn new() -> Self {
        Self {
            backends: HashMap::new(),
            round_robin_counter: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Update backends from discovery
    pub fn update_backends(&mut self, backends: Vec<Backend>) {
        // Clear old backends
        self.backends.clear();

        // Add new backends
        for backend in backends {
            self.backends.insert(backend.instance_id.clone(), backend);
        }
    }

    /// Get backend by instance ID
    pub fn get_by_instance_id(&self, instance_id: &str) -> Option<&Backend> {
        self.backends
            .get(instance_id)
            .filter(|b| b.healthy)
    }

    /// Select backend using least connections algorithm with round-robin for ties
    pub fn least_connections_backend(&self) -> Option<&Backend> {
        let min_connections = self
            .backends
            .values()
            .filter(|b| b.healthy)
            .min_set_by_key(|b| b.connection_count());

        if min_connections.is_empty() {
            None
        } else {
            let index = self.round_robin_counter.fetch_add(1, Ordering::Relaxed);
            Some(min_connections[index % min_connections.len()])
        }
    }

    /// Increment connection count for a backend by address
    pub fn increment_connections(&self, address: &str) {
        if let Ok(socket_addr) = address.parse::<SocketAddr>() {
            if let Some(backend) = self.backends.values().find(|b| b.address == socket_addr) {
                backend.increment_connections();
            }
        }
    }

    /// Decrement connection count for a backend by address
    pub fn decrement_connections(&self, address: &str) {
        if let Ok(socket_addr) = address.parse::<SocketAddr>() {
            if let Some(backend) = self.backends.values().find(|b| b.address == socket_addr) {
                backend.decrement_connections();
            }
        }
    }
}
