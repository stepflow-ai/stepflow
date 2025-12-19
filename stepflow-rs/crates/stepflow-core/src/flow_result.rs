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

use schemars::{JsonSchema, Schema};
use serde::{Deserialize, Serialize};

use crate::error_stack::ErrorStack;
use crate::workflow::ValueRef;

/// An error reported from within a flow or step.
#[derive(
    Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, JsonSchema, utoipa::ToSchema,
)]
pub struct FlowError {
    pub code: i64,
    pub message: Cow<'static, str>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<ValueRef>,
}

impl std::fmt::Display for FlowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "error({}): {}", self.code, self.message)
    }
}

pub const FLOW_ERROR_UNDEFINED_FIELD: i64 = 1;

impl FlowError {
    pub fn new(code: i64, message: impl Into<Cow<'static, str>>) -> Self {
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

    /// Create a FlowError from an error_stack::Report, preserving the full stack trace
    pub fn from_error_stack<T: error_stack::Context>(report: error_stack::Report<T>) -> Self {
        // Extract the root error as the main message
        let message = report.current_context().to_string();

        // Create ErrorStack using the shared implementation
        let error_stack = ErrorStack::from_error_stack(report);

        // Serialize the error stack to ValueRef for the data field
        let data = match serde_json::to_value(&error_stack) {
            Ok(value) => Some(ValueRef::new(value)),
            Err(_) => None, // If serialization fails, proceed without stack data
        };

        Self {
            code: 500, // Default to internal server error for system errors
            message: message.into(),
            data,
        }
    }
}

/// The results of a step execution.
#[derive(Debug, Clone, PartialEq, utoipa::ToSchema)]
pub enum FlowResult {
    /// # Success
    /// The step execution was successful.
    Success(ValueRef),
    /// # Failed
    /// The step failed with the given error.
    Failed(FlowError),
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

impl JsonSchema for FlowResult {
    fn schema_name() -> Cow<'static, str> {
        "FlowResult".into()
    }

    fn json_schema(generator: &mut schemars::generate::SchemaGenerator) -> Schema {
        let value_schema = generator.subschema_for::<ValueRef>();
        let error_schema = generator.subschema_for::<FlowError>();
        let defs = generator.definitions_mut();
        defs.insert(
            "FlowResultSuccess".to_string(),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "outcome": {
                        "title": "FlowOutcome",
                        "const": "success",
                        "default": "success",
                    },
                    "result": value_schema,
                },
                "required": ["outcome", "result"],
            }),
        );
        defs.insert(
            "FlowResultFailed".to_string(),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "outcome": {
                        "title": "FlowOutcome",
                        "const": "failed",
                        "default": "failed"
                    },
                    "error": error_schema,
                },
                "required": ["outcome", "error"],
            }),
        );

        schemars::json_schema!({
            "title": "FlowResult",
            "description": "The results of a step execution.",
            "oneOf": [
                {"$ref": "#/$defs/FlowResultSuccess"},
                {"$ref": "#/$defs/FlowResultFailed"}
            ],
            "discriminator": {
                "propertyName": "outcome",
                "mapping": {
                    "success": "#/$defs/FlowResultSuccess",
                    "failed": "#/$defs/FlowResultFailed"
                }
            },
        })
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
    use error_stack::{Context, report};

    #[derive(Debug)]
    struct TestError(&'static str);

    impl std::fmt::Display for TestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "TestError: {}", self.0)
        }
    }

    impl Context for TestError {}

    #[test]
    fn test_flow_error_from_error_stack_serialization() {
        // Create a test error stack
        let report = report!(TestError("root cause"))
            .attach_printable("Additional context")
            .change_context(TestError("higher level error"));

        // Convert to FlowError
        let flow_error = FlowError::from_error_stack(report);

        // Verify basic fields
        assert_eq!(flow_error.code, 500);
        assert_eq!(flow_error.message, "TestError: higher level error");
        assert!(flow_error.data.is_some());

        // Serialize to JSON
        let json = serde_json::to_string(&flow_error).expect("Should serialize successfully");

        // Verify data field is included in JSON
        assert!(json.contains("\"data\":"));

        // Parse back to verify structure
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("Should parse back");
        assert!(parsed.get("data").is_some());

        let data = parsed.get("data").unwrap();
        assert!(data.get("stack").is_some());

        let stack = data
            .get("stack")
            .unwrap()
            .as_array()
            .expect("Stack should be array");
        assert!(!stack.is_empty(), "Stack should not be empty");

        // Check first stack entry
        let first_entry = &stack[0];
        assert!(first_entry.get("error").is_some());
        assert!(first_entry.get("attachments").is_some());
    }

    #[test]
    fn test_flow_result_failed_serialization() {
        // Create a test error stack
        let report = report!(TestError("component failed"))
            .attach_printable("Step execution error")
            .change_context(TestError("workflow step failed"));

        let flow_error = FlowError::from_error_stack(report);
        let flow_result = FlowResult::Failed(flow_error);

        // Serialize to JSON
        let json = serde_json::to_string(&flow_result).expect("Should serialize successfully");

        // Verify the full structure
        assert!(json.contains("\"outcome\":\"failed\""));
        assert!(json.contains("\"error\":{"));
        assert!(json.contains("\"data\":{"));
        assert!(json.contains("\"stack\":["));
    }

    #[test]
    fn test_data_field_structure_and_content() {
        // Create a complex error stack with multiple contexts and attachments
        let report = report!(TestError("database connection failed"))
            .attach_printable("Connection timeout: 30s")
            .attach_printable("Host: localhost:5432")
            .change_context(TestError("plugin initialization failed"))
            .attach_printable("Plugin: langflow")
            .change_context(TestError("step execution failed"));

        let flow_error = FlowError::from_error_stack(report);

        // Verify data field exists and is Some
        assert!(flow_error.data.is_some(), "Data field should be populated");

        // Serialize and parse to verify exact structure
        let json = serde_json::to_string(&flow_error).expect("Should serialize successfully");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("Should parse back");

        // Verify data field structure
        let data = parsed.get("data").expect("Should have data field");
        let stack = data
            .get("stack")
            .expect("Should have stack field")
            .as_array()
            .expect("Stack should be array");

        // Verify we have multiple stack entries
        assert!(stack.len() >= 2, "Should have multiple stack entries");

        // Verify first entry (most recent error)
        let first_entry = &stack[0];
        assert_eq!(
            first_entry.get("error").unwrap().as_str(),
            Some("TestError: step execution failed")
        );

        let attachments = first_entry
            .get("attachments")
            .unwrap()
            .as_array()
            .expect("Should have attachments");
        assert!(
            attachments
                .iter()
                .any(|a| a.as_str().unwrap().contains("Plugin: langflow"))
        );

        // Verify last entry (root cause)
        let last_entry = stack.last().unwrap();
        assert_eq!(
            last_entry.get("error").unwrap().as_str(),
            Some("TestError: database connection failed")
        );
    }
}
