use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::workflow::Value;

/// An expression that can be either a literal value or a template expression.
#[derive(Debug, Clone, PartialEq, Hash, Eq, Serialize, Deserialize, JsonSchema)]
pub enum BaseRef {
    /// Reference to the input of the flow.
    #[serde(rename = "$input")]
    Input,
    #[serde(untagged)]
    /// Reference the output of an earlier step.
    Step(String),
}

/// An expression that can be either a literal value or a template expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Expr {
    Ref {
        /// The source of the reference.
        #[serde(rename = "$from")]
        from: BaseRef,
        /// JSON pointer expression to apply to the referenced value.
        ///
        /// May be omitted to use the entire value.
        /// May also be a bare field name (without the leading `/`) if
        /// the referenced value is an object.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },
    Literal(Value),
}

impl Expr {
    pub fn literal(literal: impl Into<Value>) -> Self {
        Self::Literal(literal.into())
    }

    fn new_ref(from: BaseRef, path: String) -> Self {
        let path = Some(path).filter(|s| !s.is_empty());
        Self::Ref { from, path }
    }

    pub fn step_path(step: impl Into<String>, path: impl Into<String>) -> Self {
        Self::new_ref(BaseRef::Step(step.into()), path.into())
    }

    pub fn input_path(path: impl Into<String>) -> Self {
        Self::new_ref(BaseRef::Input, path.into())
    }

    pub fn base_ref(&self) -> Option<&BaseRef> {
        match self {
            Self::Literal { .. } => None,
            Self::Ref { from, .. } => Some(from),
        }
    }

    pub fn field(&self) -> Option<&str> {
        match self {
            Self::Literal { .. } => None,
            Self::Ref { path: field, .. } => field.as_deref(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expr_to_yaml() {
        let to_yaml = |e: &Expr| serde_yml::to_string(e).unwrap();
        assert_eq!(to_yaml(&Expr::literal("foo")), "foo\n");
        assert_eq!(to_yaml(&Expr::literal(5)), "5\n");

        assert_eq!(
            to_yaml(&Expr::input_path("out")),
            "$from: $input\npath: out\n"
        );
        assert_eq!(to_yaml(&Expr::input_path("")), "$from: $input\n");

        assert_eq!(to_yaml(&Expr::step_path("step1", "")), "$from: step1\n");
        assert_eq!(
            to_yaml(&Expr::step_path("step1", "out")),
            "$from: step1\npath: out\n"
        );
    }

    #[test]
    fn test_expr_from_yaml() {
        let from_yaml = |s| serde_yml::from_str::<Expr>(s).unwrap();
        assert_eq!(from_yaml("foo"), Expr::literal("foo"));
        assert_eq!(from_yaml("5"), Expr::literal(5));

        assert_eq!(
            from_yaml("{ $from: \"step1\" }"),
            Expr::step_path("step1", "")
        );
        assert_eq!(
            from_yaml("{ $from: \"step1\", path: \"out\" }"),
            Expr::step_path("step1", "out")
        );
    }
}
