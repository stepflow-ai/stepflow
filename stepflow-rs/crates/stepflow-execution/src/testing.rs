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

//! Test utilities for the execution module.
//!
//! This module provides shared test helpers for creating mock executors,
//! test flows, and common result values. These utilities are used by tests
//! in `flow_executor` and other execution-related modules.

use std::sync::Arc;

use serde_json::json;
use stepflow_core::values::ValueRef;
use stepflow_core::workflow::{Flow, FlowBuilder, StepBuilder};
use stepflow_core::{FlowResult, StepflowEnvironment, ValueExpr};
use stepflow_mock::{MockComponentBehavior, MockPlugin};
use stepflow_plugin::StepflowEnvironmentBuilder;
use stepflow_state::{BlobStore, InMemoryStateStore, MetadataStore, MetadataStoreExt as _};

/// Builder for creating a mock [`StepflowEnvironment`] for testing.
///
/// This provides a convenient way to set up a test executor with mock plugins
/// that return predictable results.
///
/// # Example
///
/// ```ignore
/// let executor = MockExecutorBuilder::new()
///     .with_success_result(json!({"result": "ok"}))
///     .build()
///     .await;
/// ```
pub struct MockExecutorBuilder {
    /// The result to return for successful component executions.
    success_result: serde_json::Value,
    /// Additional inputs to register with the mock plugin.
    additional_inputs: Vec<serde_json::Value>,
}

impl Default for MockExecutorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl MockExecutorBuilder {
    /// Create a new mock executor builder with default settings.
    pub fn new() -> Self {
        Self {
            success_result: json!({"result": "ok"}),
            additional_inputs: Vec::new(),
        }
    }

    /// Set the result value returned by successful component executions.
    pub fn with_success_result(mut self, result: serde_json::Value) -> Self {
        self.success_result = result;
        self
    }

    /// Add additional input values that the mock plugin will accept.
    pub fn with_input(mut self, input: serde_json::Value) -> Self {
        self.additional_inputs.push(input);
        self
    }

    /// Build the mock environment.
    pub async fn build(self) -> Arc<StepflowEnvironment> {
        // Create mock plugin that returns success for any component
        let mut mock_plugin = MockPlugin::new();
        let behavior = MockComponentBehavior::result(FlowResult::Success(ValueRef::new(
            self.success_result.clone(),
        )));

        // Register common inputs that tests typically use
        let mut inputs = vec![
            json!({}),
            json!({"x": 1}),
            json!({"x": 2}),
            json!({"x": 3}),
            json!({"value": 1}),
            json!({"value": 2}),
            self.success_result.clone(),
        ];
        inputs.extend(self.additional_inputs);

        for input in &inputs {
            mock_plugin
                .mock_component("/mock/test")
                .behavior(ValueRef::new(input.clone()), behavior.clone());
        }

        let dyn_plugin = stepflow_plugin::DynPlugin::boxed(mock_plugin);

        // Create plugin router
        use stepflow_plugin::routing::RouteRule;
        let rules = vec![RouteRule {
            conditions: vec![],
            component_allow: None,
            component_deny: None,
            plugin: "mock".into(),
            component: None,
        }];

        let plugin_router = stepflow_plugin::routing::PluginRouter::builder()
            .with_routing_path("/{*component}".to_string(), rules)
            .register_plugin("mock".to_string(), dyn_plugin)
            .build()
            .unwrap();

        let store = Arc::new(InMemoryStateStore::new());
        let metadata_store: Arc<dyn MetadataStore> = store.clone();
        let blob_store: Arc<dyn BlobStore> = store;
        StepflowEnvironmentBuilder::new()
            .metadata_store(metadata_store)
            .blob_store(blob_store)
            .working_directory(std::path::PathBuf::from("."))
            .plugin_router(plugin_router)
            .build()
            .await
            .expect("MockPlugin should always initialize successfully")
    }
}

/// Create a simple flow with the specified number of independent steps.
///
/// Each step uses the `/mock/test` component and takes `$input` as its input.
/// The flow output references the last step.
///
/// # Example
///
/// ```ignore
/// let flow = create_linear_flow(3);
/// // Creates: step0 -> step1 -> step2 (all independent, output = step2)
/// ```
pub fn create_linear_flow(step_count: usize) -> Flow {
    let steps = (0..step_count)
        .map(|i| {
            StepBuilder::new(format!("step{}", i))
                .component("/mock/test")
                .input(ValueExpr::Input {
                    input: Default::default(),
                })
                .build()
        })
        .collect::<Vec<_>>();

    let last_step_id = format!("step{}", step_count.saturating_sub(1));
    FlowBuilder::test_flow()
        .steps(steps)
        .output(ValueExpr::Step {
            step: last_step_id,
            path: Default::default(),
        })
        .build()
}

/// Create a flow with steps that form a dependency chain.
///
/// Each step depends on the previous step's output (except the first which takes `$input`).
/// The flow output references the last step.
///
/// # Example
///
/// ```ignore
/// let flow = create_chain_flow(3);
/// // Creates: step0($input) -> step1($step.step0) -> step2($step.step1)
/// ```
pub fn create_chain_flow(step_count: usize) -> Flow {
    let steps = (0..step_count)
        .map(|i| {
            let input = if i == 0 {
                ValueExpr::Input {
                    input: Default::default(),
                }
            } else {
                ValueExpr::Step {
                    step: format!("step{}", i - 1),
                    path: Default::default(),
                }
            };
            StepBuilder::new(format!("step{}", i))
                .component("/mock/test")
                .input(input)
                .build()
        })
        .collect::<Vec<_>>();

    let last_step_id = format!("step{}", step_count.saturating_sub(1));
    FlowBuilder::test_flow()
        .steps(steps)
        .output(ValueExpr::Step {
            step: last_step_id,
            path: Default::default(),
        })
        .build()
}

/// Create a flow with named steps (for more readable tests).
///
/// Each step uses the `/mock/test` component and takes `$input` as its input.
/// The flow output references the last step.
///
/// # Example
///
/// ```ignore
/// let flow = create_flow_with_names(&["fetch", "transform", "store"]);
/// ```
pub fn create_flow_with_names(step_names: &[&str]) -> Flow {
    let steps = step_names
        .iter()
        .map(|name| {
            StepBuilder::new(*name)
                .component("/mock/test")
                .input(ValueExpr::Input {
                    input: Default::default(),
                })
                .build()
        })
        .collect::<Vec<_>>();

    let last_step_id = step_names.last().copied().unwrap_or("step0").to_string();
    FlowBuilder::test_flow()
        .steps(steps)
        .output(ValueExpr::Step {
            step: last_step_id,
            path: Default::default(),
        })
        .build()
}

/// Create a diamond-shaped DAG flow: A → B, A → C, B+C → D.
///
/// Step A takes input, B and C depend on A, D depends on both B and C.
/// The flow output references step D.
///
/// This tests parallel execution of independent steps (B and C can run in parallel).
pub fn create_diamond_flow() -> Flow {
    FlowBuilder::test_flow()
        .steps(vec![
            // Step A: takes input
            StepBuilder::new("A")
                .component("/mock/test")
                .input(ValueExpr::Input {
                    input: Default::default(),
                })
                .build(),
            // Step B: depends on A
            StepBuilder::new("B")
                .component("/mock/test")
                .input(ValueExpr::Step {
                    step: "A".to_string(),
                    path: Default::default(),
                })
                .build(),
            // Step C: depends on A (can run in parallel with B)
            StepBuilder::new("C")
                .component("/mock/test")
                .input(ValueExpr::Step {
                    step: "A".to_string(),
                    path: Default::default(),
                })
                .build(),
            // Step D: depends on both B and C (uses object literal)
            StepBuilder::new("D")
                .component("/mock/test")
                .input(ValueExpr::Object(
                    vec![
                        (
                            "b".to_string(),
                            ValueExpr::Step {
                                step: "B".to_string(),
                                path: Default::default(),
                            },
                        ),
                        (
                            "c".to_string(),
                            ValueExpr::Step {
                                step: "C".to_string(),
                                path: Default::default(),
                            },
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ))
                .build(),
        ])
        .output(ValueExpr::Step {
            step: "D".to_string(),
            path: Default::default(),
        })
        .build()
}

/// Create a test executor with custom input-to-behavior mapping.
///
/// Each tuple maps an input JSON value to the FlowResult that should be returned.
/// This is useful for testing flows where different steps receive different inputs.
///
/// # Example
///
/// ```ignore
/// let executor = create_executor_with_behaviors(vec![
///     (json!({"x": 1}), FlowResult::Success(ValueRef::new(json!({"y": 2})))),
///     (json!({"y": 2}), FlowResult::Failed(FlowError::new(500, "step2 failed"))),
/// ]).await;
/// ```
pub async fn create_executor_with_behaviors(
    behaviors: Vec<(serde_json::Value, FlowResult)>,
) -> Arc<StepflowEnvironment> {
    let mut mock_plugin = MockPlugin::new();

    for (input, result) in &behaviors {
        mock_plugin.mock_component("/mock/test").behavior(
            ValueRef::new(input.clone()),
            MockComponentBehavior::result(result.clone()),
        );
    }

    let dyn_plugin = stepflow_plugin::DynPlugin::boxed(mock_plugin);

    use stepflow_plugin::routing::RouteRule;
    let rules = vec![RouteRule {
        conditions: vec![],
        component_allow: None,
        component_deny: None,
        plugin: "mock".into(),
        component: None,
    }];

    let plugin_router = stepflow_plugin::routing::PluginRouter::builder()
        .with_routing_path("/{*component}".to_string(), rules)
        .register_plugin("mock".to_string(), dyn_plugin)
        .build()
        .unwrap();

    let store = Arc::new(InMemoryStateStore::new());
    let metadata_store: Arc<dyn MetadataStore> = store.clone();
    let blob_store: Arc<dyn BlobStore> = store;
    StepflowEnvironmentBuilder::new()
        .metadata_store(metadata_store)
        .blob_store(blob_store)
        .working_directory(std::path::PathBuf::from("."))
        .plugin_router(plugin_router)
        .build()
        .await
        .expect("MockPlugin should always initialize successfully")
}

/// Create a test environment with a wait signal for a specific input.
///
/// Returns both the executor and the signal sender. Dropping or sending to the
/// signal sender will unblock the component execution for that input.
///
/// # Example
///
/// ```ignore
/// let (executor, signal) = create_executor_with_wait_signal(json!({"x": 1})).await;
///
/// // Start execution (will block on the wait signal)
/// let handle = tokio::spawn(async move {
///     items_executor.execute_to_completion().await
/// });
///
/// // Do something while execution is blocked...
///
/// // Signal completion
/// drop(signal);
/// handle.await.unwrap();
/// ```
pub async fn create_env_with_wait_signal(
    wait_input: serde_json::Value,
) -> (Arc<StepflowEnvironment>, stepflow_mock::SignalSender) {
    let mut mock_plugin = MockPlugin::new();

    // Register the wait signal for the specific input
    let signal = mock_plugin.wait_for("/mock/test", ValueRef::new(wait_input.clone()));

    // Register behavior for the input (will be returned after signal)
    mock_plugin.mock_component("/mock/test").behavior(
        ValueRef::new(wait_input),
        MockComponentBehavior::result(FlowResult::Success(ValueRef::new(json!({"result": "ok"})))),
    );

    // Also register common inputs that tests typically use
    let common_inputs = vec![json!({}), json!({"x": 1}), json!({"x": 2}), json!({"x": 3})];
    for input in common_inputs {
        mock_plugin.mock_component("/mock/test").behavior(
            ValueRef::new(input),
            MockComponentBehavior::result(FlowResult::Success(ValueRef::new(
                json!({"result": "ok"}),
            ))),
        );
    }

    let dyn_plugin = stepflow_plugin::DynPlugin::boxed(mock_plugin);

    use stepflow_plugin::routing::RouteRule;
    let rules = vec![RouteRule {
        conditions: vec![],
        component_allow: None,
        component_deny: None,
        plugin: "mock".into(),
        component: None,
    }];

    let plugin_router = stepflow_plugin::routing::PluginRouter::builder()
        .with_routing_path("/{*component}".to_string(), rules)
        .register_plugin("mock".to_string(), dyn_plugin)
        .build()
        .unwrap();

    let store = Arc::new(InMemoryStateStore::new());
    let metadata_store: Arc<dyn MetadataStore> = store.clone();
    let blob_store: Arc<dyn BlobStore> = store;
    let env = StepflowEnvironmentBuilder::new()
        .metadata_store(metadata_store)
        .blob_store(blob_store)
        .working_directory(std::path::PathBuf::from("."))
        .plugin_router(plugin_router)
        .build()
        .await
        .expect("MockPlugin should always initialize successfully");

    (env, signal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_linear_flow() {
        let flow = create_linear_flow(3);
        assert_eq!(flow.steps().len(), 3);
        assert_eq!(flow.steps()[0].id, "step0");
        assert_eq!(flow.steps()[1].id, "step1");
        assert_eq!(flow.steps()[2].id, "step2");
    }

    #[test]
    fn test_create_chain_flow() {
        let flow = create_chain_flow(3);
        assert_eq!(flow.steps().len(), 3);

        // First step should reference $input
        match &flow.steps()[0].input {
            ValueExpr::Input { .. } => {}
            _ => panic!("Expected Input expression for step0"),
        }

        // Second step should reference step0
        match &flow.steps()[1].input {
            ValueExpr::Step { step, .. } => assert_eq!(step, "step0"),
            _ => panic!("Expected Step expression for step1"),
        }

        // Third step should reference step1
        match &flow.steps()[2].input {
            ValueExpr::Step { step, .. } => assert_eq!(step, "step1"),
            _ => panic!("Expected Step expression for step2"),
        }
    }

    #[test]
    fn test_create_flow_with_names() {
        let flow = create_flow_with_names(&["fetch", "transform", "store"]);
        assert_eq!(flow.steps().len(), 3);
        assert_eq!(flow.steps()[0].id, "fetch");
        assert_eq!(flow.steps()[1].id, "transform");
        assert_eq!(flow.steps()[2].id, "store");
    }

    #[tokio::test]
    async fn test_mock_executor_builder() {
        let executor = MockExecutorBuilder::new()
            .with_success_result(json!({"custom": "result"}))
            .build()
            .await;

        // Verify the executor was created by checking metadata_store is accessible
        // (We can't easily test get_blob without a valid blob ID, so just verify construction)
        let _ = executor.metadata_store();
    }

    #[test]
    fn test_create_diamond_flow() {
        let flow = create_diamond_flow();
        assert_eq!(flow.steps().len(), 4);

        // Verify step names
        assert_eq!(flow.steps()[0].id, "A");
        assert_eq!(flow.steps()[1].id, "B");
        assert_eq!(flow.steps()[2].id, "C");
        assert_eq!(flow.steps()[3].id, "D");

        // Step A should reference input
        match &flow.steps()[0].input {
            ValueExpr::Input { .. } => {}
            _ => panic!("Expected Input expression for A"),
        }

        // Steps B and C should reference A
        match &flow.steps()[1].input {
            ValueExpr::Step { step, .. } => assert_eq!(step, "A"),
            _ => panic!("Expected Step expression for B"),
        }
        match &flow.steps()[2].input {
            ValueExpr::Step { step, .. } => assert_eq!(step, "A"),
            _ => panic!("Expected Step expression for C"),
        }

        // Step D should be an object referencing B and C
        match &flow.steps()[3].input {
            ValueExpr::Object(obj) => {
                let keys: Vec<_> = obj.iter().map(|(k, _)| k.as_str()).collect();
                assert!(keys.contains(&"b"));
                assert!(keys.contains(&"c"));
            }
            _ => panic!("Expected Object expression for D"),
        }
    }
}
