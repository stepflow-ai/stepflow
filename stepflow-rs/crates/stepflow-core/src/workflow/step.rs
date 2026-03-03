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

use serde_with::{DefaultOnNull, serde_as};

use super::Component;
use crate::ValueExpr;

/// A step in a workflow that executes a component with specific arguments.
///
/// Note: Step output schemas are stored in the flow's `types.steps` field,
/// not on individual steps. This allows for shared `$defs` and avoids duplication.
#[serde_as]
#[derive(Clone, serde::Serialize, serde::Deserialize, Debug, PartialEq, schemars::JsonSchema)]
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
    #[serde_as(as = "DefaultOnNull")]
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
#[derive(
    Clone, Debug, PartialEq, Default, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(tag = "action", rename_all = "camelCase")]
#[schemars(transform = crate::discriminator_schema::AddDiscriminator::new("action"))]
pub enum ErrorAction {
    /// If the step fails, the flow will fail.
    #[default]
    #[schemars(title = "OnErrorFail")]
    Fail,
    /// If the step fails, use the `defaultValue` instead.
    /// If `defaultValue` is not specified, the step returns null.
    /// The default value must be a literal JSON value (not an expression).
    /// For dynamic defaults, use `$coalesce` in the consuming expression instead.
    #[serde(rename_all = "camelCase")]
    #[schemars(title = "OnErrorDefault")]
    UseDefault {
        #[serde(skip_serializing_if = "Option::is_none")]
        default_value: Option<serde_json::Value>,
    },
    /// If the step fails, retry it.
    ///
    /// `max_retries` limits retries due to component errors — cases where the
    /// component ran and returned an error. Transport-level failures (subprocess
    /// crashes, network errors) are retried separately according to the plugin's
    /// retry configuration and do not count against this budget.
    #[serde(rename_all = "camelCase")]
    #[schemars(title = "OnErrorRetry")]
    Retry {
        /// Maximum number of retries due to component errors (default: 3).
        ///
        /// Total attempts for component errors = max_retries + 1 (initial).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_retries: Option<u32>,
    },
}

impl ErrorAction {
    /// Default maximum retries for component errors.
    pub const DEFAULT_MAX_RETRIES: u32 = 3;

    pub fn is_default(&self) -> bool {
        matches!(self, Self::Fail)
    }

    /// Get the max retries for component errors, if this is a Retry action.
    pub fn max_retries(&self) -> Option<u32> {
        match self {
            Self::Retry { max_retries } => Some(max_retries.unwrap_or(Self::DEFAULT_MAX_RETRIES)),
            _ => None,
        }
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

        // Retry with default max_retries omits the field
        let retry = ErrorAction::Retry { max_retries: None };
        assert_eq!(serde_yaml_ng::to_string(&retry).unwrap(), "action: retry\n");

        // Retry with explicit max_retries includes the field
        let retry_with_max = ErrorAction::Retry {
            max_retries: Some(5),
        };
        assert_eq!(
            serde_yaml_ng::to_string(&retry_with_max).unwrap(),
            "action: retry\nmaxRetries: 5\n"
        );

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

        // Retry without maxRetries
        let retry: ErrorAction = serde_yaml_ng::from_str("action: retry").unwrap();
        assert_eq!(retry, ErrorAction::Retry { max_retries: None });

        // Retry with maxRetries
        let retry_with_max: ErrorAction =
            serde_yaml_ng::from_str("action: retry\nmaxRetries: 5").unwrap();
        assert_eq!(
            retry_with_max,
            ErrorAction::Retry {
                max_retries: Some(5)
            }
        );

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
        assert!(!ErrorAction::Retry { max_retries: None }.is_default());
        assert!(
            !ErrorAction::UseDefault {
                default_value: Some(serde_json::json!("test"))
            }
            .is_default()
        );
    }

    #[test]
    fn test_error_action_max_retries() {
        assert_eq!(ErrorAction::Fail.max_retries(), None);
        assert_eq!(
            ErrorAction::Retry { max_retries: None }.max_retries(),
            Some(ErrorAction::DEFAULT_MAX_RETRIES)
        );
        assert_eq!(
            ErrorAction::Retry {
                max_retries: Some(5)
            }
            .max_retries(),
            Some(5)
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
    fn test_step_all_optional_null() {
        // All optional/defaulted Step fields as explicit null — simulates a Python
        // client calling model_dump() without exclude_none=True.
        let json = serde_json::json!({
            "id": "test_step",
            "component": "/mock/test_component",
            "onError": null,
            "input": null,
            "mustExecute": null,
            "metadata": null,
        });
        let step: Step = serde_json::from_value(json).unwrap();
        assert_eq!(step.id, "test_step");
        assert!(step.on_error.is_none());
        assert_eq!(step.on_error_or_default(), ErrorAction::Fail);
        assert!(step.input.is_null());
        assert!(step.must_execute.is_none());
        assert!(step.metadata.is_empty());
    }

    #[test]
    fn test_error_action_use_default_null_value() {
        // defaultValue: null means "no default value" (use null as the step output)
        let json = serde_json::json!({"action": "useDefault", "defaultValue": null});
        let action: ErrorAction = serde_json::from_value(json).unwrap();
        assert!(matches!(action, ErrorAction::UseDefault { default_value: None }));
    }

    #[test]
    fn test_error_action_retry_null_max_retries() {
        // maxRetries: null means "use the built-in default"
        let json = serde_json::json!({"action": "retry", "maxRetries": null});
        let action: ErrorAction = serde_json::from_value(json).unwrap();
        assert!(matches!(action, ErrorAction::Retry { max_retries: None }));
    }
}
