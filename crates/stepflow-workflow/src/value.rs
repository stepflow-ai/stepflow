use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// An expression that can be either a literal value or a template expression.
#[derive(Debug, Clone, PartialEq, Hash, Eq)]
pub enum BaseRef {
    /// Reference the output of an earlier step.
    Step { step: String },
    /// Reference to the input of the flow.
    Input,
}

/// An expression that can be either a literal value or a template expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", untagged)]
pub enum Expr {
    Literal {
        literal: Value,
    },
    Input {
        #[serde(rename = "input")]
        field: String,
    },
    Step {
        step: String,
        field: Option<String>,
    },
}

impl Expr {
    pub fn literal(literal: impl Into<Value>) -> Self {
        let literal = literal.into();
        Self::Literal { literal }
    }

    pub fn step_field(step: impl Into<String>, field: impl Into<String>) -> Self {
        Self::Step {
            step: step.into(),
            field: Some(field.into()),
        }
    }

    pub fn input_field(field: impl Into<String>) -> Self {
        Self::Input {
            field: field.into(),
        }
    }

    pub fn base_ref(&self) -> Option<BaseRef> {
        match self {
            Self::Literal { .. } => None,
            Self::Input { .. } => Some(BaseRef::Input),
            Self::Step { step, .. } => Some(BaseRef::Step { step: step.clone() }),
        }
    }

    pub fn field(&self) -> Option<&str> {
        match self {
            Self::Literal { .. } => None,
            Self::Input { field, .. } => Some(field.as_str()),
            Self::Step { field, .. } => field.as_deref(),
        }
    }
}

/// A literal value which may be passed to a component.

// TODO: Change value representation to avoid copying?
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, JsonSchema)]
#[repr(transparent)]
pub struct Value(Arc<serde_json::Value>);

impl Value {
    pub fn new(value: serde_json::Value) -> Self {
        Self(Arc::new(value))
    }
}

impl AsRef<serde_json::Value> for Value {
    fn as_ref(&self) -> &serde_json::Value {
        &self.0
    }
}

impl From<serde_json::Value> for Value {
    fn from(value: serde_json::Value) -> Self {
        Self::new(value)
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Self::new(serde_json::Value::Number(value.into()))
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::new(serde_json::Value::String(value))
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self(Arc::new(serde_json::Value::String(value.to_owned())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arg_to_yaml() {
        let to_yaml = |e: &Expr| serde_yml::to_string(e).unwrap();
        assert_eq!(to_yaml(&Expr::literal("foo")), "literal: foo\n");
        assert_eq!(to_yaml(&Expr::literal(5)), "literal: 5\n");

        assert_eq!(to_yaml(&Expr::input_field("out")), "input: out\n");

        assert_eq!(
            to_yaml(&Expr::step_field("step1", "out")),
            "step: step1\nfield: out\n"
        );
    }

    #[test]
    fn test_arg_from_yaml() {
        let from_yaml = |s| serde_yml::from_str::<Expr>(s).unwrap();
        assert_eq!(from_yaml("{ literal: foo }"), Expr::literal("foo"));
        assert_eq!(from_yaml("{ literal: 5 }"), Expr::literal(5));

        assert_eq!(
            from_yaml("{ step: \"step1\", field: \"out\" }"),
            Expr::step_field("step1", "out")
        );
    }
}
