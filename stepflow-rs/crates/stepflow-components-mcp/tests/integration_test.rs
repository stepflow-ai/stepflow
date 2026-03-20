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

use indexmap::IndexMap;
use serde_json::json;
use std::sync::Arc;
use stepflow_components_mcp::McpPluginConfig;
use stepflow_core::{
    BlobId, FlowResult,
    workflow::{Component, Flow, ValueRef},
};
use stepflow_plugin::{
    Plugin as _, PluginConfig as _, RunContext, StepflowEnvironment, TaskRegistryExt as _,
    build_in_memory_environment,
};
use uuid::Uuid;

// Helper function to create a test context
async fn create_test_context() -> (Arc<StepflowEnvironment>, Arc<RunContext>) {
    let env = build_in_memory_environment().await.unwrap();
    let run_id = Uuid::now_v7();
    let test_flow = Arc::new(Flow::default());
    let flow_id = BlobId::from_flow(&test_flow).expect("Flow should serialize");
    let run_context = Arc::new(RunContext::new(run_id, test_flow, flow_id, env.clone()));
    (env, run_context)
}

#[tokio::test]
async fn test_mcp_plugin_initialization() {
    let config = McpPluginConfig {
        command: "python3".to_string(),
        args: vec!["tests/mock_mcp_server.py".to_string()],
        env: IndexMap::new(),
    };

    let working_dir = std::env::current_dir().unwrap();
    let plugin = config.create_plugin(&working_dir).await.unwrap();

    let (_, run_context) = create_test_context().await;
    plugin.ensure_initialized(run_context.env()).await.unwrap();

    // List components should return our mock tools
    let components = plugin.list_components().await.unwrap();
    assert_eq!(components.len(), 2);

    // Check that the component paths are correct
    let paths: Vec<String> = components
        .iter()
        .map(|c| c.component.path().to_string())
        .collect();
    assert!(paths.contains(&"/echo".to_string()));
    assert!(paths.contains(&"/add".to_string()));
}

#[tokio::test]
async fn test_mcp_tool_execution() {
    let config = McpPluginConfig {
        command: "python3".to_string(),
        args: vec!["tests/mock_mcp_server.py".to_string()],
        env: IndexMap::new(),
    };

    let working_dir = std::env::current_dir().unwrap();
    let plugin = config.create_plugin(&working_dir).await.unwrap();

    let (_, run_context) = create_test_context().await;
    plugin.ensure_initialized(run_context.env()).await.unwrap();

    // Test echo tool
    let echo_component = Component::from_string("/mock-server/echo");
    let echo_input = ValueRef::new(json!({
        "message": "Hello, MCP!"
    }));

    let task_id = Uuid::now_v7().to_string();
    let registry = run_context.env().task_registry();
    let rx = registry.register(task_id.clone());
    let empty_params = std::collections::HashMap::new();
    plugin
        .start_task(
            &task_id,
            &echo_component,
            &run_context,
            None,
            echo_input,
            1,
            &empty_params,
        )
        .await
        .unwrap();
    let result = rx.await.unwrap();

    match result {
        FlowResult::Success(result) => {
            let content = result.as_ref();
            assert!(content.is_array());
            let content_array = content.as_array().unwrap();
            let first_item = &content_array[0];
            assert_eq!(first_item.get("type").unwrap(), "text");
            assert_eq!(first_item.get("text").unwrap(), "Echo: Hello, MCP!");
        }
        _ => panic!("Expected success result"),
    }

    // Test add tool
    let add_component = Component::from_string("/mock-server/add");
    let add_input = ValueRef::new(json!({
        "a": 5,
        "b": 3
    }));

    let task_id = Uuid::now_v7().to_string();
    let registry = run_context.env().task_registry();
    let rx = registry.register(task_id.clone());
    let empty_params = std::collections::HashMap::new();
    plugin
        .start_task(
            &task_id,
            &add_component,
            &run_context,
            None,
            add_input,
            1,
            &empty_params,
        )
        .await
        .unwrap();
    let result = rx.await.unwrap();

    match result {
        FlowResult::Success(result) => {
            let content = result.as_ref();
            assert!(content.is_array());
            let content_array = content.as_array().unwrap();
            let first_item = &content_array[0];
            assert_eq!(first_item.get("type").unwrap(), "text");
            assert_eq!(first_item.get("text").unwrap(), "Result: 8");
        }
        _ => panic!("Expected success result"),
    }
}

#[tokio::test]
async fn test_mcp_error_handling() {
    let config = McpPluginConfig {
        command: "python3".to_string(),
        args: vec!["tests/mock_mcp_server.py".to_string()],
        env: IndexMap::new(),
    };

    let working_dir = std::env::current_dir().unwrap();
    let plugin = config.create_plugin(&working_dir).await.unwrap();

    let (_, run_context) = create_test_context().await;
    plugin.ensure_initialized(run_context.env()).await.unwrap();

    // Test calling a non-existent tool
    let bad_component = Component::from_string("/mock-server/nonexistent");
    let input = ValueRef::new(json!({}));

    let task_id = Uuid::now_v7().to_string();
    let registry = run_context.env().task_registry();
    let rx = registry.register(task_id.clone());
    let empty_params = std::collections::HashMap::new();
    let start_result = plugin
        .start_task(
            &task_id,
            &bad_component,
            &run_context,
            None,
            input,
            1,
            &empty_params,
        )
        .await;
    assert!(
        start_result.is_ok(),
        "Expected Ok, got Err: {start_result:?}"
    );
    let result = rx.await.unwrap();

    match result {
        FlowResult::Failed(error) => {
            let error_message = format!("{error}");
            assert!(error_message.contains("nonexistent"));
        }
        other => panic!("Expected FlowResult::Failed, got: {other:?}"),
    }
}
