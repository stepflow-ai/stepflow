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

use std::borrow::Cow;

use schemars::{JsonSchema, Schema};
use serde::{Deserialize, Serialize};

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
}

/// The results of a step execution.
#[derive(Debug, Clone, PartialEq, utoipa::ToSchema)]
pub enum FlowResult {
    /// # Success
    /// The step execution was successful.
    Success(ValueRef),
    /// # Skipped
    /// The step was skipped.
    Skipped,
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
            FlowResult::Skipped => {
                let mut state = serializer.serialize_struct("FlowResult", 1)?;
                state.serialize_field("outcome", "skipped")?;
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
            "skipped" => Ok(FlowResult::Skipped),
            "failed" => {
                let error = FlowError::deserialize(
                    value
                        .get("error")
                        .ok_or_else(|| D::Error::missing_field("error"))?,
                )
                .map_err(D::Error::custom)?;
                Ok(FlowResult::Failed(error))
            }
            _ => Err(D::Error::unknown_variant(
                outcome,
                &["success", "skipped", "failed"],
            )),
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
            "FlowResultSkipped".to_string(),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "outcome": {
                        "title": "FlowOutcome",
                        "const": "skipped",
                        "default": "skipped"
                    },
                },
                "required": ["outcome"],
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
                {"$ref": "#/$defs/FlowResultSkipped"},
                {"$ref": "#/$defs/FlowResultFailed"}
            ],
            "discriminator": {
                "propertyName": "outcome",
                "mapping": {
                    "success": "#/$defs/FlowResultSuccess",
                    "skipped": "#/$defs/FlowResultSkipped",
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

    pub fn skipped(&self) -> bool {
        matches!(self, Self::Skipped)
    }

    pub fn failed(&self) -> Option<&FlowError> {
        match self {
            Self::Failed(error) => Some(error),
            _ => None,
        }
    }
}
