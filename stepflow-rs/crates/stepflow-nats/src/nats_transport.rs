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

//! NATS JetStream task transport implementation.
//!
//! Each route (or plugin default) maps to a separate JetStream **stream**
//! with WorkQueue retention. Within each stream, tasks are published to
//! a fixed internal subject (`tasks`). Workers consume from the stream
//! and report completion via gRPC `OrchestratorService`.
//!
//! The stream-per-queue model gives each worker pool its own isolated
//! work queue with automatic message cleanup on ack.
//!
//! ## Ack semantics
//!
//! Workers should ack NATS messages immediately upon receipt, before
//! starting task execution. NATS is purely a delivery mechanism — the
//! orchestrator owns task lifecycle (heartbeat timeouts, retry budgets,
//! attempt counting). If a worker crashes between ack and the first
//! heartbeat, the orchestrator's queue timeout detects the missing
//! heartbeat and retries through its normal retry path. Delaying ack
//! would risk double-dispatch (NATS redelivery + orchestrator retry).

use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use error_stack::ResultExt as _;
use futures::stream::BoxStream;
use stepflow_core::component::ComponentInfo;
use stepflow_plugin::{PluginError, Result};

use stepflow_grpc::TaskAssignment;
use stepflow_grpc::task_transport::TaskTransport;

/// Fixed subject within each stream. All tasks in a stream are published
/// to this subject; workers subscribe to it via their durable consumer.
pub const TASK_SUBJECT: &str = "tasks";

/// Resolve the stream name from route params, falling back to the
/// plugin-level default.
pub(crate) fn resolve_stream(
    default_stream: Option<&str>,
    route_params: &HashMap<String, serde_json::Value>,
) -> Result<String> {
    // Route-level stream takes precedence
    if let Some(serde_json::Value::String(stream)) = route_params.get("stream") {
        return Ok(stream.clone());
    }

    // Fall back to plugin-level default
    default_stream.map(|s| s.to_string()).ok_or_else(|| {
        error_stack::report!(PluginError::Execution).attach_printable(
            "No NATS stream specified: set 'stream' in route params or plugin config",
        )
    })
}

/// NATS JetStream transport for task dispatch.
///
/// Each unique stream name gets its own JetStream stream with WorkQueue
/// retention. Streams are created lazily on first publish. Within each
/// stream, tasks are published to the fixed subject [`TASK_SUBJECT`].
/// Type alias for a NATS JetStream message stream.
type NatsMessageStream = BoxStream<
    'static,
    std::result::Result<
        async_nats::jetstream::Message,
        async_nats::error::Error<async_nats::jetstream::consumer::pull::MessagesErrorKind>,
    >,
>;

pub struct NatsTaskTransport {
    jetstream: async_nats::jetstream::Context,
    /// Plugin-level default stream name (e.g., "PYTHON_TASKS").
    default_stream: Option<String>,
    /// Cached component lists, keyed by stream name.
    component_cache: Arc<DashMap<String, Vec<ComponentInfo>>>,
    /// Cached message streams for recv_task (test-only), keyed by consumer name.
    /// Each stream is individually locked so concurrent recv_task calls on
    /// different subjects don't block each other.
    recv_streams: Arc<DashMap<String, Arc<tokio::sync::Mutex<NatsMessageStream>>>>,
}

impl NatsTaskTransport {
    /// Connect to NATS.
    ///
    /// Does NOT create any streams at connect time — streams are created
    /// lazily on first publish to each stream name.
    pub async fn connect(url: &str, default_stream: Option<String>) -> Result<Self> {
        let client = async_nats::connect(url)
            .await
            .change_context(PluginError::Initializing)
            .attach_printable_lazy(|| format!("Failed to connect to NATS at {url}"))?;

        let jetstream = async_nats::jetstream::new(client);

        log::info!("NATS transport connected to {url}");

        Ok(Self {
            jetstream,
            default_stream,
            component_cache: Arc::new(DashMap::new()),
            recv_streams: Arc::new(DashMap::new()),
        })
    }

    /// Ensure a JetStream stream exists with WorkQueue retention.
    ///
    /// Creates the stream if it doesn't exist. The stream captures a
    /// single subject (`{stream_name}.tasks`) and uses WorkQueue retention
    /// so messages are removed after ack.
    async fn ensure_stream(&self, stream_name: &str) -> Result<()> {
        let subject = format!("{stream_name}.{TASK_SUBJECT}");
        self.jetstream
            .get_or_create_stream(async_nats::jetstream::stream::Config {
                name: stream_name.to_string(),
                subjects: vec![subject],
                retention: async_nats::jetstream::stream::RetentionPolicy::WorkQueue,
                ..Default::default()
            })
            .await
            .change_context(PluginError::Execution)
            .attach_printable_lazy(|| {
                format!("Failed to create/update NATS stream '{stream_name}'")
            })?;
        Ok(())
    }

    /// Get the shared component cache.
    pub fn component_cache(&self) -> &Arc<DashMap<String, Vec<ComponentInfo>>> {
        &self.component_cache
    }
}

#[tonic::async_trait]
impl TaskTransport for NatsTaskTransport {
    async fn send_task(
        &self,
        task: TaskAssignment,
        route_params: &HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        let stream_name = resolve_stream(self.default_stream.as_deref(), route_params)?;

        // Ensure the stream exists (idempotent)
        self.ensure_stream(&stream_name).await?;

        let subject = format!("{stream_name}.{TASK_SUBJECT}");

        // Serialize and publish
        use prost::Message as _;
        let payload = task.encode_to_vec();

        self.jetstream
            .publish(subject.clone(), payload.into())
            .await
            .change_context(PluginError::Execution)
            .attach_printable_lazy(|| {
                format!("Failed to publish task to NATS stream '{stream_name}'")
            })?
            .await
            .change_context(PluginError::Execution)
            .attach_printable_lazy(|| {
                format!("NATS JetStream ack failed for stream '{stream_name}'")
            })?;

        log::debug!(
            "Published task {} to NATS stream '{stream_name}'",
            task.task_id
        );

        Ok(())
    }

    async fn list_components(&self) -> Result<Vec<ComponentInfo>> {
        let all_components: Vec<ComponentInfo> = self
            .component_cache
            .iter()
            .flat_map(|entry| entry.value().clone())
            .collect();
        Ok(all_components)
    }
}

#[tonic::async_trait]
impl stepflow_grpc::task_transport::TaskTransportRead for NatsTaskTransport {
    async fn recv_task(
        &self,
        stream_name: &str,
        timeout: std::time::Duration,
    ) -> Result<Option<TaskAssignment>> {
        use futures::StreamExt as _;
        use prost::Message as _;

        // Ensure the stream exists
        self.ensure_stream(stream_name).await?;

        let subject = format!("{stream_name}.{TASK_SUBJECT}");
        let consumer_name = format!("recv-{stream_name}");

        // Get or create a persistent message stream for this consumer.
        // The DashMap lookup is lock-free; only the individual stream's
        // mutex is held during the `.next().await` call, so concurrent
        // recv_task calls on different subjects don't block each other.
        let stream_mutex = if let Some(entry) = self.recv_streams.get(&consumer_name) {
            entry.value().clone()
        } else {
            let stream = self
                .jetstream
                .get_stream(stream_name)
                .await
                .change_context(PluginError::Execution)?;

            let consumer = stream
                .get_or_create_consumer(
                    &consumer_name,
                    async_nats::jetstream::consumer::pull::Config {
                        durable_name: Some(consumer_name.clone()),
                        filter_subject: subject,
                        ..Default::default()
                    },
                )
                .await
                .change_context(PluginError::Execution)
                .attach_printable("Failed to create consumer for recv_task")?;

            let msg_stream = consumer
                .messages()
                .await
                .change_context(PluginError::Execution)?
                .boxed();
            let mutex = Arc::new(tokio::sync::Mutex::new(msg_stream));
            self.recv_streams
                .entry(consumer_name.clone())
                .or_insert(mutex.clone());
            self.recv_streams
                .get(&consumer_name)
                .unwrap()
                .value()
                .clone()
        };

        let mut msg_stream = stream_mutex.lock().await;
        let fetch_result = tokio::time::timeout(timeout, msg_stream.next()).await;

        match fetch_result {
            Ok(Some(Ok(msg))) => {
                msg.ack().await.map_err(|e| {
                    error_stack::report!(PluginError::Execution)
                        .attach_printable(format!("Failed to ack NATS message: {e}"))
                })?;
                let task = TaskAssignment::decode(msg.payload.as_ref())
                    .change_context(PluginError::Execution)
                    .attach_printable("Failed to decode TaskAssignment")?;
                Ok(Some(task))
            }
            Ok(Some(Err(e))) => Err(error_stack::report!(PluginError::Execution)
                .attach_printable(format!("Error fetching NATS message: {e}"))),
            Ok(None) => Ok(None),
            Err(_) => Ok(None), // timeout
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_component_info(name: &str) -> ComponentInfo {
        ComponentInfo {
            component: stepflow_core::workflow::Component::from_string(name.to_string()),
            description: None,
            input_schema: None,
            output_schema: None,
        }
    }

    // =========================================================================
    // resolve_stream() tests
    // =========================================================================

    #[test]
    fn test_resolve_stream_from_route_params() {
        let mut params = HashMap::new();
        params.insert(
            "stream".to_string(),
            serde_json::Value::String("CUSTOM_STREAM".to_string()),
        );

        let result = resolve_stream(Some("DEFAULT_STREAM"), &params);
        assert_eq!(result.unwrap(), "CUSTOM_STREAM");
    }

    #[test]
    fn test_resolve_stream_falls_back_to_default() {
        let params = HashMap::new();
        let result = resolve_stream(Some("DEFAULT_STREAM"), &params);
        assert_eq!(result.unwrap(), "DEFAULT_STREAM");
    }

    #[test]
    fn test_resolve_stream_error_when_none() {
        let params = HashMap::new();
        let result = resolve_stream(None, &params);
        assert!(result.is_err());

        let err = format!("{:?}", result.unwrap_err());
        assert!(
            err.contains("No NATS stream specified"),
            "Error should mention stream, got: {err}"
        );
    }

    // =========================================================================
    // component_cache tests
    // =========================================================================

    #[tokio::test]
    async fn test_component_cache_empty_initially() {
        let cache: Arc<DashMap<String, Vec<ComponentInfo>>> = Arc::new(DashMap::new());
        let components: Vec<ComponentInfo> = cache
            .iter()
            .flat_map(|entry| entry.value().clone())
            .collect();
        assert!(components.is_empty());
    }

    #[tokio::test]
    async fn test_component_cache_populated() {
        let cache: Arc<DashMap<String, Vec<ComponentInfo>>> = Arc::new(DashMap::new());
        cache.insert(
            "PYTHON_TASKS".to_string(),
            vec![
                make_component_info("python/transform"),
                make_component_info("python/validate"),
            ],
        );
        cache.insert(
            "NODE_TASKS".to_string(),
            vec![make_component_info("node/summarize")],
        );

        let components: Vec<ComponentInfo> = cache
            .iter()
            .flat_map(|entry| entry.value().clone())
            .collect();
        assert_eq!(components.len(), 3);
    }
}
