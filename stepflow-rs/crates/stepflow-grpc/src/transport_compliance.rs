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

//! Compliance test suite for [`TaskTransport`] implementations.
//!
//! Provides two levels of compliance testing:
//!
//! - **Basic tests** ([`TransportComplianceTests::run_all`]) — require only
//!   [`TaskTransport`]. Verify that send, list_components, and component_info
//!   behave correctly.
//!
//! - **Round-trip tests** ([`TransportComplianceTests::run_all_readable`]) —
//!   require [`TaskTransportRead`] (extends `TaskTransport` with a `recv_task`
//!   method). Verify end-to-end delivery: tasks sent via `send_task` can be
//!   consumed via `recv_task`, FIFO ordering is preserved, and route-param
//!   routing works correctly.
//!
//! # Usage
//!
//! ```ignore
//! use stepflow_grpc::transport_compliance::TransportComplianceTests;
//!
//! // Basic compliance (TaskTransport only):
//! TransportComplianceTests::run_all(|| async {
//!     Box::new(MyTransport::new()) as Box<dyn TaskTransport>
//! }).await;
//!
//! // Full compliance with round-trip (TaskTransportRead):
//! TransportComplianceTests::run_all_readable(
//!     || async {
//!         let t = Box::new(MyTransport::new()) as Box<dyn TaskTransportRead>;
//!         (t, "default-queue".to_string(), "alt-queue".to_string())
//!     },
//!     "queueName",           // route param key for this transport
//! ).await;
//! ```

use std::collections::HashMap;
use std::future::Future;
use std::time::Duration;

use crate::proto::stepflow::v1::TaskAssignment;
use crate::task_transport::{TaskTransport, TaskTransportRead};

/// Create a minimal [`TaskAssignment`] for testing.
fn make_task(id: &str) -> TaskAssignment {
    TaskAssignment {
        task_id: id.to_string(),
        task: None,
        context: None,
        deadline_secs: 30,
        heartbeat_interval_secs: 1,
        execution_timeout_secs: 0,
    }
}

/// Compliance test suite for TaskTransport implementations.
pub struct TransportComplianceTests;

impl TransportComplianceTests {
    /// Run basic compliance tests (TaskTransport only).
    pub async fn run_all<F, Fut>(factory: F)
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Box<dyn TaskTransport>>,
    {
        Self::test_send_task_succeeds(&factory).await;
        Self::test_send_task_with_route_params(&factory).await;
        Self::test_list_components_empty_initially(&factory).await;
        Self::test_component_info_not_found(&factory).await;
    }

    /// Run the full compliance suite including round-trip tests.
    ///
    /// Requires [`TaskTransportRead`] so tests can verify tasks arrive
    /// at the correct queue/subject after being sent.
    ///
    /// # Arguments
    /// * `factory` — creates a fresh `(transport, default_queue, alt_queue)` tuple
    ///   per test. Each invocation should use unique queue/subject names to avoid
    ///   cross-test interference (important for durable backends like NATS).
    /// * `route_param_key` — the route param key that overrides the destination
    ///   (e.g., `"queueName"` for in-memory, `"subject"` for NATS)
    pub async fn run_all_readable<F, Fut>(factory: F, route_param_key: &str)
    where
        F: Fn() -> Fut,
        Fut: Future<Output = (Box<dyn TaskTransportRead>, String, String)>,
    {
        // Basic tests
        Self::test_list_components_empty_initially_read2(&factory).await;
        Self::test_component_info_not_found_read2(&factory).await;

        // Round-trip tests — each gets isolated queue/subject names
        Self::test_round_trip2(&factory).await;
        Self::test_fifo_ordering2(&factory).await;
        Self::test_route_param_routing2(&factory, route_param_key).await;
        Self::test_recv_timeout_on_empty_queue2(&factory).await;
    }

    // =========================================================================
    // Basic tests (TaskTransport only)
    // =========================================================================

    /// send_task with empty route params succeeds.
    pub async fn test_send_task_succeeds<F, Fut>(factory: &F)
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Box<dyn TaskTransport>>,
    {
        let transport = factory().await;
        let empty_params = HashMap::new();
        transport
            .send_task(make_task("compliance-send-basic"), &empty_params)
            .await
            .expect("send_task with empty route_params should succeed");
    }

    /// send_task with route params succeeds.
    pub async fn test_send_task_with_route_params<F, Fut>(factory: &F)
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Box<dyn TaskTransport>>,
    {
        let transport = factory().await;
        let mut params = HashMap::new();
        params.insert(
            "testKey".to_string(),
            serde_json::Value::String("testValue".to_string()),
        );
        transport
            .send_task(make_task("compliance-send-params"), &params)
            .await
            .expect("send_task with route_params should succeed");
    }

    /// list_components on fresh transport returns empty.
    pub async fn test_list_components_empty_initially<F, Fut>(factory: &F)
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Box<dyn TaskTransport>>,
    {
        let transport = factory().await;
        let components = transport
            .list_components()
            .await
            .expect("list_components should succeed");
        assert!(
            components.is_empty(),
            "Expected empty component list on fresh transport, got {} components",
            components.len()
        );
    }

    /// component_info for nonexistent component returns error.
    pub async fn test_component_info_not_found<F, Fut>(factory: &F)
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Box<dyn TaskTransport>>,
    {
        let transport = factory().await;
        let result = transport.component_info("nonexistent/component").await;
        assert!(
            result.is_err(),
            "component_info for nonexistent component should return error"
        );
    }

    // =========================================================================
    // Basic tests (TaskTransportRead — tuple factory)
    // =========================================================================

    async fn test_list_components_empty_initially_read2<F, Fut>(factory: &F)
    where
        F: Fn() -> Fut,
        Fut: Future<Output = (Box<dyn TaskTransportRead>, String, String)>,
    {
        let (transport, _, _) = factory().await;
        let components = transport
            .list_components()
            .await
            .expect("list_components should succeed");
        assert!(components.is_empty());
    }

    async fn test_component_info_not_found_read2<F, Fut>(factory: &F)
    where
        F: Fn() -> Fut,
        Fut: Future<Output = (Box<dyn TaskTransportRead>, String, String)>,
    {
        let (transport, _, _) = factory().await;
        assert!(
            transport
                .component_info("nonexistent/component")
                .await
                .is_err()
        );
    }

    // =========================================================================
    // Round-trip tests (require TaskTransportRead + isolated queues)
    // =========================================================================

    /// A task sent via send_task can be received via recv_task.
    async fn test_round_trip2<F, Fut>(factory: &F)
    where
        F: Fn() -> Fut,
        Fut: Future<Output = (Box<dyn TaskTransportRead>, String, String)>,
    {
        let (transport, default_queue, _) = factory().await;
        let empty_params = HashMap::new();
        transport
            .send_task(make_task("rt-1"), &empty_params)
            .await
            .expect("send_task should succeed");

        let received = transport
            .recv_task(&default_queue, Duration::from_secs(5))
            .await
            .expect("recv_task should succeed")
            .expect("should receive the task");

        assert_eq!(received.task_id, "rt-1");
    }

    /// Tasks are delivered in FIFO order.
    async fn test_fifo_ordering2<F, Fut>(factory: &F)
    where
        F: Fn() -> Fut,
        Fut: Future<Output = (Box<dyn TaskTransportRead>, String, String)>,
    {
        let (transport, default_queue, _) = factory().await;
        let params = HashMap::new();

        for i in 0..3 {
            transport
                .send_task(make_task(&format!("fifo-{i}")), &params)
                .await
                .unwrap();
        }

        for i in 0..3 {
            let task = transport
                .recv_task(&default_queue, Duration::from_secs(5))
                .await
                .unwrap()
                .unwrap_or_else(|| panic!("should receive task fifo-{i}"));
            assert_eq!(
                task.task_id,
                format!("fifo-{i}"),
                "Tasks should be delivered in FIFO order"
            );
        }
    }

    /// Route params direct tasks to a different queue/subject.
    async fn test_route_param_routing2<F, Fut>(factory: &F, route_param_key: &str)
    where
        F: Fn() -> Fut,
        Fut: Future<Output = (Box<dyn TaskTransportRead>, String, String)>,
    {
        let (transport, default_queue, alt_queue) = factory().await;

        let empty_params = HashMap::new();
        transport
            .send_task(make_task("routing-default"), &empty_params)
            .await
            .unwrap();

        let mut alt_params = HashMap::new();
        alt_params.insert(
            route_param_key.to_string(),
            serde_json::Value::String(alt_queue.clone()),
        );
        transport
            .send_task(make_task("routing-alt"), &alt_params)
            .await
            .unwrap();

        let default_task = transport
            .recv_task(&default_queue, Duration::from_secs(5))
            .await
            .unwrap()
            .expect("should receive task on default queue");
        assert_eq!(default_task.task_id, "routing-default");

        let alt_task = transport
            .recv_task(&alt_queue, Duration::from_secs(5))
            .await
            .unwrap()
            .expect("should receive task on alt queue");
        assert_eq!(alt_task.task_id, "routing-alt");
    }

    /// recv_task on an empty queue returns None after timeout.
    async fn test_recv_timeout_on_empty_queue2<F, Fut>(factory: &F)
    where
        F: Fn() -> Fut,
        Fut: Future<Output = (Box<dyn TaskTransportRead>, String, String)>,
    {
        let (transport, default_queue, _) = factory().await;
        let result = transport
            .recv_task(&default_queue, Duration::from_millis(200))
            .await
            .expect("recv_task should succeed (not error)");
        assert!(
            result.is_none(),
            "Empty queue should return None on timeout"
        );
    }
}
