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

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::workflow::ValueRef;

/// An expression that can be either a literal value or a template expression.
#[derive(
    Debug, Clone, PartialEq, Hash, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub enum BaseRef {
    /// # WorkflowReference
    /// Reference properties of the workflow.
    Workflow(WorkflowRef),
    /// # StepReference
    /// Reference the output of a step.
    #[serde(untagged)]
    Step { step: String },
}

impl BaseRef {
    pub const WORKFLOW_INPUT: Self = Self::Workflow(WorkflowRef::Input);

    pub fn step_output(step: impl Into<String>) -> Self {
        Self::Step { step: step.into() }
    }
}

#[derive(
    Debug, Clone, PartialEq, Hash, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub enum WorkflowRef {
    Input,
}

/// An expression that can be either a literal value or a template expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(untagged, rename_all = "camelCase")]
pub enum Expr {
    /// # Reference
    /// Reference a value from a step, workflow, or other source.
    #[serde(rename_all = "camelCase")]
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

        #[serde(default, skip_serializing_if = "SkipAction::is_default")]
        on_skip: SkipAction,
    },
    /// # EscapedLiteral
    /// A literal value that was escaped.
    ///
    /// No template expansion is performed within the value, allowing
    /// for raw JSON values that include `$from` or other special characters.
    EscapedLiteral {
        /// A literal value that should not be expanded for expressions.
        /// This allows creating JSON values that contain `$from` without expansion.
        #[serde(rename = "$literal")]
        literal: ValueRef,
    },
    /// # Literal
    /// A direct literal value that serializes naturally without special syntax
    Literal(ValueRef),
}

impl Expr {
    /// Create a direct literal expression that serializes naturally (like `"foo"`)
    pub fn literal(literal: impl Into<ValueRef>) -> Self {
        Self::Literal(literal.into())
    }

    /// Create an escaped literal expression with `$literal` syntax
    /// This prevents template expansion within the value
    pub fn escaped_literal(literal: impl Into<ValueRef>) -> Self {
        Self::EscapedLiteral {
            literal: literal.into(),
        }
    }

    fn new_ref(from: BaseRef, path: String, on_skip: SkipAction) -> Self {
        let path = Some(path).filter(|s| !s.is_empty());
        Self::Ref {
            from,
            path,
            on_skip,
        }
    }

    pub fn step_path(
        step: impl Into<String>,
        path: impl Into<String>,
        on_skip: SkipAction,
    ) -> Self {
        Self::new_ref(BaseRef::step_output(step), path.into(), on_skip)
    }

    pub fn input_path(path: impl Into<String>, on_skip: SkipAction) -> Self {
        Self::new_ref(BaseRef::WORKFLOW_INPUT, path.into(), on_skip)
    }

    // Convenience constructors with default skip behavior

    /// Create a step reference
    /// - `step_ref("step1", None)` creates `{"$from": {"step": "step1"}}`
    /// - `step_ref("step1", Some("field"))` creates `{"$from": {"step": "step1"}, "path": "field"}`
    pub fn step_ref(step_id: impl Into<String>, path: Option<&str>) -> Self {
        Self::Ref {
            from: BaseRef::step_output(step_id),
            path: path.map(|p| p.to_string()),
            on_skip: SkipAction::default(),
        }
    }

    /// Create a workflow input reference
    /// - `workflow_input(None)` creates `{"$from": {"workflow": "input"}}`
    /// - `workflow_input(Some("field"))` creates `{"$from": {"workflow": "input"}, "path": "field"}`
    pub fn workflow_input(path: Option<&str>) -> Self {
        Self::Ref {
            from: BaseRef::WORKFLOW_INPUT,
            path: path.map(|p| p.to_string()),
            on_skip: SkipAction::default(),
        }
    }

    pub fn base_ref(&self) -> Option<&BaseRef> {
        match self {
            Self::EscapedLiteral { .. } => None,
            Self::Literal(_) => None,
            Self::Ref { from, .. } => Some(from),
        }
    }

    pub fn path(&self) -> Option<&str> {
        match self {
            Self::EscapedLiteral { .. } => None,
            Self::Literal(_) => None,
            Self::Ref { path, .. } => path.as_deref(),
        }
    }

    pub fn on_skip(&self) -> Option<&SkipAction> {
        match self {
            Self::EscapedLiteral { .. } => None,
            Self::Literal(_) => None,
            Self::Ref { on_skip, .. } => Some(on_skip),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", tag = "action")]
pub enum SkipAction {
    /// # OnSkipSkip
    Skip,
    #[serde(rename_all = "camelCase")]
    /// # OnSkipDefault
    UseDefault {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default_value: Option<ValueRef>,
    },
}

impl Default for SkipAction {
    fn default() -> Self {
        Self::Skip
    }
}

impl SkipAction {
    pub fn is_default(&self) -> bool {
        matches!(self, Self::Skip)
    }

    pub fn is_optional(&self) -> bool {
        matches!(self, Self::UseDefault { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expr_to_yaml() {
        insta::assert_yaml_snapshot!(&Expr::literal("foo"), @r#"foo"#);
        insta::assert_yaml_snapshot!(&Expr::literal(5), @r#"5"#);

        // Input reference with and without path, with default skip action (skip).
        insta::assert_yaml_snapshot!(&Expr::input_path("", SkipAction::Skip),
            @r###"
        $from:
          workflow: input
        "###);
        insta::assert_yaml_snapshot!(&Expr::input_path("out", SkipAction::Skip),
            @r###"
        $from:
          workflow: input
        path: out
        "###);

        // We don't test input references with skip actions, since they don't make sense.
        // In fact, we should have a validation that ensures they aren't set.

        // Step reference with and without path, with default skip action (skip).
        insta::assert_yaml_snapshot!(&Expr::step_path("step1", "", SkipAction::Skip),
            @r###"
        $from:
          step: step1
        "###);
        insta::assert_yaml_snapshot!(&Expr::step_path("step1", "out", SkipAction::Skip),
            @r###"
        $from:
          step: step1
        path: out
        "###);

        // Step reference with and without path, with use_default skip action (use default).
        insta::assert_yaml_snapshot!(&Expr::step_path("step1", "", SkipAction::UseDefault { default_value: None }),
            @r###"
        $from:
          step: step1
        onSkip:
          action: useDefault
        "###);
        insta::assert_yaml_snapshot!(&Expr::step_path("step1", "out", SkipAction::UseDefault { default_value: None }),
            @r###"
        $from:
          step: step1
        path: out
        onSkip:
          action: useDefault
        "###);

        // Step reference with and without path, with use_default skip action (and default vaule).
        let value: ValueRef = serde_json::Value::String("test_default".to_owned()).into();
        insta::assert_yaml_snapshot!(&Expr::step_path("step1", "out", SkipAction::UseDefault { default_value: Some(value.clone()) }),
            @r###"
        $from:
          step: step1
        path: out
        onSkip:
          action: useDefault
          defaultValue: test_default
        "###);
        insta::assert_yaml_snapshot!(&Expr::step_path("step1", "", SkipAction::UseDefault { default_value: Some(value) }),
            @r###"
        $from:
          step: step1
        onSkip:
          action: useDefault
          defaultValue: test_default
        "###);
    }

    #[test]
    fn test_expr_from_yaml() {
        let from_yaml = |s| serde_yaml_ng::from_str::<Expr>(s).unwrap();
        assert_eq!(from_yaml("foo"), Expr::literal("foo"));
        assert_eq!(from_yaml("5"), Expr::literal(5));

        assert_eq!(
            from_yaml("{ $from: { step: \"step1\" } }"),
            Expr::step_path("step1", "", SkipAction::Skip)
        );
        assert_eq!(
            from_yaml("{ $from: { step: \"step1\" }, path: \"out\" }"),
            Expr::step_path("step1", "out", SkipAction::Skip)
        );
    }

    #[test]
    fn test_skip_action_deserialization() {
        let skip: SkipAction = serde_yaml_ng::from_str("action: skip").unwrap();
        assert_eq!(skip, SkipAction::Skip);

        let use_default_no_value: SkipAction =
            serde_yaml_ng::from_str("action: useDefault").unwrap();
        assert_eq!(
            use_default_no_value,
            SkipAction::UseDefault {
                default_value: None
            }
        );

        let use_default_with_value: SkipAction =
            serde_yaml_ng::from_str("action: useDefault\ndefaultValue: test_default").unwrap();
        assert_eq!(
            use_default_with_value,
            SkipAction::UseDefault {
                default_value: Some(ValueRef::from("test_default"))
            }
        );
    }

    #[test]
    fn test_expr_with_skip_action_from_yaml() {
        let expr_with_skip: Expr = serde_yaml_ng::from_str(
            "$from: { step: step1 }\npath: out\nonSkip:\n  action: useDefault\n  defaultValue: fallback",
        )
        .unwrap();

        assert_eq!(
            expr_with_skip,
            Expr::step_path(
                "step1",
                "out",
                SkipAction::UseDefault {
                    default_value: Some(ValueRef::from("fallback"))
                }
            )
        );

        let expr_with_default_skip: Expr =
            serde_yaml_ng::from_str("$from: { step: step1 }\npath: out").unwrap();

        assert_eq!(
            expr_with_default_skip,
            Expr::step_path("step1", "out", SkipAction::Skip)
        );
    }
}
