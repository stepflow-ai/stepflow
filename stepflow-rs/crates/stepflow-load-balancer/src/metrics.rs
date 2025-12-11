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

//! Prometheus metrics for the Stepflow load balancer
//!
//! Metrics are defined as static variables using the `Lazy` pattern,
//! which automatically registers them with the Prometheus registry.

use prometheus::{
    Counter, CounterVec, Gauge, GaugeVec, HistogramOpts, HistogramVec, Opts, register_counter,
    register_counter_vec, register_gauge, register_gauge_vec, register_histogram_vec,
};
use std::sync::LazyLock;

/// Total number of requests received by the load balancer
pub static REQUESTS_TOTAL: LazyLock<CounterVec> = LazyLock::new(|| {
    register_counter_vec!(
        Opts::new(
            "stepflow_lb_requests_total",
            "Total number of requests received by the load balancer"
        ),
        &["backend", "status", "method"]
    )
    .expect("Failed to register stepflow_lb_requests_total metric")
});

/// Request duration in milliseconds
pub static REQUEST_DURATION_MS: LazyLock<HistogramVec> = LazyLock::new(|| {
    register_histogram_vec!(
        HistogramOpts::new(
            "stepflow_lb_request_duration_ms",
            "Request duration in milliseconds"
        )
        .buckets(vec![
            1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0, 10000.0
        ]),
        &["backend", "method"]
    )
    .expect("Failed to register stepflow_lb_request_duration_ms metric")
});

/// Current number of active connections per backend
pub static ACTIVE_CONNECTIONS: LazyLock<GaugeVec> = LazyLock::new(|| {
    register_gauge_vec!(
        Opts::new(
            "stepflow_lb_active_connections",
            "Current number of active connections per backend"
        ),
        &["backend"]
    )
    .expect("Failed to register stepflow_lb_active_connections metric")
});

/// Backend health status (1 = healthy, 0 = unhealthy)
pub static BACKEND_HEALTH: LazyLock<GaugeVec> = LazyLock::new(|| {
    register_gauge_vec!(
        Opts::new(
            "stepflow_lb_backend_health",
            "Backend health status (1 = healthy, 0 = unhealthy)"
        ),
        &["backend", "instance_id"]
    )
    .expect("Failed to register stepflow_lb_backend_health metric")
});

/// Total number of healthy backends in the pool
pub static BACKEND_POOL_SIZE: LazyLock<Gauge> = LazyLock::new(|| {
    register_gauge!(Opts::new(
        "stepflow_lb_backend_pool_size",
        "Total number of healthy backends in the pool"
    ))
    .expect("Failed to register stepflow_lb_backend_pool_size metric")
});

/// Number of backend discovery operations
pub static DISCOVERY_OPERATIONS_TOTAL: LazyLock<Counter> = LazyLock::new(|| {
    register_counter!(Opts::new(
        "stepflow_lb_discovery_operations_total",
        "Total number of backend discovery operations"
    ))
    .expect("Failed to register stepflow_lb_discovery_operations_total metric")
});

/// Initialize all metrics by accessing them once
///
/// This ensures metrics are registered before Prometheus service starts
pub fn init_metrics() {
    // Access each metric to force registration
    let _ = &*REQUESTS_TOTAL;
    let _ = &*REQUEST_DURATION_MS;
    let _ = &*ACTIVE_CONNECTIONS;
    let _ = &*BACKEND_HEALTH;
    let _ = &*BACKEND_POOL_SIZE;
    let _ = &*DISCOVERY_OPERATIONS_TOTAL;
}
