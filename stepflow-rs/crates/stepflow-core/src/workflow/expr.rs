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

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::workflow::{ValueRef, json_path::JsonPath};

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
    /// # VariableReference
    /// Reference a workflow variable.
    #[serde(untagged)]
    Variable {
        variable: String,
        /// Optional default value to use if the variable is not available.
        ///
        /// This will be preferred over any default value defined in the workflow variable schema.
        default: Option<ValueRef>,
    },
}

impl BaseRef {
    pub const WORKFLOW_INPUT: Self = Self::Workflow(WorkflowRef::Input);

    pub fn step_output(step: impl Into<String>) -> Self {
        Self::Step { step: step.into() }
    }

    pub fn variable(variable: impl Into<String>, default: Option<ValueRef>) -> Self {
        Self::Variable {
            variable: variable.into(),
            default,
        }
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
#[derive(Debug, Clone, PartialEq, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", untagged)]
pub enum Expr {
    /// # Reference
    /// Reference a value from a step, workflow, or other source.
    #[serde(rename_all = "camelCase")]
    Ref {
        /// The source of the reference.
        #[serde(rename = "$from")]
        from: BaseRef,
        /// JSON path expression to apply to the referenced value.
        ///
        /// Defaults to `$` (the whole referenced value).
        /// May also be a bare field name (without the leading $) if
        /// the referenced value is an object.
        #[serde(default, skip_serializing_if = "JsonPath::is_empty")]
        path: JsonPath,

        #[serde(default, skip_serializing_if = "Option::is_none")]
        on_skip: Option<SkipAction>,
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

    fn new_ref(from: BaseRef, path: impl Into<JsonPath>, on_skip: Option<SkipAction>) -> Self {
        Self::Ref {
            from,
            path: path.into(),
            on_skip,
        }
    }

    pub fn step_path(
        step: impl Into<String>,
        path: impl Into<JsonPath>,
        on_skip: Option<SkipAction>,
    ) -> Self {
        Self::new_ref(BaseRef::step_output(step), path, on_skip)
    }

    pub fn input_path(path: impl Into<JsonPath>, on_skip: Option<SkipAction>) -> Self {
        Self::new_ref(BaseRef::WORKFLOW_INPUT, path, on_skip)
    }

    // Convenience constructors with default skip behavior

    /// Create a step reference
    /// - `step_ref("step1", JsonPath::default())` creates `{"$from": {"step": "step1"}}`
    /// - `step_ref("step1", JsonPath::from("field"))` creates `{"$from": {"step": "step1"}, "path": "field"}`
    pub fn step_ref(step_id: impl Into<String>, path: JsonPath) -> Self {
        Self::Ref {
            from: BaseRef::step_output(step_id),
            path,
            on_skip: None, // Use None to omit the field, falling back to default behavior
        }
    }

    /// Create a workflow input reference
    /// - `workflow_input(JsonPath::default())` creates `{"$from": {"workflow": "input"}}`
    /// - `workflow_input(JsonPath::from("field"))` creates `{"$from": {"workflow": "input"}, "path": "field"}`
    pub fn workflow_input(path: JsonPath) -> Self {
        Self::Ref {
            from: BaseRef::WORKFLOW_INPUT,
            path,
            on_skip: None, // Use None to omit the field, falling back to default behavior
        }
    }

    /// Create a variable reference
    /// - `variable_ref("api_key", JsonPath::default())` creates `{"$from": {"variable": "api_key"}}`
    /// - `variable_ref("config", JsonPath::from("temperature"))` creates `{"$from": {"variable": "config"}, "path": "temperature"}`
    pub fn variable_ref(
        variable: impl Into<String>,
        default: Option<ValueRef>,
        path: JsonPath,
    ) -> Self {
        Self::Ref {
            from: BaseRef::variable(variable, default),
            path,
            on_skip: None, // Use None to omit the field, falling back to default behavior
        }
    }

    pub fn base_ref(&self) -> Option<&BaseRef> {
        match self {
            Self::EscapedLiteral { .. } => None,
            Self::Literal(_) => None,
            Self::Ref { from, .. } => Some(from),
        }
    }

    pub fn path(&self) -> Option<&JsonPath> {
        match self {
            Self::EscapedLiteral { .. } => None,
            Self::Literal(_) => None,
            Self::Ref { path, .. } => {
                if path.is_empty() {
                    None
                } else {
                    Some(path)
                }
            }
        }
    }

    pub fn on_skip(&self) -> Option<&SkipAction> {
        match self {
            Self::EscapedLiteral { .. } => None,
            Self::Literal(_) => None,
            Self::Ref { on_skip, .. } => on_skip.as_ref(),
        }
    }

    /// Get the effective skip action, applying the default if none is specified.
    pub fn on_skip_or_default(&self) -> SkipAction {
        self.on_skip().cloned().unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", tag = "action")]
#[derive(Default)]
pub enum SkipAction {
    /// # OnSkipSkip
    #[default]
    Skip,
    #[serde(rename_all = "camelCase")]
    /// # OnSkipDefault
    UseDefault {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default_value: Option<ValueRef>,
    },
}

impl SkipAction {
    pub fn is_default(&self) -> bool {
        matches!(self, Self::Skip)
    }

    pub fn is_optional(&self) -> bool {
        matches!(self, Self::UseDefault { .. })
    }
}

// Custom serializer for Expr to maintain untagged format
impl serde::Serialize for Expr {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Ref {
                from,
                path,
                on_skip,
            } => {
                use serde::ser::SerializeStruct as _;
                let mut state = serializer.serialize_struct("Expr", 3)?;
                state.serialize_field("$from", from)?;
                if !path.is_empty() {
                    state.serialize_field("path", path)?;
                }
                if let Some(on_skip_value) = on_skip {
                    state.serialize_field("onSkip", on_skip_value)?;
                }
                state.end()
            }
            Self::EscapedLiteral { literal } => {
                use serde::ser::SerializeStruct as _;
                let mut state = serializer.serialize_struct("Expr", 1)?;
                state.serialize_field("$literal", literal)?;
                state.end()
            }
            Self::Literal(value) => {
                // Serialize literal values directly (untagged behavior)
                value.serialize(serializer)
            }
        }
    }
}

// Custom deserializer for Expr
impl<'de> serde::Deserialize<'de> for Expr {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error as _;

        let value = serde_json::Value::deserialize(deserializer)?;

        // Check if this is an object with special keys
        if let serde_json::Value::Object(ref obj) = value {
            if obj.contains_key("$from") {
                // This should be a reference - validate the $from value
                if let Some(from_value) = obj.get("$from") {
                    // $from must be an object, not a string
                    if !from_value.is_object() {
                        return Err(D::Error::custom(format!(
                            "invalid type: string \"{}\", expected a map for $from",
                            from_value.as_str().unwrap_or("<unknown>")
                        )));
                    }
                }

                // Try to deserialize as a reference
                return serde_json::from_value::<ExprRef>(value)
                    .map(|expr_ref| Self::Ref {
                        from: expr_ref.from,
                        path: expr_ref.path,
                        on_skip: expr_ref.on_skip,
                    })
                    .map_err(D::Error::custom);
            } else if obj.contains_key("$literal") {
                // This should be an escaped literal
                return serde_json::from_value::<ExprEscapedLiteral>(value)
                    .map(|expr_lit| Self::EscapedLiteral {
                        literal: expr_lit.literal,
                    })
                    .map_err(D::Error::custom);
            }
        }

        // Otherwise, it's a literal value
        let value_ref = serde_json::from_value::<ValueRef>(value).map_err(D::Error::custom)?;
        Ok(Self::Literal(value_ref))
    }
}

// Helper structs for deserialization
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExprRef {
    #[serde(rename = "$from")]
    from: BaseRef,
    #[serde(default)]
    path: JsonPath,
    #[serde(default)]
    on_skip: Option<SkipAction>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExprEscapedLiteral {
    #[serde(rename = "$literal")]
    literal: ValueRef,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expr_to_yaml() {
        insta::assert_yaml_snapshot!(&Expr::literal("foo"), @r#"foo"#);
        insta::assert_yaml_snapshot!(&Expr::literal(5), @r#"5"#);

        // Input reference with and without path, with default skip action (skip).
        insta::assert_yaml_snapshot!(&Expr::input_path("", None),
            @r###"
        $from:
          workflow: input
        "###);
        insta::assert_yaml_snapshot!(&Expr::input_path("out", None),
            @r###"
        $from:
          workflow: input
        path: $.out
        "###);

        // We don't test input references with skip actions, since they don't make sense.
        // In fact, we should have a validation that ensures they aren't set.

        // Step reference with and without path, with default skip action (skip).
        insta::assert_yaml_snapshot!(&Expr::step_path("step1", "", None),
            @r###"
        $from:
          step: step1
        "###);
        insta::assert_yaml_snapshot!(&Expr::step_path("step1", "out", None),
            @r###"
        $from:
          step: step1
        path: $.out
        "###);

        // Step reference with and without path, with use_default skip action (use default).
        insta::assert_yaml_snapshot!(&Expr::step_path("step1", "", Some(SkipAction::UseDefault { default_value: None })),
            @r###"
        $from:
          step: step1
        onSkip:
          action: useDefault
        "###);
        insta::assert_yaml_snapshot!(&Expr::step_path("step1", "out", Some(SkipAction::UseDefault { default_value: None })),
            @r###"
        $from:
          step: step1
        path: $.out
        onSkip:
          action: useDefault
        "###);

        // Step reference with and without path, with use_default skip action (and default vaule).
        let value: ValueRef = serde_json::Value::String("test_default".to_owned()).into();
        insta::assert_yaml_snapshot!(&Expr::step_path("step1", "out", Some(SkipAction::UseDefault { default_value: Some(value.clone()) })),
            @r###"
        $from:
          step: step1
        path: $.out
        onSkip:
          action: useDefault
          defaultValue: test_default
        "###);
        insta::assert_yaml_snapshot!(&Expr::step_path("step1", "", Some(SkipAction::UseDefault { default_value: Some(value) })),
            @r###"
        $from:
          step: step1
        onSkip:
          action: useDefault
          defaultValue: test_default
        "###);
    }

    #[test]
    fn test_on_skip_null() {
        let from_yaml = |s| serde_yaml_ng::from_str::<Expr>(s).unwrap();

        assert_eq!(
            from_yaml("$from:\n workflow: input\nonSkip: null"),
            Expr::input_path("", None)
        );
    }

    #[test]
    fn test_expr_from_yaml() {
        let from_yaml = |s| serde_yaml_ng::from_str::<Expr>(s).unwrap();
        assert_eq!(from_yaml("foo"), Expr::literal("foo"));
        assert_eq!(from_yaml("5"), Expr::literal(5));

        assert_eq!(
            from_yaml("{ $from: { step: \"step1\" } }"),
            Expr::step_path("step1", "", None)
        );
        assert_eq!(
            from_yaml("{ $from: { step: \"step1\" }, path: \"out\" }"),
            Expr::step_path("step1", "out", None)
        );
    }

    #[test]
    fn test_expr_from_yaml_invalid_reference() {
        let from_yaml = |s| serde_yaml_ng::from_str::<Expr>(s).unwrap_err();
        assert_eq!(
            from_yaml("{ $from: \"input\" }").to_string(),
            "invalid type: string \"input\", expected a map for $from",
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
                Some(SkipAction::UseDefault {
                    default_value: Some(ValueRef::from("fallback"))
                })
            )
        );

        let expr_with_default_skip: Expr =
            serde_yaml_ng::from_str("$from: { step: step1 }\npath: out").unwrap();

        assert_eq!(
            expr_with_default_skip,
            Expr::step_path("step1", "out", None)
        );
    }
}
