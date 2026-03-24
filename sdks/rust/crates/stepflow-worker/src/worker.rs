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

//! Worker pull loop, reconnect logic, and graceful shutdown.

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt as _;
use tokio::task::JoinSet;
use tracing::{debug, info, warn};

use stepflow_proto::{PullTasksRequest, tasks_service_client::TasksServiceClient};

use crate::task_handler::handle_task;
use crate::{ComponentRegistry, WorkerError};

/// Configuration for a [`Worker`].
///
/// All fields are populated from environment variables by default (via clap `env`
/// annotations), so workers can be configured without any code changes.
///
/// # Environment variables
///
/// | Variable | Default | Description |
/// |---|---|---|
/// | `STEPFLOW_TASKS_URL` | `http://127.0.0.1:7837` | URL of the Stepflow tasks gRPC service |
/// | `STEPFLOW_QUEUE_NAME` | `default` | Worker queue name for task routing |
/// | `STEPFLOW_MAX_CONCURRENT` | `10` | Maximum number of tasks executed concurrently |
/// | `STEPFLOW_MAX_RETRIES` | `10` | Maximum consecutive connection failures before exiting |
/// | `STEPFLOW_SHUTDOWN_GRACE_SECS` | `30` | Seconds to wait for in-flight tasks during shutdown |
/// | `STEPFLOW_BLOB_URL` | *(none)* | Override URL for blob storage API |
/// | `STEPFLOW_ORCHESTRATOR_URL` | *(none)* | Override URL for OrchestratorService callbacks |
#[derive(Debug, Clone, clap::Args)]
pub struct WorkerConfig {
    /// URL of the Stepflow tasks gRPC service.
    #[arg(
        long,
        env = "STEPFLOW_TASKS_URL",
        default_value = "http://127.0.0.1:7837"
    )]
    pub tasks_url: String,

    /// Worker queue name for task routing.
    #[arg(long, env = "STEPFLOW_QUEUE_NAME", default_value = "default")]
    pub queue_name: String,

    /// Maximum number of tasks to execute concurrently.
    #[arg(long, env = "STEPFLOW_MAX_CONCURRENT", default_value_t = 10)]
    pub max_concurrent: usize,

    /// Maximum consecutive connection failures before the worker exits.
    #[arg(long, env = "STEPFLOW_MAX_RETRIES", default_value_t = 10)]
    pub max_retries: u32,

    /// Seconds to wait for in-flight tasks during graceful shutdown.
    #[arg(long, env = "STEPFLOW_SHUTDOWN_GRACE_SECS", default_value_t = 30)]
    pub shutdown_grace_secs: u64,

    /// URL for the blob storage API (overrides the tasks service host).
    #[arg(long, env = "STEPFLOW_BLOB_URL")]
    pub blob_url: Option<String>,

    /// URL for OrchestratorService callbacks (overrides the URL in `TaskContext`).
    #[arg(long, env = "STEPFLOW_ORCHESTRATOR_URL")]
    pub orchestrator_url: Option<String>,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            tasks_url: "http://127.0.0.1:7837".to_string(),
            queue_name: "default".to_string(),
            max_concurrent: 10,
            max_retries: 10,
            shutdown_grace_secs: 30,
            blob_url: None,
            orchestrator_url: None,
        }
    }
}

/// A Stepflow worker that pulls tasks from the orchestrator and executes registered components.
///
/// # Example
///
/// ```rust,no_run
/// use stepflow_worker::{ComponentRegistry, Worker, WorkerConfig, ComponentError};
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Deserialize)]
/// struct Input { value: i64 }
///
/// #[derive(Serialize)]
/// struct Output { doubled: i64 }
///
/// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
/// let mut registry = ComponentRegistry::new();
/// registry.register_fn("/math/double", |input: Input, _ctx| async move {
///     Ok(Output { doubled: input.value * 2 })
/// });
///
/// Worker::new(registry, WorkerConfig::default())
///     .run()
///     .await?;
/// # Ok(())
/// # }
/// ```
pub struct Worker {
    registry: Arc<ComponentRegistry>,
    config: WorkerConfig,
}

impl Worker {
    /// Create a new worker with the given registry and configuration.
    pub fn new(registry: ComponentRegistry, config: WorkerConfig) -> Self {
        Self {
            registry: Arc::new(registry),
            config,
        }
    }

    /// Run the worker until `SIGTERM` or `SIGINT` is received.
    ///
    /// Waits up to `WorkerConfig::shutdown_grace_secs` for in-flight tasks to complete
    /// before returning.
    pub async fn run(self) -> Result<(), WorkerError> {
        let shutdown = async {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to listen for Ctrl-C");
        };
        self.run_until(shutdown).await
    }

    /// Run the worker until the given `shutdown` future resolves.
    ///
    /// This is useful for tests or embeddings where you control the shutdown signal.
    pub async fn run_until(
        self,
        shutdown: impl std::future::Future<Output = ()>,
    ) -> Result<(), WorkerError> {
        let config = self.config;
        let registry = self.registry;

        // Resolve the effective orchestrator URL for task callbacks
        let orchestrator_url = config
            .orchestrator_url
            .clone()
            .unwrap_or_else(|| config.tasks_url.clone());

        let worker_id = format!("rust-worker-{}", uuid::Uuid::new_v4());

        info!(
            tasks_url = %config.tasks_url,
            queue_name = %config.queue_name,
            worker_id = %worker_id,
            "Starting Stepflow worker"
        );

        // Pin shutdown so it can be used in select!
        tokio::pin!(shutdown);

        let mut consecutive_failures: u32 = 0;
        let mut in_flight: JoinSet<()> = JoinSet::new();

        'outer: loop {
            // --- Connect to the tasks service ---
            let endpoint = match tonic::transport::Channel::from_shared(config.tasks_url.clone()) {
                Ok(ep) => ep,
                Err(e) => {
                    return Err(WorkerError::Config(format!(
                        "Invalid tasks URL '{}': {e}",
                        config.tasks_url
                    )));
                }
            };

            let channel = match endpoint.connect().await {
                Ok(ch) => {
                    consecutive_failures = 0;
                    ch
                }
                Err(e) => {
                    consecutive_failures += 1;
                    warn!(
                        consecutive_failures,
                        max_retries = config.max_retries,
                        "Failed to connect to tasks service: {e}"
                    );
                    if consecutive_failures >= config.max_retries {
                        return Err(WorkerError::MaxRetriesExceeded {
                            attempts: consecutive_failures,
                        });
                    }
                    let backoff =
                        Duration::from_secs((2u64.pow(consecutive_failures.min(6))).min(60));
                    tokio::select! {
                        _ = &mut shutdown => break 'outer,
                        _ = tokio::time::sleep(backoff) => continue 'outer,
                    }
                }
            };

            let mut tasks_client = TasksServiceClient::new(channel.clone());

            // --- Open the PullTasks stream ---
            let stream = match tasks_client
                .pull_tasks(PullTasksRequest {
                    queue_name: config.queue_name.clone(),
                    worker_id: worker_id.clone(),
                })
                .await
            {
                Ok(r) => r.into_inner(),
                Err(e) => {
                    consecutive_failures += 1;
                    warn!(consecutive_failures, "pull_tasks failed: {e}");
                    if consecutive_failures >= config.max_retries {
                        return Err(WorkerError::MaxRetriesExceeded {
                            attempts: consecutive_failures,
                        });
                    }
                    continue 'outer;
                }
            };

            tokio::pin!(stream);

            // --- Process assignments ---
            loop {
                // Throttle: wait for a slot if at max concurrency
                while in_flight.len() >= config.max_concurrent {
                    tokio::select! {
                        _ = &mut shutdown => break,
                        _ = in_flight.join_next() => {}
                    }
                }

                tokio::select! {
                    _ = &mut shutdown => break 'outer,
                    completed = in_flight.join_next(), if !in_flight.is_empty() => {
                        // Task finished — nothing to do, just reclaim the slot
                        let _ = completed;
                    }
                    item = stream.next() => {
                        match item {
                            None => {
                                // Stream ended — reconnect
                                debug!("PullTasks stream ended, reconnecting");
                                consecutive_failures += 1;
                                if consecutive_failures >= config.max_retries {
                                    return Err(WorkerError::MaxRetriesExceeded {
                                        attempts: consecutive_failures,
                                    });
                                }
                                break; // inner loop → reconnect outer loop
                            }
                            Some(Err(e)) => {
                                warn!("PullTasks stream error: {e}");
                                consecutive_failures += 1;
                                if consecutive_failures >= config.max_retries {
                                    return Err(WorkerError::Stream(e));
                                }
                                break; // inner loop → reconnect
                            }
                            Some(Ok(assignment)) => {
                                consecutive_failures = 0;
                                let registry = Arc::clone(&registry);
                                let worker_id = worker_id.clone();
                                let orchestrator_url = orchestrator_url.clone();
                                let blob_url = config.blob_url.clone();
                                let channel = channel.clone();

                                in_flight.spawn(async move {
                                    handle_task(
                                        assignment,
                                        registry,
                                        &worker_id,
                                        &orchestrator_url,
                                        blob_url.as_deref(),
                                        channel,
                                    )
                                    .await;
                                });
                            }
                        }
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
}
