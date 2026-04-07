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

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::TaskErrorCode;
use crate::workflow::ValueRef;

/// An error reported from within a flow or step.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct FlowError {
    pub code: TaskErrorCode,
    pub message: Cow<'static, str>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<ValueRef>,
}

impl std::fmt::Display for FlowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "error({}): {}", self.code, self.message)
    }
}

impl FlowError {
    pub fn new(code: TaskErrorCode, message: impl Into<Cow<'static, str>>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data<D: serde::Serialize>(self, data: D) -> Result<Self, serde_json::Error> {
        let data = serde_json::to_value(data)?.into();
        Ok(Self {
            data: Some(data),
            ..self
        })
    }

    /// Create a FlowError from an error_stack::Report, preserving the full stack trace.
    pub fn from_error_stack<T: error_stack::Context>(report: error_stack::Report<T>) -> Self {
        use crate::error_stack::ErrorStack;

        let message = report.current_context().to_string();
        let error_stack = ErrorStack::from_error_stack(report);
        let data = match serde_json::to_value(&error_stack) {
            Ok(value) => Some(ValueRef::new(value)),
            Err(_) => None,
        };

        Self {
            code: TaskErrorCode::OrchestratorError,
            message: message.into(),
            data,
        }
    }
}

/// The results of a step execution.
#[derive(Debug, Clone, PartialEq)]
pub enum FlowResult {
    /// The step execution was successful.
    Success(ValueRef),
    /// The step failed with the given error.
    Failed(FlowError),
}

/// Schema for the success variant of [`FlowResult`].
///
/// This is a standalone type so that code generators (e.g. datamodel-code-generator)
/// emit it as a named class in `$defs` rather than an anonymous inline schema.
struct FlowResultSuccess;

impl schemars::JsonSchema for FlowResultSuccess {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "FlowResultSuccess".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let value_ref = generator.subschema_for::<ValueRef>();
        schemars::json_schema!({
            "description": "The step execution was successful.",
            "type": "object",
            "properties": {
                "outcome": { "type": "string", "const": "success", "default": "success" },
                "result": value_ref
            },
            "required": ["outcome", "result"]
        })
    }
}

/// Schema for the failed variant of [`FlowResult`].
struct FlowResultFailed;

impl schemars::JsonSchema for FlowResultFailed {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "FlowResultFailed".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let flow_error_ref = generator.subschema_for::<FlowError>();
        schemars::json_schema!({
            "description": "The step failed with the given error.",
            "type": "object",
            "properties": {
                "outcome": { "type": "string", "const": "failed", "default": "failed" },
                "error": flow_error_ref
            },
            "required": ["outcome", "error"]
        })
    }
}

impl schemars::JsonSchema for FlowResult {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "FlowResult".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let success_ref = generator.subschema_for::<FlowResultSuccess>();
        let failed_ref = generator.subschema_for::<FlowResultFailed>();

        schemars::json_schema!({
            "oneOf": [success_ref, failed_ref],
            "discriminator": {
                "propertyName": "outcome",
                "mapping": {
                    "success": "#/$defs/FlowResultSuccess",
                    "failed": "#/$defs/FlowResultFailed"
                }
            }
        })
    }
}

impl Serialize for FlowResult {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct as _;

        match self {
            FlowResult::Success(result) => {
                let mut state = serializer.serialize_struct("FlowResult", 2)?;
                state.serialize_field("outcome", "success")?;
                state.serialize_field("result", result)?;
                state.end()
            }
            FlowResult::Failed(error) => {
                let mut state = serializer.serialize_struct("FlowResult", 2)?;
                state.serialize_field("outcome", "failed")?;
                state.serialize_field("error", error)?;
                state.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for FlowResult {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error as _;

        let value = serde_json::Value::deserialize(deserializer)?;
        let outcome = value
            .get("outcome")
            .and_then(|v| v.as_str())
            .ok_or_else(|| D::Error::missing_field("outcome"))?;

        match outcome {
            "success" => {
                let result = value
                    .get("result")
                    .ok_or_else(|| D::Error::missing_field("result"))?;
                let result_ref = ValueRef::new(result.clone());
                Ok(FlowResult::Success(result_ref))
            }
            "failed" => {
                let error = FlowError::deserialize(
                    value
                        .get("error")
                        .ok_or_else(|| D::Error::missing_field("error"))?,
                )
                .map_err(D::Error::custom)?;
                Ok(FlowResult::Failed(error))
            }
            _ => Err(D::Error::unknown_variant(outcome, &["success", "failed"])),
        }
    }
}

impl From<serde_json::Value> for FlowResult {
    fn from(value: serde_json::Value) -> Self {
        let result = ValueRef::new(value);
        Self::Success(result)
    }
}

impl FlowResult {
    pub fn success(&self) -> Option<ValueRef> {
        match self {
            Self::Success(result) => Some(result.clone()),
            _ => None,
        }
    }

    pub fn failed(&self) -> Option<&FlowError> {
        match self {
            Self::Failed(error) => Some(error),
            _ => None,
        }
    }

    /// Returns true if this is a transport/infrastructure error (always retried).
    pub fn is_transport_error(&self) -> bool {
        matches!(
            self,
            Self::Failed(e) if matches!(e.code, TaskErrorCode::Unreachable | TaskErrorCode::Timeout)
        )
    }

    /// Returns true if this is a component execution error
    /// (retryable with `onError: { action: retry }`).
    pub fn is_component_execution_error(&self) -> bool {
        matches!(
            self,
            Self::Failed(e) if matches!(e.code, TaskErrorCode::ComponentFailed | TaskErrorCode::ResourceUnavailable)
        )
    }

    /// Unwrap a successful result, panicking if the result is not Success.
    ///
    /// This is primarily useful for testing where we expect a successful result.
    #[cfg(test)]
    pub fn unwrap_success(self) -> ValueRef {
        match self {
            Self::Success(result) => result,
            Self::Failed(error) => {
                panic!("Expected Success, got Failed: {}", error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_transport_error() {
        assert!(
            FlowResult::Failed(FlowError::new(TaskErrorCode::Unreachable, "test"))
                .is_transport_error()
        );
        assert!(
            FlowResult::Failed(FlowError::new(TaskErrorCode::Timeout, "test")).is_transport_error()
        );
        assert!(
            !FlowResult::Failed(FlowError::new(TaskErrorCode::ComponentFailed, "test"))
                .is_transport_error()
        );
    }

    #[test]
    fn test_is_component_execution_error() {
        assert!(
            FlowResult::Failed(FlowError::new(TaskErrorCode::ComponentFailed, "test"))
                .is_component_execution_error()
        );
        assert!(
            FlowResult::Failed(FlowError::new(TaskErrorCode::ResourceUnavailable, "test"))
                .is_component_execution_error()
        );
        assert!(
            !FlowResult::Failed(FlowError::new(TaskErrorCode::Unreachable, "test"))
                .is_component_execution_error()
        );
    }
}
