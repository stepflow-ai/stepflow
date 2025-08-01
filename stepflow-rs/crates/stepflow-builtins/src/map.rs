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
use futures::future;
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

/// Component for mapping a workflow over a list of items.
pub struct MapComponent;

impl MapComponent {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MapComponent {
    fn default() -> Self {
        Self::new()
    }
}

/// Input for the map component
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
struct MapInput {
    /// The workflow to apply to each item
    workflow: Flow,

    /// The list of items to process
    items: Vec<ValueRef>,
}

/// Output from the map component
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
struct MapOutput {
    /// The results from processing each item
    results: Vec<FlowResult>,

    /// Summary statistics
    successful: u32,
    failed: u32,
    skipped: u32,
}

impl BuiltinComponent for MapComponent {
    fn component_info(&self) -> Result<ComponentInfo> {
        let input_schema = SchemaRef::for_type::<MapInput>();
        let output_schema = SchemaRef::for_type::<MapOutput>();

        Ok(ComponentInfo {
            component: stepflow_core::workflow::Component::from_string("/map"),
            input_schema: Some(input_schema),
            output_schema: Some(output_schema),
            description: Some(
                "Apply a workflow to each item in a list and collect the results".to_string(),
            ),
        })
    }

    async fn execute(&self, context: ExecutionContext, input: ValueRef) -> Result<FlowResult> {
        let input: MapInput = serde_json::from_value(input.as_ref().clone())
            .change_context(BuiltinError::InvalidInput)?;

        let flow = Arc::new(input.workflow);
        let workflow_hash = Flow::hash(&flow);

        let mut successful = 0u32;
        let mut failed = 0u32;
        let mut skipped = 0u32;

        // Process each item in parallel
        let tasks = input.items.into_iter().map(|item| {
            let context = context.clone();
            let flow = flow.clone();
            let workflow_hash = workflow_hash.clone();

            tokio::spawn(async move {
                context
                    .execute_flow(flow, workflow_hash, item)
                    .await
                    .change_context(BuiltinError::Internal)
            })
        });

        // Wait for all tasks to complete
        let results: Vec<FlowResult> = future::join_all(tasks)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .change_context(BuiltinError::Internal)?
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;

        // Update counters
        for result in &results {
            match result {
                FlowResult::Success(_) => successful += 1,
                FlowResult::Failed { .. } => failed += 1,
                FlowResult::Skipped => skipped += 1,
            }
        }

        let output = MapOutput {
            results,
            successful,
            failed,
            skipped,
        };

        let output_value = serde_json::to_value(output).change_context(BuiltinError::Internal)?;

        Ok(FlowResult::Success(ValueRef::new(output_value)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_context::MockContext;
    use stepflow_core::values::ValueTemplate;
    use stepflow_core::workflow::FlowV1;

    #[tokio::test]
    async fn test_map_component_success() {
        let component = MapComponent::new();

        // Create a workflow that doubles the input value
        let test_flow = Flow::V1(FlowV1 {
            name: Some("test-double".to_string()),
            steps: vec![],
            output: ValueTemplate::literal(serde_json::json!({
                "doubled": 42
            })),
            ..Default::default()
        });

        let input = MapInput {
            workflow: test_flow,
            items: vec![
                serde_json::json!(1).into(),
                serde_json::json!(2).into(),
                serde_json::json!(3).into(),
            ],
        };

        let input_value = serde_json::to_value(input).unwrap();
        let mock = MockContext::new();

        let result = component
            .execute(mock.execution_context(), input_value.into())
            .await
            .unwrap();

        match result {
            FlowResult::Success(result) => {
                let output: MapOutput = serde_json::from_value(result.as_ref().clone()).unwrap();
                assert_eq!(output.results.len(), 3);
                assert_eq!(output.successful, 3);
                assert_eq!(output.failed, 0);
                assert_eq!(output.skipped, 0);
            }
            _ => panic!("Expected success result"),
        }
    }

    #[tokio::test]
    async fn test_map_component_empty_list() {
        let component = MapComponent::new();

        let test_flow = Flow::V1(FlowV1 {
            name: Some("test-empty".to_string()),
            steps: vec![],
            output: ValueTemplate::literal(serde_json::json!({"result": "processed"})),
            ..Default::default()
        });

        let input = MapInput {
            workflow: test_flow,
            items: vec![],
        };

        let input_value = serde_json::to_value(input).unwrap();
        let mock = MockContext::new();

        let result = component
            .execute(mock.execution_context(), input_value.into())
            .await
            .unwrap();

        match result {
            FlowResult::Success(result) => {
                let output: MapOutput = serde_json::from_value(result.as_ref().clone()).unwrap();
                assert_eq!(output.results.len(), 0);
                assert_eq!(output.successful, 0);
                assert_eq!(output.failed, 0);
                assert_eq!(output.skipped, 0);
            }
            _ => panic!("Expected success result"),
        }
    }
}
