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
use std::time::Duration;

use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use stepflow_core::FlowResult;
use stepflow_core::workflow::Component;
use stepflow_core::workflow::StepId;
use stepflow_core::{component::ComponentInfo, schema::SchemaRef, workflow::ValueRef};
use stepflow_plugin::RunContext;

use crate::{BuiltinComponent, Result, error::BuiltinError};

/// A sleep component that waits for a specified duration before returning.
///
/// Uses `tokio::time::sleep` which is cooperative and lock-free, so thousands
/// of concurrent sleep tasks will not exhaust threads. Useful for benchmarking
/// orchestrator concurrent flow capacity with simulated worker latency.
pub struct SleepComponent;

#[derive(Serialize, Deserialize, schemars::JsonSchema, Default)]
struct SleepInput {
    /// Duration to sleep in milliseconds.
    duration_ms: u64,

    /// Optional data to pass through unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    passthrough: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, schemars::JsonSchema, Default)]
struct SleepOutput {
    /// The duration that was slept, in milliseconds.
    duration_ms: u64,

    /// The passthrough data, if any was provided.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    passthrough: Option<serde_json::Value>,
}

impl BuiltinComponent for SleepComponent {
    fn component_info(&self) -> Result<ComponentInfo> {
        let input_schema = SchemaRef::for_type::<SleepInput>();
        let output_schema = SchemaRef::for_type::<SleepOutput>();

        Ok(ComponentInfo {
            component: Component::from_string("/sleep"),
            path: "/sleep".to_string(),
            input_schema: Some(input_schema),
            output_schema: Some(output_schema),
            description: Some(
                "Sleeps for a specified duration then returns. Used for benchmarking concurrent flow capacity."
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
        let SleepInput {
            duration_ms,
            passthrough,
        } = serde_json::from_value(input.as_ref().clone())
            .change_context(BuiltinError::InvalidInput)?;

        tokio::time::sleep(Duration::from_millis(duration_ms)).await;

        let result = SleepOutput {
            duration_ms,
            passthrough,
        };
        let output = serde_json::to_value(result).change_context(BuiltinError::Internal)?;
        Ok(output.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_context::MockContext;

    #[tokio::test]
    async fn test_sleep_returns_duration_and_passthrough() {
        let component = SleepComponent;
        let input = SleepInput {
            duration_ms: 10,
            passthrough: Some(serde_json::json!({"data": "hello"})),
        };
        let input = serde_json::to_value(input).unwrap();
        let mock = MockContext::new().await;
        let output = component
            .execute(&mock.run_context(), None, input.into())
            .await
            .unwrap();
        let result: SleepOutput = output.success().unwrap().deserialize().unwrap();
        assert_eq!(result.duration_ms, 10);
        assert_eq!(
            result.passthrough,
            Some(serde_json::json!({"data": "hello"}))
        );
    }

    #[tokio::test]
    async fn test_sleep_without_passthrough() {
        let component = SleepComponent;
        let input = serde_json::json!({"duration_ms": 1});
        let mock = MockContext::new().await;
        let output = component
            .execute(&mock.run_context(), None, input.into())
            .await
            .unwrap();
        let result: SleepOutput = output.success().unwrap().deserialize().unwrap();
        assert_eq!(result.duration_ms, 1);
        assert_eq!(result.passthrough, None);
    }

    /// Regression test for #866: integer inputs should not be coerced to floats.
    /// `duration_ms` is `u64`, and passing `json!(10)` (integer) must work.
    #[tokio::test]
    async fn test_sleep_accepts_integer_input() {
        let component = SleepComponent;
        // Pass integer (not float) - this is what workflows produce from `"duration_ms": 10`
        let input = serde_json::json!({"duration_ms": 10});
        assert!(
            input["duration_ms"].is_u64(),
            "Precondition: input should be integer, got {:?}",
            input["duration_ms"]
        );
        let mock = MockContext::new().await;
        let output = component
            .execute(&mock.run_context(), None, input.into())
            .await
            .unwrap();
        let result: SleepOutput = output.success().unwrap().deserialize().unwrap();
        assert_eq!(result.duration_ms, 10);
    }

    #[tokio::test]
    async fn test_sleep_actually_delays() {
        let component = SleepComponent;
        let input = serde_json::json!({"duration_ms": 50});
        let mock = MockContext::new().await;
        let start = std::time::Instant::now();
        component
            .execute(&mock.run_context(), None, input.into())
            .await
            .unwrap();
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(40),
            "Expected at least 40ms delay, got {:?}",
            elapsed
        );
    }
}
