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

use super::{Component, Expr, ValueTemplate};
use crate::schema::SchemaRef;
use schemars::JsonSchema;

/// A step in a workflow that executes a component with specific arguments.
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    /// Identifier for the step
    pub id: String,

    /// The component to execute in this step
    pub component: Component,

    /// The input schema for this step.
    pub input_schema: Option<SchemaRef>,

    /// The output schema for this step.
    pub output_schema: Option<SchemaRef>,

    /// If set and the referenced value is truthy, this step will be skipped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_if: Option<Expr>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_error: Option<ErrorAction>,

    /// Arguments to pass to the component for this step
    #[serde(default, skip_serializing_if = "ValueTemplate::is_null")]
    pub input: ValueTemplate,

    /// If true, this step must execute even if its output is not used by the workflow output.
    /// Useful for steps with side effects (e.g., writing to databases, sending notifications).
    ///
    /// Note: If the step has `skip_if` that evaluates to true, the step will still be skipped
    /// and its dependencies will not be forced to execute.
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

#[derive(
    Clone, serde::Serialize, serde::Deserialize, Debug, PartialEq, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "camelCase", tag = "action")]
#[derive(Default)]
pub enum ErrorAction {
    /// # OnErrorFail
    /// If the step fails, the flow will fail.
    #[default]
    Fail,
    /// # OnErrorSkip
    /// If the step fails, mark it as skipped. This allows down-stream steps to handle the skipped step.
    Skip,
    /// # OnErrorDefault
    /// If the step fails, use the `defaultValue` instead.
    #[serde(rename_all = "camelCase")]
    UseDefault {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default_value: Option<ValueTemplate>,
    },
    /// # OnErrorRetry
    /// If the step fails, retry it.
    Retry,
}

impl ErrorAction {
    pub fn is_default(&self) -> bool {
        matches!(self, Self::Fail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::{StepBuilder, ValueRef};

    #[test]
    fn test_error_action_serialization() {
        let fail = ErrorAction::Fail;
        assert_eq!(serde_yaml_ng::to_string(&fail).unwrap(), "action: fail\n");

        let skip = ErrorAction::Skip;
        assert_eq!(serde_yaml_ng::to_string(&skip).unwrap(), "action: skip\n");

        let retry = ErrorAction::Retry;
        assert_eq!(serde_yaml_ng::to_string(&retry).unwrap(), "action: retry\n");

        let use_default = ErrorAction::UseDefault {
            default_value: Some(ValueRef::from("test_default").into()),
        };
        assert_eq!(
            serde_yaml_ng::to_string(&use_default).unwrap(),
            "action: useDefault\ndefaultValue: test_default\n"
        );
    }

    #[test]
    fn test_error_action_deserialization() {
        let fail: ErrorAction = serde_yaml_ng::from_str("action: fail").unwrap();
        assert_eq!(fail, ErrorAction::Fail);

        let skip: ErrorAction = serde_yaml_ng::from_str("action: skip").unwrap();
        assert_eq!(skip, ErrorAction::Skip);

        let retry: ErrorAction = serde_yaml_ng::from_str("action: retry").unwrap();
        assert_eq!(retry, ErrorAction::Retry);

        let use_default: ErrorAction =
            serde_yaml_ng::from_str("action: useDefault\ndefaultValue: test_default").unwrap();
        assert_eq!(
            use_default,
            ErrorAction::UseDefault {
                default_value: Some(ValueRef::from("test_default").into())
            }
        );
    }

    #[test]
    fn test_error_action_default() {
        assert_eq!(ErrorAction::default(), ErrorAction::Fail);
        assert!(ErrorAction::Fail.is_default());
        assert!(!ErrorAction::Skip.is_default());
        assert!(!ErrorAction::Retry.is_default());
        assert!(
            !ErrorAction::UseDefault {
                default_value: Some(ValueRef::from("test").into())
            }
            .is_default()
        );
    }

    #[test]
    fn test_step_serialization_with_error_action() {
        let step = StepBuilder::new("test_step")
            .component("/mock/test_component")
            .on_error(ErrorAction::UseDefault {
                default_value: Some(ValueRef::from("fallback").into()),
            })
            .input(ValueTemplate::null())
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
            .input(ValueTemplate::null())
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
