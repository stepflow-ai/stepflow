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

use std::sync::Arc;
use stepflow_core::FlowResult;
use stepflow_core::workflow::Component;
use stepflow_core::workflow::StepId;
use stepflow_core::{component::ComponentInfo, workflow::ValueRef};
use stepflow_plugin::RunContext;

use crate::{BuiltinComponent, Result};

/// A no-op component that accepts any JSON input and returns it unchanged.
///
/// Useful for benchmarking orchestrator overhead without any real work.
pub struct NoopComponent;

impl BuiltinComponent for NoopComponent {
    fn component_info(&self) -> Result<ComponentInfo> {
        Ok(ComponentInfo {
            component: Component::from_string("/noop"),
            path: "/noop".to_string(),
            input_schema: None,
            output_schema: None,
            description: Some(
                "No-op component that returns its input unchanged. Used for benchmarking."
                    .to_string(),
            ),
        })
    }

    async fn execute(
        &self,
        _run_context: &Arc<RunContext>,
        _step: Option<&StepId>,
        input: ValueRef,
    ) -> Result<FlowResult> {
        Ok(input.as_ref().clone().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_context::MockContext;

    #[tokio::test]
    async fn test_noop_passes_through_input() {
        let component = NoopComponent;
        let input = serde_json::json!({"key": "value", "num": 42});
        let mock = MockContext::new().await;
        let output = component
            .execute(&mock.run_context(), None, input.clone().into())
            .await
            .unwrap();
        let result: serde_json::Value = output.success().unwrap().deserialize().unwrap();
        assert_eq!(result, input);
    }

    #[tokio::test]
    async fn test_noop_passes_through_null() {
        let component = NoopComponent;
        let input = serde_json::Value::Null;
        let mock = MockContext::new().await;
        let output = component
            .execute(&mock.run_context(), None, input.clone().into())
            .await
            .unwrap();
        let result: serde_json::Value = output.success().unwrap().deserialize().unwrap();
        assert_eq!(result, input);
    }
}
