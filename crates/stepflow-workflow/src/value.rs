use serde::{Deserialize, Serialize};

/// An expression that can be either a literal value or a template expression.
///
/// Template expressions use the syntax `{{ <step_id>.<output_name> }}` to reference
/// outputs from previous steps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", untagged)]
pub enum Expr {
    /// A reference to an earlier step.
    Step {
        /// The step being referenced.
        step: String,
        /// The name of the step output being referenced.
        output: String,
    },
    /// A literal JSON value.
    Literal { literal: Value },
}

impl Expr {
    pub fn literal(literal: impl Into<Value>) -> Self {
        Self::Literal { literal: literal.into() }
    }

    pub fn step(step: impl Into<String>, output: impl Into<String>) -> Self {
        Self::Step { step: step.into(), output: output.into() }
    }
}

// A literal value which may be passed to a component.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[repr(transparent)]
pub struct Value(serde_json::Value);

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

        assert_eq!(from_yaml("{ step: \"step1\", output: \"out\" }"), Expr::step("step1", "out"));
    }
}
