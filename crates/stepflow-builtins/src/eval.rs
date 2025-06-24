use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_core::workflow::FlowHash;
use stepflow_core::{
    FlowResult,
    component::ComponentInfo,
    schema::SchemaRef,
    workflow::{Flow, ValueRef},
};
use stepflow_plugin::{Context as _, ExecutionContext};

use crate::{BuiltinComponent, Result, error::BuiltinError};

/// Component for executing nested workflows.
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
    /// The workflow to execute
    workflow: Flow,

    /// Precomputed hash of the workflow.
    workflow_hash: Option<FlowHash>,

    /// The input to pass to the workflow
    input: ValueRef,
}

/// Output from the eval component
///
/// The output is simply the result of the nested workflow execution
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
struct EvalOutput {
    /// The result from executing the nested workflow
    result: ValueRef,

    /// The execution ID of the nested workflow
    execution_id: String,
}

impl BuiltinComponent for EvalComponent {
    fn component_info(&self) -> Result<ComponentInfo> {
        let input_schema = SchemaRef::for_type::<EvalInput>();
        let output_schema = SchemaRef::for_type::<EvalOutput>();

        Ok(ComponentInfo {
            input_schema,
            output_schema,
            description: Some(
                "Execute a nested workflow with given input and return the result".to_string(),
            ),
        })
    }

    async fn execute(&self, context: ExecutionContext, input: ValueRef) -> Result<FlowResult> {
        let input: EvalInput = serde_json::from_value(input.as_ref().clone())
            .change_context(BuiltinError::InvalidInput)?;

        let flow = Arc::new(input.workflow);
        let workflow_hash = input.workflow_hash.unwrap_or_else(|| Flow::hash(&flow));
        let workflow_input = input.input;

        // Execute the nested workflow
        let nested_result = context
            .execute_flow(flow, workflow_hash, workflow_input)
            .await
            .change_context(BuiltinError::Internal)?;

        // Extract the result value
        let result_value = match nested_result {
            FlowResult::Success { result } => result.as_ref().clone(),
            FlowResult::Skipped => serde_json::Value::Null,
            FlowResult::Streaming { stream_id, metadata, chunk, chunk_index, is_final } => {
                // For streaming results, return the metadata and chunk info
                serde_json::json!({
                    "stream_id": stream_id,
                    "metadata": metadata.as_ref(),
                    "chunk": chunk,
                    "chunk_index": chunk_index,
                    "is_final": is_final
                })
            }
            FlowResult::Failed { error } => {
                // Propagate the failure from the nested workflow
                return Ok(FlowResult::Failed { error });
            }
        };

        let output = EvalOutput {
            result: result_value.into(),
            execution_id: context.execution_id().to_string(),
        };

        let output_value = serde_json::to_value(output).change_context(BuiltinError::Internal)?;

        Ok(FlowResult::Success {
            result: ValueRef::new(output_value),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_context::MockContext;

    #[tokio::test]
    async fn test_eval_component() {
        let component = EvalComponent::new();

        // Create a simple test workflow
        let test_flow = Flow {
            name: Some("test-nested".to_string()),
            steps: vec![],
            output: ValueRef::new(serde_json::json!({
                "result": "Hello from nested flow"
            })),
            ..Default::default()
        };

        let input = EvalInput {
            workflow: test_flow,
            workflow_hash: None,
            input: serde_json::json!({}).into(),
        };

        let input_value = serde_json::to_value(input).unwrap();
        let mock = MockContext::new();

        let result = component
            .execute(mock.execution_context(), input_value.into())
            .await
            .unwrap();

        match result {
            FlowResult::Success { result } => {
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
