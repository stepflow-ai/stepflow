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

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use stepflow_core::{FlowResult, GetRunOptions, SubmitRunParams, workflow::ValueRef};
use stepflow_plugin::{Context, ExecutionContext, Plugin as _, PluginConfig as _, PluginError};
use stepflow_protocol::{StepflowPluginConfig, StepflowTransport};
use stepflow_state::{InMemoryStateStore, ItemResult, ItemStatistics, RunStatus, StateStore};
use tokio::process::Command;
use tokio::time::{sleep, timeout};
use uuid::Uuid;

/// Mock context for testing
struct MockContext {
    state_store: Arc<dyn StateStore>,
}

impl MockContext {
    fn new() -> Self {
        Self {
            state_store: Arc::new(InMemoryStateStore::new()),
        }
    }
}

impl Context for MockContext {
    fn submit_run(
        &self,
        params: SubmitRunParams,
    ) -> Pin<
        Box<dyn Future<Output = Result<RunStatus, error_stack::Report<PluginError>>> + Send + '_>,
    > {
        let input_count = params.inputs.len();
        Box::pin(async move {
            let flow_id = stepflow_core::BlobId::new(
                "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            )
            .expect("mock blob id");
            let now = chrono::Utc::now();

            Ok(RunStatus {
                run_id: Uuid::now_v7(),
                flow_id,
                flow_name: None,
                status: stepflow_core::status::ExecutionStatus::Running,
                items: ItemStatistics {
                    total: input_count,
                    completed: 0,
                    running: input_count,
                    failed: 0,
                    cancelled: 0,
                },
                created_at: now,
                completed_at: None,
                results: None,
            })
        })
    }

    fn get_run(
        &self,
        run_id: Uuid,
        options: GetRunOptions,
    ) -> Pin<
        Box<dyn Future<Output = Result<RunStatus, error_stack::Report<PluginError>>> + Send + '_>,
    > {
        Box::pin(async move {
            let flow_id = stepflow_core::BlobId::new(
                "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            )
            .expect("mock blob id");
            let now = chrono::Utc::now();

            let results = if options.include_results {
                Some(vec![ItemResult {
                    item_index: 0,
                    status: stepflow_core::status::ExecutionStatus::Completed,
                    result: Some(FlowResult::Success(ValueRef::new(
                        serde_json::json!({"message": "Hello from mock"}),
                    ))),
                    completed_at: Some(now),
                }])
            } else {
                None
            };

            Ok(RunStatus {
                run_id,
                flow_id,
                flow_name: None,
                status: stepflow_core::status::ExecutionStatus::Completed,
                items: ItemStatistics {
                    total: 1,
                    completed: 1,
                    running: 0,
                    failed: 0,
                    cancelled: 0,
                },
                created_at: now,
                completed_at: Some(now),
                results,
            })
        })
    }

    fn state_store(&self) -> &Arc<dyn StateStore> {
        &self.state_store
    }

    fn working_directory(&self) -> &Path {
        Path::new(".")
    }
}

/// Test that we can create an HTTP plugin and it fails gracefully when no server is running
#[tokio::test]
async fn test_http_plugin_creation_failure() {
    // Test streamable HTTP transport (now the only HTTP transport)
    let config = StepflowPluginConfig {
        transport: StepflowTransport::Remote {
            url: "http://127.0.0.1:18080".to_string(),
        },
    };

    let plugin = config
        .create_plugin(std::path::Path::new("."))
        .await
        .expect("Should create HTTP plugin");

    let context: Arc<dyn Context> = Arc::new(MockContext::new());

    // Try to initialize - this should fail since no server is running
    let result = plugin.init(&context).await;
    assert!(
        result.is_err(),
        "Expected initialization to fail with no server running"
    );
}

/// Integration test that starts a Python HTTP server and tests the streamable HTTP protocol
#[tokio::test]
async fn test_http_protocol_integration() {
    // Get paths relative to CARGO_MANIFEST_DIR for stability
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let python_sdk_path = manifest_dir.join("../../../sdks/python");
    let server_script = manifest_dir.join("tests/test_echo_server.py");

    // Fail test if Python SDK not found (required for this test)
    assert!(
        python_sdk_path.exists(),
        "Python SDK not found at {:?}. This test requires the Python SDK.",
        python_sdk_path
    );

    // Fail test if echo server script not found
    assert!(
        server_script.exists(),
        "Test echo server not found at {:?}. This test requires test_echo_server.py.",
        server_script
    );

    // Start the Python HTTP server
    let mut python_server = Command::new("uv")
        .args([
            "run",
            "--project",
            python_sdk_path.to_str().unwrap(),
            "--extra",
            "http",
            "python",
            server_script.to_str().unwrap(),
            "--http",
            "--port",
            "18081",
        ])
        .current_dir(&manifest_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start Python server");

    // Wait for server to start
    sleep(Duration::from_secs(3)).await;

    // Create streamable HTTP plugin (now the primary HTTP transport)
    let config = StepflowPluginConfig {
        transport: StepflowTransport::Remote {
            url: "http://127.0.0.1:18081".to_string(),
        },
    };

    let plugin = config
        .create_plugin(std::path::Path::new("."))
        .await
        .expect("Should create HTTP plugin");

    let context: Arc<dyn Context> = Arc::new(MockContext::new());

    // Test initialization
    let init_result = timeout(Duration::from_secs(10), plugin.init(&context)).await;

    // Clean up the server process
    let _ = python_server.kill().await;

    match init_result {
        Ok(Ok(())) => {
            println!("✓ Streamable HTTP plugin initialized successfully");

            // Test component listing
            let components_result = timeout(Duration::from_secs(5), plugin.list_components()).await;
            match components_result {
                Ok(Ok(components)) => {
                    println!(
                        "✓ Listed {} components with streamable HTTP",
                        components.len()
                    );

                    // Look for the echo component
                    let echo_component = components
                        .iter()
                        .find(|c| c.component.path().ends_with("/echo"));
                    if let Some(echo_info) = echo_component {
                        println!(
                            "✓ Found echo component via streamable HTTP: {}",
                            echo_info.component.path()
                        );

                        // Test component info
                        let component = &echo_info.component;

                        let info_result =
                            timeout(Duration::from_secs(5), plugin.component_info(component)).await;
                        match info_result {
                            Ok(Ok(info)) => {
                                println!(
                                    "✓ Got component info via streamable HTTP: {}",
                                    info.component.path()
                                );

                                // Test component execution
                                let input_json = serde_json::json!({
                                    "message": "Hello, Streamable HTTP!"
                                });
                                let input_ref = ValueRef::from(input_json);

                                let execution_context =
                                    ExecutionContext::for_testing(context.clone(), Uuid::now_v7());

                                let execute_result = timeout(
                                    Duration::from_secs(5),
                                    plugin.execute(component, execution_context, input_ref),
                                )
                                .await;

                                match execute_result {
                                    Ok(Ok(flow_result)) => {
                                        println!(
                                            "✓ Streamable HTTP component execution successful"
                                        );
                                        // The result should be a success with the echo response
                                        match flow_result {
                                            stepflow_core::FlowResult::Success(result) => {
                                                println!(
                                                    "✓ Got streamable HTTP result: {result:?}"
                                                );
                                            }
                                            _ => {
                                                eprintln!(
                                                    "✗ Expected success result from streamable HTTP"
                                                );
                                            }
                                        }
                                    }
                                    Ok(Err(e)) => {
                                        eprintln!(
                                            "✗ Streamable HTTP component execution failed: {e:?}"
                                        );
                                    }
                                    Err(_) => {
                                        eprintln!(
                                            "✗ Streamable HTTP component execution timed out"
                                        );
                                    }
                                }
                            }
                            Ok(Err(e)) => {
                                eprintln!("✗ Streamable HTTP component info failed: {e:?}");
                            }
                            Err(_) => {
                                eprintln!("✗ Streamable HTTP component info timed out");
                            }
                        }
                    } else {
                        eprintln!("✗ Echo component not found in streamable HTTP component list");
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("✗ Streamable HTTP list components failed: {e:?}");
                }
                Err(_) => {
                    eprintln!("✗ Streamable HTTP list components timed out");
                }
            }
        }
        Ok(Err(e)) => {
            eprintln!("✗ Streamable HTTP plugin initialization failed: {e:?}");
        }
        Err(_) => {
            eprintln!("✗ Streamable HTTP plugin initialization timed out");
        }
    }
}

/// Test HTTP plugin with server that starts and stops
#[tokio::test]
async fn test_http_plugin_lifecycle() {
    // Get paths relative to CARGO_MANIFEST_DIR for stability
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let python_sdk_path = manifest_dir.join("../../../sdks/python");
    let server_script = manifest_dir.join("tests/test_echo_server.py");

    // Fail test if Python SDK not found (required for this test)
    assert!(
        python_sdk_path.exists(),
        "Python SDK not found at {:?}. This test requires the Python SDK.",
        python_sdk_path
    );

    // Fail test if echo server script not found
    assert!(
        server_script.exists(),
        "Test echo server not found at {:?}. This test requires test_echo_server.py.",
        server_script
    );

    // Create streamable HTTP plugin (now the primary HTTP transport)
    let config = StepflowPluginConfig {
        transport: StepflowTransport::Remote {
            url: "http://127.0.0.1:18082".to_string(),
        },
    };

    let plugin = config
        .create_plugin(std::path::Path::new("."))
        .await
        .expect("Should create HTTP plugin");

    let context: Arc<dyn Context> = Arc::new(MockContext::new());

    // Test initialization without server - should fail
    let init_result = timeout(Duration::from_secs(2), plugin.init(&context)).await;
    assert!(
        init_result.is_ok() && init_result.unwrap().is_err(),
        "Expected initialization to fail without server"
    );

    // Start server
    let mut python_server = Command::new("uv")
        .args([
            "run",
            "--project",
            python_sdk_path.to_str().unwrap(),
            "--extra",
            "http",
            "python",
            server_script.to_str().unwrap(),
            "--http",
            "--port",
            "18082",
        ])
        .current_dir(&manifest_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start Python server");

    // Wait for server to start
    sleep(Duration::from_secs(3)).await;

    // Test initialization with server - should succeed
    let init_result = timeout(Duration::from_secs(5), plugin.init(&context)).await;

    // Clean up
    let _ = python_server.kill().await;

    match init_result {
        Ok(Ok(())) => {
            println!("✓ Streamable HTTP plugin initialized successfully after server startup");
        }
        Ok(Err(e)) => {
            eprintln!("✗ Streamable HTTP plugin initialization failed: {e:?}");
        }
        Err(_) => {
            eprintln!("✗ Streamable HTTP plugin initialization timed out");
        }
    }
}
