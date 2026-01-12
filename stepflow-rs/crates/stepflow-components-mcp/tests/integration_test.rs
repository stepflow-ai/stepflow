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
    FlowResult,
    workflow::{Component, ValueRef},
};
use stepflow_plugin::{
    ExecutionContext, Plugin as _, PluginConfig as _, RunContext, StepflowEnvironment,
};
use uuid::Uuid;

// Helper function to create a test context
async fn create_test_context() -> (Arc<StepflowEnvironment>, ExecutionContext) {
    let env = StepflowEnvironment::new_in_memory().await.unwrap();
    let run_id = Uuid::now_v7();
    let run_context = Arc::new(RunContext::for_root(run_id));
    let exec_context = ExecutionContext::for_testing(env.clone(), run_context);
    (env, exec_context)
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

    let (env, _exec_context) = create_test_context().await;
    plugin.ensure_initialized(&env).await.unwrap();

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

    let (env, exec_context) = create_test_context().await;
    plugin.ensure_initialized(&env).await.unwrap();

    // Test echo tool
    let echo_component = Component::from_string("/mock-server/echo");
    let echo_input = ValueRef::new(json!({
        "message": "Hello, MCP!"
    }));

    let result = plugin
        .execute(&echo_component, exec_context.clone(), echo_input)
        .await
        .unwrap();

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

    let result = plugin
        .execute(&add_component, exec_context, add_input)
        .await
        .unwrap();

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

    let (env, exec_context) = create_test_context().await;
    plugin.ensure_initialized(&env).await.unwrap();

    // Test calling a non-existent tool
    let bad_component = Component::from_string("/mock-server/nonexistent");
    let input = ValueRef::new(json!({}));

    let result = plugin.execute(&bad_component, exec_context, input).await;
    assert!(
        result.is_ok(),
        "Expected Ok(FlowResult::Failed), got Err: {result:?}"
    );

    match result.unwrap() {
        FlowResult::Failed(error) => {
            let error_message = format!("{error}");
            assert!(error_message.contains("nonexistent"));
        }
        other => panic!("Expected FlowResult::Failed, got: {other:?}"),
    }
}
