use std::borrow::Cow;

use schemars::JsonSchema;

use crate::workflow::ValueRef;

/// An error reported from within a flow or step.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum FlowResult {
    /// The step execution was successful.
    Success { result: ValueRef },
    /// The step was skipped.
    Skipped,
    /// The step failed with the given error.
    Failed { error: FlowError },
}

impl From<serde_json::Value> for FlowResult {
    fn from(value: serde_json::Value) -> Self {
        let result = ValueRef::new(value);
        Self::Success { result }
    }
}

impl FlowResult {
    pub fn success(&self) -> Option<&serde_json::Value> {
        match self {
            Self::Success { result } => Some(result.as_ref()),
            _ => None,
        }
    }

    pub fn skipped(&self) -> bool {
        matches!(self, Self::Skipped)
    }

    pub fn failed(&self) -> Option<&FlowError> {
        match self {
            Self::Failed { error } => Some(error),
            _ => None,
        }
    }
}
