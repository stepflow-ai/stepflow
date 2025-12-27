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

use std::collections::HashMap;

use super::Component;
use crate::ValueExpr;

/// A step in a workflow that executes a component with specific arguments.
///
/// Note: Step output schemas are stored in the flow's `types.steps` field,
/// not on individual steps. This allows for shared `$defs` and avoids duplication.
#[derive(Clone, serde::Serialize, serde::Deserialize, Debug, PartialEq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    /// Identifier for the step
    pub id: String,

    /// The component to execute in this step
    pub component: Component,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_error: Option<ErrorAction>,

    /// Arguments to pass to the component for this step
    #[serde(default, skip_serializing_if = "ValueExpr::is_null")]
    pub input: ValueExpr,

    /// If true, this step must execute even if its output is not used by the workflow output.
    /// Useful for steps with side effects (e.g., writing to databases, sending notifications).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub must_execute: Option<bool>,

    /// Extensible metadata for the step that can be used by tools and frameworks.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Step {
    pub fn on_error(&self) -> Option<&ErrorAction> {
        self.on_error.as_ref()
    }

    /// Get the effective error action, applying the default if none is specified.
    pub fn on_error_or_default(&self) -> ErrorAction {
        self.on_error().cloned().unwrap_or_default()
    }

    /// Check if this step must execute, treating None as false (the default).
    pub fn must_execute(&self) -> bool {
        self.must_execute.unwrap_or(false)
    }
}

/// Error action determines what happens when a step fails.
#[derive(Clone, Debug, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum ErrorAction {
    /// If the step fails, the flow will fail.
    #[default]
    Fail,
    /// If the step fails, use the `defaultValue` instead.
    /// If `defaultValue` is not specified, the step returns null.
    /// The default value must be a literal JSON value (not an expression).
    /// For dynamic defaults, use `$coalesce` in the consuming expression instead.
    #[serde(rename_all = "camelCase")]
    UseDefault {
        #[serde(skip_serializing_if = "Option::is_none")]
        default_value: Option<serde_json::Value>,
    },
    /// If the step fails, retry it.
    Retry,
}

/// Helper to create a variant schema with action property
fn error_action_variant_schema(
    title: &str,
    description: &str,
    extra_properties: Option<(&str, utoipa::openapi::schema::Schema)>,
) -> utoipa::openapi::schema::Schema {
    use utoipa::openapi::schema::*;

    // The action property is defined as a plain string without const/enum constraint.
    // The discriminator mapping provides the constraint at the oneOf level.
    // This approach works with datamodel-code-generator without requiring post-processing.
    let mut builder = ObjectBuilder::new()
        .schema_type(SchemaType::Type(Type::Object))
        .title(Some(title))
        .description(Some(description))
        .property(
            "action",
            ObjectBuilder::new().schema_type(SchemaType::Type(Type::String)),
        )
        .required("action");

    if let Some((name, prop_schema)) = extra_properties {
        builder = builder.property(name, prop_schema);
    }

    Schema::Object(builder.build())
}

impl utoipa::PartialSchema for ErrorAction {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::*;

        // Create discriminator with mapping to $defs
        let mut discriminator = Discriminator::new("action");
        discriminator.mapping = [
            (
                "fail".to_string(),
                "#/components/schemas/OnErrorFail".to_string(),
            ),
            (
                "useDefault".to_string(),
                "#/components/schemas/OnErrorDefault".to_string(),
            ),
            (
                "retry".to_string(),
                "#/components/schemas/OnErrorRetry".to_string(),
            ),
        ]
        .into_iter()
        .collect();

        // Use $refs to the variants defined in $defs
        RefOr::T(schema::Schema::OneOf(
            schema::OneOfBuilder::new()
                .item(RefOr::Ref(Ref::new("#/components/schemas/OnErrorFail")))
                .item(RefOr::Ref(Ref::new("#/components/schemas/OnErrorDefault")))
                .item(RefOr::Ref(Ref::new("#/components/schemas/OnErrorRetry")))
                .description(Some(
                    "Error action determines what happens when a step fails.",
                ))
                .discriminator(Some(discriminator))
                .build(),
        ))
    }
}

impl utoipa::ToSchema for ErrorAction {
    fn schemas(
        schemas: &mut Vec<(
            String,
            utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
        )>,
    ) {
        use utoipa::openapi::schema::*;
        use utoipa::openapi::*;

        // Register variant schemas in $defs
        let fail = error_action_variant_schema(
            "OnErrorFail",
            "If the step fails, the flow will fail.",
            None,
        );
        schemas.push(("OnErrorFail".to_string(), RefOr::T(fail)));

        // defaultValue can be any JSON value (true in JSON Schema means "any")
        let any_value_schema = Schema::AllOf(AllOfBuilder::new().build());
        let use_default = error_action_variant_schema(
            "OnErrorDefault",
            "If the step fails, use the `defaultValue` instead.\n\
             If `defaultValue` is not specified, the step returns null.\n\
             The default value must be a literal JSON value (not an expression).\n\
             For dynamic defaults, use `$coalesce` in the consuming expression instead.",
            Some(("defaultValue", any_value_schema)),
        );
        schemas.push(("OnErrorDefault".to_string(), RefOr::T(use_default)));

        let retry =
            error_action_variant_schema("OnErrorRetry", "If the step fails, retry it.", None);
        schemas.push(("OnErrorRetry".to_string(), RefOr::T(retry)));
    }
}

impl ErrorAction {
    pub fn is_default(&self) -> bool {
        matches!(self, Self::Fail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::StepBuilder;

    #[test]
    fn test_error_action_serialization() {
        let fail = ErrorAction::Fail;
        assert_eq!(serde_yaml_ng::to_string(&fail).unwrap(), "action: fail\n");

        let retry = ErrorAction::Retry;
        assert_eq!(serde_yaml_ng::to_string(&retry).unwrap(), "action: retry\n");

        let use_default = ErrorAction::UseDefault {
            default_value: Some(serde_json::json!("test_default")),
        };
        assert_eq!(
            serde_yaml_ng::to_string(&use_default).unwrap(),
            "action: useDefault\ndefaultValue: test_default\n"
        );

        // UseDefault with no value serializes without defaultValue field
        let use_default_none = ErrorAction::UseDefault {
            default_value: None,
        };
        assert_eq!(
            serde_yaml_ng::to_string(&use_default_none).unwrap(),
            "action: useDefault\n"
        );
    }

    #[test]
    fn test_error_action_deserialization() {
        let fail: ErrorAction = serde_yaml_ng::from_str("action: fail").unwrap();
        assert_eq!(fail, ErrorAction::Fail);

        let retry: ErrorAction = serde_yaml_ng::from_str("action: retry").unwrap();
        assert_eq!(retry, ErrorAction::Retry);

        let use_default: ErrorAction =
            serde_yaml_ng::from_str("action: useDefault\ndefaultValue: test_default").unwrap();
        assert_eq!(
            use_default,
            ErrorAction::UseDefault {
                default_value: Some(serde_json::json!("test_default"))
            }
        );
    }

    #[test]
    fn test_error_action_default() {
        assert_eq!(ErrorAction::default(), ErrorAction::Fail);
        assert!(ErrorAction::Fail.is_default());
        assert!(!ErrorAction::Retry.is_default());
        assert!(
            !ErrorAction::UseDefault {
                default_value: Some(serde_json::json!("test"))
            }
            .is_default()
        );
    }

    #[test]
    fn test_step_serialization_with_error_action() {
        let step = StepBuilder::new("test_step")
            .component("/mock/test_component")
            .on_error(ErrorAction::UseDefault {
                default_value: Some(serde_json::json!("fallback")),
            })
            .input(ValueExpr::null())
            .build();

        let yaml = serde_yaml_ng::to_string(&step).unwrap();
        assert!(yaml.contains("onError:"));
        assert!(yaml.contains("action: useDefault"));
        assert!(yaml.contains("defaultValue: fallback"));
    }

    #[test]
    fn test_step_default_error_action_not_serialized() {
        let step = StepBuilder::new("test_step")
            .component("/mock/test_component")
            .input(ValueExpr::null())
            .build();

        let yaml = serde_yaml_ng::to_string(&step).unwrap();
        assert!(!yaml.contains("onError:"));
    }

    #[test]
    fn test_on_error_null() {
        let yaml_with_null = r#"
id: test_step
component: /mock/test_component
onError: null
input: {}
metadata: {}
        "#;

        let step: Step = serde_yaml_ng::from_str(yaml_with_null.trim()).unwrap();
        assert_eq!(step.on_error, None);
        assert_eq!(step.on_error_or_default(), ErrorAction::Fail);
    }
}
