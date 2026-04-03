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

//! Instant-completion mode: pulls tasks via raw gRPC and completes them
//! immediately, bypassing the worker SDK.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use clap::Args;
use futures::StreamExt as _;
use tracing::{error, info, warn};

use stepflow_proto::{
    CompleteTaskRequest, ComponentExecuteResponse, ComponentInfo as ProtoComponentInfo,
    ListComponentsResult, TaskAssignment, TaskHeartbeatRequest, TaskStatus,
    complete_task_request::Result as TaskResult,
    orchestrator_service_client::OrchestratorServiceClient, task_assignment::Task,
};
use stepflow_worker::open_task_stream;

#[derive(Args, Debug)]
pub struct InstantArgs {
    /// URL of the Stepflow tasks gRPC service.
    #[arg(
        long,
        env = "STEPFLOW_TASKS_URL",
        default_value = "http://127.0.0.1:7840"
    )]
    tasks_url: String,

    /// Queue name to pull tasks from.
    #[arg(long, env = "STEPFLOW_QUEUE_NAME", default_value = "bench")]
    queue_name: String,

    /// Optional delay (ms) before completing each task. Simulates work.
    #[arg(long, default_value = "0")]
    delay_ms: u64,

    /// Number of concurrent pull streams (simulates multiple workers).
    #[arg(long, default_value = "1")]
    workers: usize,
}

pub async fn run(args: InstantArgs) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        tasks_url = %args.tasks_url,
        queue_name = %args.queue_name,
        delay_ms = args.delay_ms,
        workers = args.workers,
        "Starting instant-completion worker"
    );

    let completed = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicU64::new(0));

    // Spawn stats reporter
    let stats_completed = completed.clone();
    let stats_errors = errors.clone();
    let stats_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        let mut last_count = 0u64;
        loop {
            interval.tick().await;
            let current = stats_completed.load(Ordering::Relaxed);
            let err_count = stats_errors.load(Ordering::Relaxed);
            let delta = current - last_count;
            let rps = delta as f64 / 5.0;
            info!(
                completed = current,
                errors = err_count,
                "tasks/sec (5s avg)" = format!("{rps:.1}"),
                "Stats"
            );
            last_count = current;
        }
    });

    // Spawn worker tasks
    let mut handles = Vec::with_capacity(args.workers);
    for i in 0..args.workers {
        let tasks_url = args.tasks_url.clone();
        let queue_name = args.queue_name.clone();
        let delay_ms = args.delay_ms;
        let completed = completed.clone();
        let errors = errors.clone();

        handles.push(tokio::spawn(async move {
            let worker_id = uuid::Uuid::new_v4().to_string();
            info!(worker = i, worker_id = %worker_id, "Connecting to task stream");

            loop {
                match open_task_stream(&tasks_url, &queue_name, &worker_id).await {
                    Ok((channel, mut stream)) => {
                        info!(worker = i, "Connected, pulling tasks");

                        while let Some(result) = stream.next().await {
                            match result {
                                Ok(assignment) => {
                                    handle_assignment(
                                        assignment,
                                        &worker_id,
                                        channel.clone(),
                                        &tasks_url,
                                        delay_ms,
                                        &completed,
                                        &errors,
                                    )
                                    .await;
                                }
                                Err(e) => {
                                    warn!(worker = i, "Stream error: {e}");
                                    break;
                                }
                            }
                        }

                        warn!(worker = i, "Stream ended, reconnecting in 1s");
                    }
                    Err(e) => {
                        error!(worker = i, "Failed to connect: {e}");
                    }
                }

                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }));
    }

    // Wait for Ctrl+C
    tokio::signal::ctrl_c().await?;
    info!("Shutting down...");

    let total = completed.load(Ordering::Relaxed);
    let total_errors = errors.load(Ordering::Relaxed);
    info!(
        total_completed = total,
        total_errors = total_errors,
        "Final stats"
    );

    stats_handle.abort();
    for h in handles {
        h.abort();
    }

    Ok(())
}

async fn handle_assignment(
    assignment: TaskAssignment,
    worker_id: &str,
    default_channel: tonic::transport::Channel,
    tasks_url: &str,
    delay_ms: u64,
    completed: &AtomicU64,
    errors: &AtomicU64,
) {
    let task_id = assignment.task_id.clone();
    let task_context = assignment.context.clone().unwrap_or_default();

    // Resolve orchestrator channel — reuse default_channel when possible
    // to avoid ephemeral port exhaustion under high throughput.
    let orch_channel = if task_context.orchestrator_service_url.is_empty() {
        default_channel
    } else {
        let url = &task_context.orchestrator_service_url;
        let normalised = if url.starts_with("http://") || url.starts_with("https://") {
            url.to_string()
        } else {
            format!("http://{url}")
        };
        // Reuse default channel if the URL matches the tasks URL
        if normalised == tasks_url {
            default_channel
        } else {
            match tonic::transport::Channel::from_shared(normalised) {
                Ok(endpoint) => endpoint.connect_lazy(),
                Err(e) => {
                    warn!(task_id, "Invalid orchestrator URL: {e}");
                    errors.fetch_add(1, Ordering::Relaxed);
                    return;
                }
            }
        }
    };

    let mut orch_client = OrchestratorServiceClient::new(orch_channel.clone());

    // Claim the task via initial heartbeat
    let claim_result = orch_client
        .task_heartbeat(TaskHeartbeatRequest {
            task_id: task_id.clone(),
            worker_id: worker_id.to_string(),
            progress: None,
            status_message: None,
            run_id: None,
        })
        .await;

    match claim_result {
        Ok(resp) => {
            if resp.into_inner().status == TaskStatus::AlreadyClaimed as i32 {
                return;
            }
        }
        Err(e) => {
            warn!(task_id, "Failed to claim task: {e}");
            errors.fetch_add(1, Ordering::Relaxed);
            return;
        }
    }

    // Optional delay (with heartbeat if delay is long)
    if delay_ms > 0 {
        let heartbeat_interval = assignment.heartbeat_interval_secs;
        if delay_ms > (heartbeat_interval as u64 * 1000) && heartbeat_interval > 0 {
            // Need heartbeat loop for long delays
            let hb_task_id = task_id.clone();
            let hb_worker_id = worker_id.to_string();
            let hb_channel = orch_channel.clone();
            let hb_handle = tokio::spawn(async move {
                let mut client = OrchestratorServiceClient::new(hb_channel);
                let interval = Duration::from_secs(heartbeat_interval.max(1) as u64);
                loop {
                    tokio::time::sleep(interval).await;
                    let _ = client
                        .task_heartbeat(TaskHeartbeatRequest {
                            task_id: hb_task_id.clone(),
                            worker_id: hb_worker_id.clone(),
                            progress: None,
                            status_message: None,
                            run_id: None,
                        })
                        .await;
                }
            });

            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            hb_handle.abort();
        } else {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
    }

    // Build result based on task type
    let complete_result = match assignment.task {
        Some(Task::Execute(req)) => {
            // Echo input back as output
            TaskResult::Response(ComponentExecuteResponse { output: req.input })
        }
        Some(Task::ListComponents(_)) => TaskResult::ListComponents(ListComponentsResult {
            components: vec![ProtoComponentInfo {
                component_id: "noop".to_string(),
                path: "/noop".to_string(),
                description: Some("Benchmark noop component".to_string()),
                input_schema: None,
                output_schema: None,
            }],
        }),
        None => {
            warn!(task_id, "Empty task payload");
            errors.fetch_add(1, Ordering::Relaxed);
            return;
        }
    };

    // Complete the task
    if let Err(e) = OrchestratorServiceClient::new(orch_channel)
        .complete_task(CompleteTaskRequest {
            task_id: task_id.clone(),
            result: Some(complete_result),
            run_id: None,
        })
        .await
    {
        warn!(task_id, "Failed to complete task: {e}");
        errors.fetch_add(1, Ordering::Relaxed);
        return;
    }

    completed.fetch_add(1, Ordering::Relaxed);
}
