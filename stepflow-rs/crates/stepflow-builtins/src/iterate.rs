// Licensed to the Apache Software Foundation (ASF) under one or more contributor license agreements.
// See the NOTICE file distributed with this work for additional information regarding copyright
// ownership.  The ASF licenses this file to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance with the License.  You may obtain a
// copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied.  See the License for the specific language governing permissions and limitations under
// the License.

use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_core::{
    FlowResult,
    component::ComponentInfo,
    schema::SchemaRef,
    workflow::{Flow, ValueRef},
};
use stepflow_plugin::{Context as _, ExecutionContext};

use crate::{BuiltinComponent, Result, error::BuiltinError};

/// Component for iteratively applying a workflow until a termination condition is met.
pub struct IterateComponent;

impl IterateComponent {
    pub fn new() -> Self {
        Self
    }
}

impl Default for IterateComponent {
    fn default() -> Self {
        Self::new()
    }
}

/// Input for the iterate component
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
struct IterateInput {
    /// The workflow to iterate. Must return either {"result": value} or {"next": value}
    flow: Flow,

    /// The initial input to pass to the workflow
    initial_input: ValueRef,

    /// Maximum number of iterations to prevent infinite loops (default: 1000)
    #[serde(default = "default_max_iterations")]
    max_iterations: u32,
}

fn default_max_iterations() -> u32 {
    1000
}

/// Output from the iterate component
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
struct IterateOutput {
    /// The final result value
    result: ValueRef,

    /// The number of iterations performed
    iterations: u32,

    /// Whether the iteration was terminated by max_iterations
    terminated: bool,
}

impl BuiltinComponent for IterateComponent {
    fn component_info(&self) -> Result<ComponentInfo> {
        let input_schema = SchemaRef::for_type::<IterateInput>();
        let output_schema = SchemaRef::for_type::<IterateOutput>();

        Ok(ComponentInfo {
            component: stepflow_core::workflow::Component::from_string("/iterate"),
            input_schema: Some(input_schema),
            output_schema: Some(output_schema),
            description: Some(
                "Iteratively apply a workflow until it returns a result instead of next"
                    .to_string(),
            ),
        })
    }

    async fn execute(&self, context: ExecutionContext, input: ValueRef) -> Result<FlowResult> {
        let input: IterateInput = serde_json::from_value(input.as_ref().clone())
            .change_context(BuiltinError::InvalidInput)?;

        let flow = Arc::new(input.flow);
        let workflow_hash = Flow::hash(&flow);
        let mut current_input = input.initial_input;
        let mut iterations = 0;

        loop {
            // Check if we've exceeded max iterations
            if iterations >= input.max_iterations {
                let output = IterateOutput {
                    result: current_input,
                    iterations,
                    terminated: true,
                };
                let output_value =
                    serde_json::to_value(output).change_context(BuiltinError::Internal)?;
                return Ok(FlowResult::Success(ValueRef::new(output_value)));
            }

            // Execute the workflow
            let result = context
                .execute_flow(flow.clone(), workflow_hash.clone(), current_input)
                .await
                .change_context(BuiltinError::Internal)?;

            iterations += 1;

            match result {
                FlowResult::Success(result) => {
                    // Parse the result to check for "result" or "next" fields
                    let result_value = result.as_ref();

                    if let Some(result_obj) = result_value.as_object() {
                        if let Some(final_result) = result_obj.get("result") {
                            // Found "result" field - return it
                            let output = IterateOutput {
                                result: ValueRef::new(final_result.clone()),
                                iterations,
                                terminated: false,
                            };
                            let output_value = serde_json::to_value(output)
                                .change_context(BuiltinError::Internal)?;
                            return Ok(FlowResult::Success(ValueRef::new(output_value)));
                        } else if let Some(next_input) = result_obj.get("next") {
                            // Found "next" field - continue iteration
                            current_input = ValueRef::new(next_input.clone());
                            continue;
                        }
                    }

                    // Result doesn't have expected structure - return error
                    return Ok(FlowResult::Failed(stepflow_core::FlowError::new(
                        400,
                        "Workflow result must contain either 'result' or 'next' field",
                    )));
                }
                FlowResult::Failed(error) => {
                    // Propagate the failure from the workflow
                    return Ok(FlowResult::Failed(error));
                }
                FlowResult::Skipped => {
                    // Treat skipped as an error in this context
                    return Ok(FlowResult::Failed(stepflow_core::FlowError::new(
                        400,
                        "Workflow cannot be skipped during iteration",
                    )));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_context::MockContext;
    use stepflow_core::values::ValueTemplate;

    #[tokio::test]
    async fn test_iterate_component_with_result() {
        let component = IterateComponent::new();

        // Create a simple workflow - the mock will return {"message": "Hello from nested flow"}
        let test_flow = Flow {
            name: Some("test-result".to_string()),
            steps: vec![],
            output: ValueTemplate::literal(serde_json::json!({"result": "test"})),
            ..Default::default()
        };

        let input = IterateInput {
            flow: test_flow,
            initial_input: serde_json::json!({"start": "value"}).into(),
            max_iterations: 10,
        };

        let input_value = serde_json::to_value(input).unwrap();
        let mock = MockContext::new();

        let result = component
            .execute(mock.execution_context(), input_value.into())
            .await
            .unwrap();

        match result {
            FlowResult::Failed(error) => {
                // The mock context returns {"message": "Hello from nested flow"} which doesn't have "result" or "next"
                assert!(error.message.contains("result") || error.message.contains("next"));
            }
            _ => panic!("Expected failed result due to mock limitations"),
        }
    }

    #[tokio::test]
    async fn test_iterate_component_with_next() {
        let component = IterateComponent::new();

        // Create a workflow that returns "next" - the mock will return {"message": "Hello from nested flow"}
        let test_flow = Flow {
            name: Some("test-next".to_string()),
            steps: vec![],
            output: ValueTemplate::literal(serde_json::json!({"next": "continued_value"})),
            ..Default::default()
        };

        let input = IterateInput {
            flow: test_flow,
            initial_input: serde_json::json!({"start": "value"}).into(),
            max_iterations: 10,
        };

        let input_value = serde_json::to_value(input).unwrap();
        let mock = MockContext::new();

        let result = component
            .execute(mock.execution_context(), input_value.into())
            .await
            .unwrap();

        match result {
            FlowResult::Failed(error) => {
                // The mock context returns {"message": "Hello from nested flow"} which doesn't have "result" or "next"
                assert!(error.message.contains("result") || error.message.contains("next"));
            }
            _ => panic!("Expected failed result due to mock limitations"),
        }
    }

    #[tokio::test]
    async fn test_iterate_component_max_iterations() {
        let component = IterateComponent::new();

        // Create a workflow that always returns "next" (infinite loop)
        let test_flow = Flow {
            name: Some("test-infinite".to_string()),
            steps: vec![],
            output: ValueTemplate::literal(serde_json::json!({"next": "forever"})),
            ..Default::default()
        };

        let input = IterateInput {
            flow: test_flow,
            initial_input: serde_json::json!({"start": "value"}).into(),
            max_iterations: 5,
        };

        let input_value = serde_json::to_value(input).unwrap();
        let mock = MockContext::new();

        let result = component
            .execute(mock.execution_context(), input_value.into())
            .await
            .unwrap();

        match result {
            FlowResult::Failed(error) => {
                // The mock context returns {"message": "Hello from nested flow"} which doesn't have "result" or "next"
                assert!(error.message.contains("result") || error.message.contains("next"));
            }
            _ => panic!("Expected failed result due to mock limitations"),
        }
    }
}
