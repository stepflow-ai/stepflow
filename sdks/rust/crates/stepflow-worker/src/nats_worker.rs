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

use stepflow_proto::TaskAssignment;

use crate::WorkerError;
use crate::executor::TaskExecutor;
use crate::worker::WorkerConfig;

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
    executor: Arc<dyn TaskExecutor>,
    config: WorkerConfig,
    worker_id: &str,
    shutdown: impl std::future::Future<Output = ()>,
) -> Result<(), WorkerError> {
    info!(
        nats_url = %redact_url_credentials(&config.nats_url),
        stream = %config.nats_stream,
        consumer = %config.nats_consumer,
        worker_id = %worker_id,
        max_concurrent = config.max_concurrent,
        "Starting Stepflow worker (NATS transport)"
    );

    tokio::pin!(shutdown);

    let mut consecutive_failures: u32 = 0;
    let mut in_flight: JoinSet<()> = JoinSet::new();

    'outer: loop {
        match fetch_loop(&executor, &config, &mut in_flight, &mut shutdown).await {
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
            Ok(FetchOutcome::DisconnectedAfterSuccess) => {
                // Had a successful session — reset failure counter before reconnecting.
                consecutive_failures = 0;
                warn!("NATS connection lost after successful session, reconnecting");
                tokio::select! {
                    _ = &mut shutdown => break 'outer,
                    _ = tokio::time::sleep(Duration::from_secs(2)) => continue 'outer,
                }
            }
            Ok(FetchOutcome::DisconnectedBeforeSuccess) => {
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
                    "NATS connection failed, reconnecting in {}s",
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
    /// Successfully connected and processed tasks, then lost the connection.
    /// The outer loop should reset the consecutive failure counter.
    DisconnectedAfterSuccess,
    /// Failed to connect or bind the consumer (never processed any tasks).
    DisconnectedBeforeSuccess,
}

/// A single NATS session: connect, subscribe, fetch and process tasks.
///
/// Returns when the connection drops, the stream disappears, or shutdown fires.
async fn fetch_loop(
    executor: &Arc<dyn TaskExecutor>,
    config: &WorkerConfig,
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

    // Successfully connected — any disconnect from here is "after success".
    let connected = true;

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
                        return Ok(if connected {
                            FetchOutcome::DisconnectedAfterSuccess
                        } else {
                            FetchOutcome::DisconnectedBeforeSuccess
                        });
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

        debug!(task_id = assignment.task_id, "Received task via NATS");

        // Spawn the executor. The permit is moved into the task and dropped
        // when it completes, releasing the concurrency slot.
        let executor = Arc::clone(executor);

        in_flight.spawn(async move {
            executor.execute_task(assignment).await;
            drop(permit);
        });
    }
}

/// Redact userinfo (credentials) from a URL for safe logging.
///
/// `nats://user:pass@host:4222` → `nats://***@host:4222`
fn redact_url_credentials(url: &str) -> String {
    // Find "://" then check for "@" before the next "/"
    let Some(scheme_end) = url.find("://") else {
        return url.to_string();
    };
    let after_scheme = scheme_end + 3;
    let authority_end = url[after_scheme..]
        .find('/')
        .map(|i| after_scheme + i)
        .unwrap_or(url.len());
    let authority = &url[after_scheme..authority_end];

    if let Some(at_pos) = authority.find('@') {
        format!(
            "{}***@{}{}",
            &url[..after_scheme],
            &authority[at_pos + 1..],
            &url[authority_end..]
        )
    } else {
        url.to_string()
    }
}
