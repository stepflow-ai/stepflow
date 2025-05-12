use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::StepRef;

/// An expression that can be either a literal value or a template expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", untagged)]
pub enum Expr {
    /// A reference to an input of the flow.
    Input {
        input: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value_ref: Option<ValueRef>,
    },
    /// A reference to an output of an earlier step.
    Step {
        #[serde(flatten)]
        step_ref: StepRef,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value_ref: Option<ValueRef>,
    },
    /// A literal JSON value.
    #[schemars()]
    Literal { literal: Value },
}

/// A reference to a specific output of a step.
#[derive(Debug, Eq, PartialEq, Hash, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", untagged)]
pub enum ValueRef {
    Input { input: u32 },
    Step { step: u32, output: u32 },
}

impl std::fmt::Display for ValueRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Input { input } => write!(f, "input{input}"),
            Self::Step { step, output } => write!(f, "step{step}.output{output}"),
        }
    }
}

impl Expr {
    pub fn literal(literal: impl Into<Value>) -> Self {
        Self::Literal {
            literal: literal.into(),
        }
    }

    pub fn step(step: impl Into<String>, output: impl Into<String>) -> Self {
        Self::Step {
            step_ref: StepRef {
                step: step.into(),
                output: output.into(),
            },
            value_ref: None,
        }
    }
}

// A literal value which may be passed to a component.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, JsonSchema)]
#[repr(transparent)]
pub struct Value(serde_json::Value);

impl Value {
    pub const NULL: Self = Self(serde_json::Value::Null);
}

impl From<serde_json::Value> for Value {
    fn from(value: serde_json::Value) -> Self {
        Self(value)
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Self(serde_json::Value::Number(value.into()))
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Self(serde_json::Value::String(value))
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self(serde_json::Value::String(value.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arg_from_yaml() {
        let from_yaml = |s| serde_yml::from_str::<Expr>(s).unwrap();
        assert_eq!(from_yaml("{ literal: foo }"), Expr::literal("foo"));
        assert_eq!(from_yaml("{ literal: 5 }"), Expr::literal(5));

        assert_eq!(
            from_yaml("{ step: \"step1\", output: \"out\" }"),
            Expr::step("step1", "out")
        );
    }
}
