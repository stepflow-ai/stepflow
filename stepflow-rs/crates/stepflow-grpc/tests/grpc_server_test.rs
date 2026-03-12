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

//! Integration tests for [`StepflowGrpcServer`] with multiple plugins sharing
//! a single gRPC server.
//!
//! These tests verify the shared server architecture:
//! - Two plugins register separate queues on the same server
//! - Both plugins get the same server address
//! - Workers connect via gRPC and pull tasks from their respective queues
//! - Task completion round-trips through `OrchestratorService.CompleteTask`
//! - Blob service is accessible on the same address
//! - Error code mapping (TaskErrorCode → FlowError) works correctly

use std::sync::Arc;
use std::time::Duration;

use stepflow_grpc::PullTaskQueue;
use stepflow_grpc::grpc_server::StepflowGrpcServer;
use stepflow_grpc::proto::stepflow::v1::blob_service_client::BlobServiceClient;
use stepflow_grpc::proto::stepflow::v1::orchestrator_service_client::OrchestratorServiceClient;
use stepflow_grpc::proto::stepflow::v1::tasks_service_client::TasksServiceClient;
use stepflow_grpc::proto::stepflow::v1::{
    CompleteTaskRequest, ComponentExecuteResponse, ComponentInfo, GetBlobRequest, PullTasksRequest,
    PutBlobRequest, StartTaskRequest, TaskAssignment, TaskError, TaskHeartbeatRequest,
};

/// Create an in-memory environment suitable for the gRPC server.
async fn test_env() -> Arc<stepflow_core::StepflowEnvironment> {
    stepflow_plugin::build_in_memory_environment()
        .await
        .unwrap()
}

/// Start a `StepflowGrpcServer`, register two queues ("python" and "node"),
/// and return the server, queues, and bound address.
async fn setup_two_queue_server() -> (
    Arc<StepflowGrpcServer>,
    Arc<PullTaskQueue>,
    Arc<PullTaskQueue>,
    String,
) {
    let env = test_env().await;
    let server = Arc::new(StepflowGrpcServer::new());

    let python_queue = Arc::new(PullTaskQueue::new());
    let node_queue = Arc::new(PullTaskQueue::new());

    server.register_queue("python".to_string(), python_queue.clone());
    server.register_queue("node".to_string(), node_queue.clone());

    let address = server.ensure_started(&env).await.unwrap();

    (server, python_queue, node_queue, address)
}

fn endpoint(addr: &str) -> tonic::transport::Endpoint {
    tonic::transport::Channel::from_shared(format!("http://{addr}")).unwrap()
}

fn make_component(name: &str) -> ComponentInfo {
    ComponentInfo {
        name: name.to_string(),
        description: Some("test component".to_string()),
        input_schema: None,
        output_schema: None,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[tokio::test]
async fn test_two_plugins_share_same_address() {
    let env = test_env().await;
    let server = Arc::new(StepflowGrpcServer::new());

    // First plugin calls ensure_started
    let addr1 = server.ensure_started(&env).await.unwrap();

    // Second plugin calls ensure_started — should get the same address
    let addr2 = server.ensure_started(&env).await.unwrap();

    assert_eq!(
        addr1, addr2,
        "both plugins must get the same server address"
    );
}

#[tokio::test]
async fn test_queue_isolation() {
    let (_server, python_queue, node_queue, address) = setup_two_queue_server().await;

    let channel = endpoint(&address).connect().await.unwrap();

    // Connect a "python" worker
    let mut python_client = TasksServiceClient::new(channel.clone());
    let python_stream = python_client
        .pull_tasks(PullTasksRequest {
            queue_name: "python".to_string(),
            max_concurrent: 1,
            components: vec![make_component("/python/transform")],
        })
        .await
        .unwrap();
    let mut python_stream = python_stream.into_inner();

    // Connect a "node" worker
    let mut node_client = TasksServiceClient::new(channel.clone());
    let node_stream = node_client
        .pull_tasks(PullTasksRequest {
            queue_name: "node".to_string(),
            max_concurrent: 1,
            components: vec![make_component("/node/summarize")],
        })
        .await
        .unwrap();
    let mut node_stream = node_stream.into_inner();

    // Wait for workers to register
    tokio::time::timeout(Duration::from_secs(2), python_queue.wait_for_worker())
        .await
        .expect("python worker should connect");
    tokio::time::timeout(Duration::from_secs(2), node_queue.wait_for_worker())
        .await
        .expect("node worker should connect");

    // Push a task to the python queue only
    python_queue.push_task(TaskAssignment {
        task_id: "task-py-1".to_string(),
        request: None,
        deadline_secs: 30,
        heartbeat_interval_secs: 5,
        execution_timeout_secs: 0,
    });

    // Python worker should receive it
    let task = tokio::time::timeout(Duration::from_secs(2), python_stream.message())
        .await
        .expect("should not timeout")
        .expect("stream should not error")
        .expect("should receive a task");
    assert_eq!(task.task_id, "task-py-1");

    // Push a task to the node queue
    node_queue.push_task(TaskAssignment {
        task_id: "task-node-1".to_string(),
        request: None,
        deadline_secs: 30,
        heartbeat_interval_secs: 5,
        execution_timeout_secs: 0,
    });

    // Node worker should receive it
    let task = tokio::time::timeout(Duration::from_secs(2), node_stream.message())
        .await
        .expect("should not timeout")
        .expect("stream should not error")
        .expect("should receive a task");
    assert_eq!(task.task_id, "task-node-1");
}

#[tokio::test]
async fn test_unknown_queue_returns_not_found() {
    let (_server, _pq, _nq, address) = setup_two_queue_server().await;
    let channel = endpoint(&address).connect().await.unwrap();
    let mut client = TasksServiceClient::new(channel);

    let result = client
        .pull_tasks(PullTasksRequest {
            queue_name: "unknown".to_string(),
            max_concurrent: 1,
            components: vec![make_component("/unknown/comp")],
        })
        .await;

    assert!(result.is_err());
    let status = result.unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn test_complete_task_success_round_trip() {
    let (server, python_queue, _node_queue, address) = setup_two_queue_server().await;

    // Register a task in PendingTasks and get the receiver
    let rx = server.pending_tasks().register(
        "task-1".to_string(),
        "/python/transform".to_string(),
        Duration::from_secs(30),
        None,
    );

    // Push the task to the queue
    python_queue.push_task(TaskAssignment {
        task_id: "task-1".to_string(),
        request: None,
        deadline_secs: 30,
        heartbeat_interval_secs: 5,
        execution_timeout_secs: 0,
    });

    let channel = endpoint(&address).connect().await.unwrap();

    // Worker starts the task
    let mut orch_client = OrchestratorServiceClient::new(channel.clone());
    orch_client
        .start_task(StartTaskRequest {
            task_id: "task-1".to_string(),
        })
        .await
        .unwrap();

    // Worker sends heartbeat
    orch_client
        .task_heartbeat(TaskHeartbeatRequest {
            progress: None,
            status_message: None,
            task_id: "task-1".to_string(),
        })
        .await
        .unwrap();

    // Worker completes the task with a success result
    let output = prost_wkt_types::Value {
        kind: Some(prost_wkt_types::value::Kind::StringValue(
            "hello from worker".to_string(),
        )),
    };
    orch_client
        .complete_task(CompleteTaskRequest {
            task_id: "task-1".to_string(),
            result: Some(
                stepflow_grpc::proto::stepflow::v1::complete_task_request::Result::Response(
                    ComponentExecuteResponse {
                        output: Some(output),
                    },
                ),
            ),
        })
        .await
        .unwrap();

    // The plugin side should receive the result
    let result = tokio::time::timeout(Duration::from_secs(2), rx)
        .await
        .expect("should not timeout")
        .expect("channel should not be dropped");

    match result {
        stepflow_core::FlowResult::Success(value) => {
            assert_eq!(value.as_ref(), &serde_json::json!("hello from worker"),);
        }
        other => panic!("expected Success, got {other:?}"),
    }
}

#[tokio::test]
async fn test_complete_task_error_code_mapping() {
    let (server, _pq, _nq, address) = setup_two_queue_server().await;

    // Register tasks for different error codes
    let rx_component_failed = server.pending_tasks().register(
        "task-err-component".to_string(),
        "/python/fail".to_string(),
        Duration::from_secs(30),
        None,
    );
    let rx_invalid_input = server.pending_tasks().register(
        "task-err-input".to_string(),
        "/python/validate".to_string(),
        Duration::from_secs(30),
        None,
    );

    let channel = endpoint(&address).connect().await.unwrap();
    let mut orch_client = OrchestratorServiceClient::new(channel);

    // Complete with COMPONENT_FAILED (proto enum value 4) → should map to HTTP 500
    orch_client
        .complete_task(CompleteTaskRequest {
            task_id: "task-err-component".to_string(),
            result: Some(
                stepflow_grpc::proto::stepflow::v1::complete_task_request::Result::Error(
                    TaskError {
                        code: 4, // COMPONENT_FAILED
                        message: "component crashed".to_string(),
                    },
                ),
            ),
        })
        .await
        .unwrap();

    // Complete with INVALID_INPUT (proto enum value 3) → should map to HTTP 400
    orch_client
        .complete_task(CompleteTaskRequest {
            task_id: "task-err-input".to_string(),
            result: Some(
                stepflow_grpc::proto::stepflow::v1::complete_task_request::Result::Error(
                    TaskError {
                        code: 3, // INVALID_INPUT
                        message: "bad input".to_string(),
                    },
                ),
            ),
        })
        .await
        .unwrap();

    // Verify COMPONENT_FAILED → 500
    let result = rx_component_failed.await.unwrap();
    match result {
        stepflow_core::FlowResult::Failed(err) => {
            assert_eq!(err.code, 500, "COMPONENT_FAILED should map to HTTP 500");
            assert_eq!(err.message, "component crashed");
        }
        other => panic!("expected Failed, got {other:?}"),
    }

    // Verify INVALID_INPUT → 400
    let result = rx_invalid_input.await.unwrap();
    match result {
        stepflow_core::FlowResult::Failed(err) => {
            assert_eq!(err.code, 400, "INVALID_INPUT should map to HTTP 400");
            assert_eq!(err.message, "bad input");
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[tokio::test]
async fn test_complete_unknown_task_returns_not_found() {
    let (_server, _pq, _nq, address) = setup_two_queue_server().await;
    let channel = endpoint(&address).connect().await.unwrap();
    let mut orch_client = OrchestratorServiceClient::new(channel);

    let result = orch_client
        .complete_task(CompleteTaskRequest {
            task_id: "nonexistent".to_string(),
            result: Some(
                stepflow_grpc::proto::stepflow::v1::complete_task_request::Result::Response(
                    ComponentExecuteResponse { output: None },
                ),
            ),
        })
        .await;

    // CompleteTask with output: None should fail validation
    // (response.output is required), but let's check the unknown task case.
    // Actually the error for "output missing" comes before the task lookup.
    // Let's try with a valid output instead.
    drop(result);

    let output = prost_wkt_types::Value {
        kind: Some(prost_wkt_types::value::Kind::StringValue("ok".to_string())),
    };
    let result = orch_client
        .complete_task(CompleteTaskRequest {
            task_id: "nonexistent".to_string(),
            result: Some(
                stepflow_grpc::proto::stepflow::v1::complete_task_request::Result::Response(
                    ComponentExecuteResponse {
                        output: Some(output),
                    },
                ),
            ),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn test_blob_service_round_trip() {
    let (_server, _pq, _nq, address) = setup_two_queue_server().await;
    let channel = endpoint(&address).connect().await.unwrap();
    let mut blob_client = BlobServiceClient::new(channel);

    // Store a JSON blob
    let json_data = prost_wkt_types::Value {
        kind: Some(prost_wkt_types::value::Kind::StructValue(
            prost_wkt_types::Struct {
                fields: vec![(
                    "greeting".to_string(),
                    prost_wkt_types::Value {
                        kind: Some(prost_wkt_types::value::Kind::StringValue(
                            "hello world".to_string(),
                        )),
                    },
                )]
                .into_iter()
                .collect(),
            },
        )),
    };

    let put_response = blob_client
        .put_blob(PutBlobRequest {
            content: Some(
                stepflow_grpc::proto::stepflow::v1::put_blob_request::Content::JsonData(
                    json_data.clone(),
                ),
            ),
            blob_type: "data".to_string(),
            filename: None,
            content_type: None,
        })
        .await
        .unwrap()
        .into_inner();

    assert!(!put_response.blob_id.is_empty(), "blob_id should be set");

    // Retrieve the blob
    let get_response = blob_client
        .get_blob(GetBlobRequest {
            blob_id: put_response.blob_id.clone(),
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(get_response.blob_id, put_response.blob_id);
    assert_eq!(get_response.blob_type, "data");

    // Verify content round-trips
    match get_response.content {
        Some(stepflow_grpc::proto::stepflow::v1::get_blob_response::Content::JsonData(data)) => {
            let greeting = data
                .kind
                .as_ref()
                .and_then(|k| match k {
                    prost_wkt_types::value::Kind::StructValue(s) => s.fields.get("greeting"),
                    _ => None,
                })
                .and_then(|v| match &v.kind {
                    Some(prost_wkt_types::value::Kind::StringValue(s)) => Some(s.as_str()),
                    _ => None,
                });
            assert_eq!(greeting, Some("hello world"));
        }
        other => panic!("expected JsonData, got {other:?}"),
    }
}

#[tokio::test]
async fn test_blob_not_found() {
    let (_server, _pq, _nq, address) = setup_two_queue_server().await;
    let channel = endpoint(&address).connect().await.unwrap();
    let mut blob_client = BlobServiceClient::new(channel);

    let result = blob_client
        .get_blob(GetBlobRequest {
            blob_id: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn test_start_task_and_heartbeat() {
    let (server, _pq, _nq, address) = setup_two_queue_server().await;

    // Register a task
    let _rx = server.pending_tasks().register(
        "task-hb".to_string(),
        "/python/slow".to_string(),
        Duration::from_secs(30),
        None,
    );

    let channel = endpoint(&address).connect().await.unwrap();
    let mut orch_client = OrchestratorServiceClient::new(channel);

    // StartTask
    let resp = orch_client
        .start_task(StartTaskRequest {
            task_id: "task-hb".to_string(),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(!resp.timed_out);

    // Heartbeat
    let resp = orch_client
        .task_heartbeat(TaskHeartbeatRequest {
            progress: None,
            status_message: None,
            task_id: "task-hb".to_string(),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(!resp.should_cancel);

    // Heartbeat for unknown task
    let result = orch_client
        .task_heartbeat(TaskHeartbeatRequest {
            progress: None,
            status_message: None,
            task_id: "nonexistent".to_string(),
        })
        .await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::NotFound);
}
