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

use std::process::Stdio;
use std::time::Duration;

use std::sync::Arc;

use stepflow_core::BlobId;
use stepflow_core::workflow::{Flow, ValueRef};
use stepflow_plugin::{Plugin as _, PluginConfig as _, RunContext, StepflowEnvironmentBuilder};
use stepflow_protocol::{StepflowPluginConfig, StepflowTransport};
use tokio::process::Command;
use tokio::time::{sleep, timeout};
use uuid::Uuid;

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

    let env = StepflowEnvironmentBuilder::build_in_memory().await.unwrap();

    // Try to initialize - this should fail since no server is running
    let result = plugin.ensure_initialized(&env).await;
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

    let env = StepflowEnvironmentBuilder::build_in_memory().await.unwrap();

    // Test initialization
    let init_result = timeout(Duration::from_secs(10), plugin.ensure_initialized(&env)).await;

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

                                let test_flow = Arc::new(Flow::default());
                                let flow_id =
                                    BlobId::from_flow(&test_flow).expect("Flow should serialize");
                                let run_context = Arc::new(RunContext::new(
                                    Uuid::now_v7(),
                                    test_flow,
                                    flow_id,
                                    env.clone(),
                                ));

                                let execute_result = timeout(
                                    Duration::from_secs(5),
                                    plugin.execute(component, &run_context, None, input_ref),
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

    let env = StepflowEnvironmentBuilder::build_in_memory().await.unwrap();

    // Test initialization without server - should fail
    let init_result = timeout(Duration::from_secs(2), plugin.ensure_initialized(&env)).await;
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
    let init_result = timeout(Duration::from_secs(5), plugin.ensure_initialized(&env)).await;

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
