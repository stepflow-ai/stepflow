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

use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use stepflow_core::workflow::Component;
use stepflow_core::{
    BlobId, FlowResult, component::ComponentInfo, schema::SchemaRef, workflow::ValueRef,
};
use stepflow_plugin::{Context as _, ExecutionContext};

use crate::{BuiltinComponent, Result, error::BuiltinError};

/// Component for executing nested flows.
pub struct EvalComponent;

impl EvalComponent {
    pub fn new() -> Self {
        Self
    }
}

impl Default for EvalComponent {
    fn default() -> Self {
        Self::new()
    }
}

/// Input for the eval component
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
struct EvalInput {
    /// The ID of the workflow to execute (blob ID)
    flow_id: BlobId,

    /// The input to pass to the workflow
    input: ValueRef,
}

/// Output from the eval component
///
/// The output is simply the result of the nested workflow execution
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
struct EvalOutput {
    /// The result from executing the nested workflow
    result: FlowResult,

    /// The run ID of the nested workflow
    run_id: String,
}

impl BuiltinComponent for EvalComponent {
    fn component_info(&self) -> Result<ComponentInfo> {
        let input_schema = SchemaRef::for_type::<EvalInput>();
        let output_schema = SchemaRef::for_type::<EvalOutput>();

        Ok(ComponentInfo {
            component: Component::from_string("/eval"),
            input_schema: Some(input_schema),
            output_schema: Some(output_schema),
            description: Some(
                "Execute a nested workflow with given input and return the result".to_string(),
            ),
        })
    }

    async fn execute(&self, context: ExecutionContext, input: ValueRef) -> Result<FlowResult> {
        let input: EvalInput = serde_json::from_value(input.as_ref().clone())
            .change_context(BuiltinError::InvalidInput)?;

        // Execute the nested workflow using the shared utility
        let workflow_input = input.input;
        let flow_result = context
            .execute_flow_by_id(&input.flow_id, workflow_input, None)
            .await
            .change_context(BuiltinError::Internal)?;

        // Wrap that to make the evaluation output
        let output = EvalOutput {
            result: flow_result.clone(),
            run_id: context.run_id().to_string(),
        };

        let result = FlowResult::Success(ValueRef::new(serde_json::to_value(output).unwrap()));
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use stepflow_core::ValueExpr;
    use stepflow_core::workflow::{Flow, FlowV1};

    use super::*;
    use crate::mock_context::MockContext;

    #[tokio::test]
    async fn test_eval_component() {
        let component = EvalComponent::new();
        let mock = MockContext::new();

        // Create a simple test workflow
        let test_flow = Flow::V1(FlowV1 {
            name: Some("test-nested".to_string()),
            steps: vec![],
            output: ValueExpr::Literal(serde_json::json!({
                "result": "Hello from nested flow"
            })),
            ..Default::default()
        });

        // Store the flow as a blob first
        let flow_data = ValueRef::new(serde_json::to_value(&test_flow).unwrap());
        let flow_id = mock
            .execution_context()
            .state_store()
            .put_blob(flow_data, stepflow_core::BlobType::Flow)
            .await
            .unwrap();

        let input = EvalInput {
            flow_id,
            input: serde_json::json!({}).into(),
        };

        let input_value = serde_json::to_value(input).unwrap();

        let result = component
            .execute(mock.execution_context(), input_value.into())
            .await
            .unwrap();

        match result {
            FlowResult::Success(result) => {
                let output: EvalOutput = serde_json::from_value(result.as_ref().clone()).unwrap();
                assert_eq!(
                    output.result,
                    serde_json::json!({"message": "Hello from nested flow"}).into()
                );
            }
            _ => panic!("Expected success result"),
        }
    }
}
