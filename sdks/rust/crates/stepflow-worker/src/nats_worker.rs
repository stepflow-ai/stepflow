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

//! NATS JetStream worker transport.
//!
//! Connects to a NATS JetStream server, pulls task assignments from a durable
//! consumer, and delegates execution to the shared task handler. Task completion
//! is reported via gRPC OrchestratorService using the `orchestrator_service_url`
//! from each task's `TaskContext`.
//!
//! This mirrors the Python SDK's NATS transport (`nats_worker.py`), using
//! identical environment variables, subject naming, and protocol semantics.

use std::sync::Arc;
use std::time::Duration;

use async_nats::jetstream;
use async_nats::jetstream::ErrorCode;
use async_nats::jetstream::consumer::PullConsumer;
use async_nats::jetstream::context::GetStreamErrorKind;
use futures::StreamExt as _;
use prost::Message as _;
use tokio::task::JoinSet;
use tracing::{debug, info, warn};

use stepflow_proto::{CompleteTaskRequest, TaskAssignment, task_assignment::Task};

use crate::task_handler::{
    ChannelCache, handle_task, list_components_response, new_channel_cache, normalize_url,
};
use crate::worker::WorkerConfig;
use crate::{ComponentRegistry, WorkerError};

/// Run the NATS JetStream transport loop.
///
/// Connects to NATS, subscribes to a JetStream stream via a durable pull
/// consumer, and processes task assignments. Component execution and
/// completion reporting are delegated to the shared task handler (which
/// uses gRPC for heartbeats and `CompleteTask`).
///
/// Messages are acknowledged immediately upon receipt — NATS is purely a
/// delivery mechanism. The orchestrator is the single source of truth for
/// task lifecycle (heartbeat timeouts, retry budgets, attempt counting).
pub(crate) async fn run_nats_loop(
    registry: Arc<ComponentRegistry>,
    config: WorkerConfig,
    shutdown: impl std::future::Future<Output = ()>,
) -> Result<(), WorkerError> {
    let worker_id = format!("rust-worker-{}", uuid::Uuid::new_v4());

    info!(
        nats_url = %config.nats_url,
        stream = %config.nats_stream,
        consumer = %config.nats_consumer,
        worker_id = %worker_id,
        max_concurrent = config.max_concurrent,
        "Starting Stepflow worker (NATS transport)"
    );

    tokio::pin!(shutdown);

    let channel_cache = new_channel_cache();
    let mut consecutive_failures: u32 = 0;
    let mut in_flight: JoinSet<()> = JoinSet::new();

    'outer: loop {
        match fetch_loop(
            &registry,
            &config,
            &worker_id,
            &channel_cache,
            &mut in_flight,
            &mut shutdown,
        )
        .await
        {
            Ok(FetchOutcome::Shutdown) => break 'outer,
            Ok(FetchOutcome::StreamNotFound) => {
                // Stream not yet created by orchestrator — retry without
                // counting as a failure.
                debug!(
                    stream = %config.nats_stream,
                    "Stream not found, waiting for orchestrator to create it"
                );
                tokio::select! {
                    _ = &mut shutdown => break 'outer,
                    _ = tokio::time::sleep(Duration::from_secs(2)) => continue 'outer,
                }
            }
            Ok(FetchOutcome::Disconnected) => {
                consecutive_failures += 1;
                if consecutive_failures >= config.max_retries {
                    return Err(WorkerError::MaxRetriesExceeded {
                        attempts: consecutive_failures,
                    });
                }
                let backoff = Duration::from_secs(2u64.pow(consecutive_failures.min(6)).min(60));
                warn!(
                    consecutive_failures,
                    max_retries = config.max_retries,
                    "NATS connection lost, reconnecting in {}s",
                    backoff.as_secs()
                );
                tokio::select! {
                    _ = &mut shutdown => break 'outer,
                    _ = tokio::time::sleep(backoff) => continue 'outer,
                }
            }
            Err(e) => {
                consecutive_failures += 1;
                if consecutive_failures >= config.max_retries {
                    return Err(e);
                }
                let backoff = Duration::from_secs(2u64.pow(consecutive_failures.min(6)).min(60));
                warn!(
                    consecutive_failures,
                    max_retries = config.max_retries,
                    "NATS fetch error: {e}, reconnecting in {}s",
                    backoff.as_secs()
                );
                tokio::select! {
                    _ = &mut shutdown => break 'outer,
                    _ = tokio::time::sleep(backoff) => continue 'outer,
                }
            }
        }
    }

    // --- Graceful shutdown: wait for in-flight tasks ---
    info!(
        in_flight = in_flight.len(),
        grace_secs = config.shutdown_grace_secs,
        "Shutdown signal received, waiting for in-flight tasks"
    );

    let grace = Duration::from_secs(config.shutdown_grace_secs);
    let _ = tokio::time::timeout(grace, async {
        while in_flight.join_next().await.is_some() {}
    })
    .await;

    info!("Worker shut down cleanly");
    Ok(())
}

/// Outcome of a single NATS session.
enum FetchOutcome {
    /// Shutdown signal received — exit the outer loop.
    Shutdown,
    /// The JetStream stream does not exist yet.
    StreamNotFound,
    /// NATS connection was closed or lost.
    Disconnected,
}

/// A single NATS session: connect, subscribe, fetch and process tasks.
///
/// Returns when the connection drops, the stream disappears, or shutdown fires.
async fn fetch_loop(
    registry: &Arc<ComponentRegistry>,
    config: &WorkerConfig,
    worker_id: &str,
    channel_cache: &ChannelCache,
    in_flight: &mut JoinSet<()>,
    shutdown: &mut (impl std::future::Future<Output = ()> + Unpin),
) -> Result<FetchOutcome, WorkerError> {
    // --- Connect to NATS ---
    let client = async_nats::connect(&config.nats_url).await.map_err(|e| {
        WorkerError::Config(format!(
            "Failed to connect to NATS at '{}': {e}",
            config.nats_url
        ))
    })?;

    let js = jetstream::new(client);

    // --- Get or bind to the durable consumer ---
    let subject = format!("{}.tasks", config.nats_stream);

    let consumer: PullConsumer = match js.get_stream(&config.nats_stream).await {
        Ok(stream) => {
            match stream
                .get_or_create_consumer(
                    &config.nats_consumer,
                    jetstream::consumer::pull::Config {
                        durable_name: Some(config.nats_consumer.clone()),
                        filter_subject: subject.clone(),
                        deliver_policy: jetstream::consumer::DeliverPolicy::All,
                        ..Default::default()
                    },
                )
                .await
            {
                Ok(consumer) => consumer,
                Err(e) => {
                    return Err(WorkerError::Config(format!(
                        "Failed to create NATS consumer '{}': {e}",
                        config.nats_consumer
                    )));
                }
            }
        }
        Err(e) => {
            // Use typed error matching (same pattern as orchestrator's nats_journal.rs)
            if matches!(
                e.kind(),
                GetStreamErrorKind::JetStream(ref js_err)
                    if js_err.error_code() == ErrorCode::STREAM_NOT_FOUND
            ) {
                return Ok(FetchOutcome::StreamNotFound);
            }
            return Err(WorkerError::Config(format!(
                "Failed to get NATS stream '{}': {e}",
                config.nats_stream
            )));
        }
    };

    info!(
        stream = %config.nats_stream,
        subject = %subject,
        consumer = %config.nats_consumer,
        "NATS worker connected and listening for tasks"
    );

    // Resolve orchestrator URL for task callbacks
    let orchestrator_url = config
        .orchestrator_url
        .clone()
        .unwrap_or_else(|| config.tasks_url.clone());
    let default_normalised_url = normalize_url(&orchestrator_url);

    // Concurrency limiter — acquire BEFORE pulling from NATS so we only
    // pull tasks we can immediately process (natural backpressure).
    let semaphore = Arc::new(tokio::sync::Semaphore::new(config.max_concurrent));

    // --- Fetch loop ---
    loop {
        // Wait for a concurrency slot before pulling
        let permit = tokio::select! {
            _ = &mut *shutdown => return Ok(FetchOutcome::Shutdown),
            permit = semaphore.clone().acquire_owned() => {
                match permit {
                    Ok(p) => p,
                    Err(_) => return Ok(FetchOutcome::Shutdown),
                }
            }
        };

        // Pull one message from NATS with a timeout
        let mut messages = consumer
            .fetch()
            .max_messages(1)
            .expires(Duration::from_secs(5))
            .messages()
            .await
            .map_err(|e| WorkerError::Config(format!("Failed to fetch from NATS consumer: {e}")))?;

        let msg = tokio::select! {
            _ = &mut *shutdown => return Ok(FetchOutcome::Shutdown),
            item = messages.next() => {
                match item {
                    Some(Ok(msg)) => msg,
                    Some(Err(e)) => {
                        drop(permit);
                        warn!("NATS message error: {e}");
                        return Ok(FetchOutcome::Disconnected);
                    }
                    None => {
                        // Batch exhausted (timeout) — release slot and retry
                        drop(permit);
                        continue;
                    }
                }
            }
        };

        // ACK immediately — NATS is purely a delivery mechanism.
        // The orchestrator handles retry via heartbeat timeout detection.
        if let Err(e) = msg.ack().await {
            warn!("Failed to ack NATS message: {e}");
        }

        // Deserialize TaskAssignment from protobuf bytes
        let assignment = match TaskAssignment::decode(msg.payload.as_ref()) {
            Ok(a) => a,
            Err(e) => {
                warn!("Failed to decode TaskAssignment from NATS message: {e}");
                drop(permit);
                continue;
            }
        };

        let task_id = assignment.task_id.clone();

        // Handle ListComponents inline (lightweight — release concurrency slot)
        if matches!(&assignment.task, Some(Task::ListComponents(_))) {
            debug!(task_id, "Received list_components discovery task via NATS");
            drop(permit);
            let registry = Arc::clone(registry);
            let orch_url = orchestrator_url.clone();
            let default_norm = default_normalised_url.clone();
            let cache = Arc::clone(channel_cache);
            in_flight.spawn(async move {
                handle_list_components_nats(
                    assignment,
                    &registry,
                    &orch_url,
                    &default_norm,
                    &cache,
                )
                .await;
            });
            continue;
        }

        debug!(task_id, "Received execute task via NATS");

        // Spawn the full task handler (claim via heartbeat, execute, complete).
        // The permit is moved into the task and dropped when it completes,
        // releasing the concurrency slot.
        let registry = Arc::clone(registry);
        let orch_url = orchestrator_url.clone();
        let default_norm = default_normalised_url.clone();
        let blob_url = config.blob_url.clone();
        let blob_threshold = config.blob_threshold_bytes;
        let cache = Arc::clone(channel_cache);
        let wid = worker_id.to_string();

        in_flight.spawn(async move {
            // Build a lazy gRPC channel for the orchestrator.
            // The task handler needs a default channel — we create a lazy one
            // since NATS transport doesn't maintain a persistent gRPC connection
            // for task pulling.
            let task_context = assignment.context.clone().unwrap_or_default();
            let orch_target = if task_context.orchestrator_service_url.is_empty() {
                orch_url.clone()
            } else {
                task_context.orchestrator_service_url.clone()
            };

            let channel = match make_lazy_channel(&orch_target) {
                Some(ch) => ch,
                None => {
                    warn!(task_id, "Invalid orchestrator URL, skipping task");
                    drop(permit);
                    return;
                }
            };

            handle_task(
                assignment,
                registry,
                &wid,
                &orch_url,
                &default_norm,
                blob_url.as_deref(),
                blob_threshold,
                channel,
                &cache,
            )
            .await;

            drop(permit);
        });
    }
}

/// Handle a `ListComponentsRequest` task received via NATS.
///
/// Responds via gRPC `CompleteTask` with the worker's available components.
async fn handle_list_components_nats(
    assignment: TaskAssignment,
    registry: &ComponentRegistry,
    orchestrator_url: &str,
    default_normalised_url: &str,
    channel_cache: &ChannelCache,
) {
    let task_id = assignment.task_id.clone();
    let task_context = assignment.context.unwrap_or_default();

    let orch_url = if task_context.orchestrator_service_url.is_empty() {
        orchestrator_url.to_string()
    } else {
        task_context.orchestrator_service_url.clone()
    };

    // Build the ListComponents result from the registry
    let result = match assignment.task {
        Some(Task::ListComponents(req)) => match list_components_response(registry, req) {
            Ok(r) => r,
            Err(e) => {
                warn!(task_id, "Failed to build component list: {e}");
                return;
            }
        },
        _ => return,
    };

    // Send CompleteTask via gRPC
    let default_channel = match make_lazy_channel(&orch_url) {
        Some(ch) => ch,
        None => {
            warn!(
                task_id,
                orch_url, "Invalid orchestrator URL for list_components, skipping"
            );
            return;
        }
    };
    let channel = match crate::task_handler::resolve_orch_channel(
        &task_id,
        &orch_url,
        default_normalised_url,
        default_channel,
        channel_cache,
    ) {
        Some(ch) => ch,
        None => {
            warn!(
                task_id,
                "Cannot resolve orchestrator channel for list_components"
            );
            return;
        }
    };

    let mut client =
        stepflow_proto::orchestrator_service_client::OrchestratorServiceClient::new(channel);

    if let Err(e) = client
        .complete_task(CompleteTaskRequest {
            task_id: task_id.clone(),
            result: Some(result),
            run_id: None,
        })
        .await
    {
        warn!(task_id, "Failed to complete list_components task: {e}");
    } else {
        debug!(task_id, "Completed list_components discovery task");
    }
}

/// Create a lazily-connecting gRPC channel for the given URL.
///
/// Returns `None` if the URL is invalid.
fn make_lazy_channel(url: &str) -> Option<tonic::transport::Channel> {
    let with_scheme = if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("http://{url}")
    };

    tonic::transport::Channel::from_shared(with_scheme)
        .ok()
        .map(|endpoint| endpoint.connect_lazy())
}
