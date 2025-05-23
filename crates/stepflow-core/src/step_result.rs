use std::borrow::Cow;

use crate::workflow::ValueRef;

/// An error reported from within a flow or step.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct FlowError {
    pub code: i64,
    pub message: Cow<'static, str>,
    pub data: Option<ValueRef>,
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
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum FlowResult {
    /// The step execution was successful.
    Success(ValueRef),
    /// The step was skipped.
    Skipped,
    /// The step failed with the given error.
    Failed(FlowError),
}

impl From<serde_json::Value> for FlowResult {
    fn from(value: serde_json::Value) -> Self {
        Self::Success(ValueRef::new(value))
    }
}

impl FlowResult {
    pub fn success(&self) -> Option<&serde_json::Value> {
        match self {
            Self::Success(value) => Some(value.as_ref()),
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
