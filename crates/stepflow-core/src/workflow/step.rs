use super::{Component, Expr, ValueRef};
use crate::schema::SchemaRef;
use indexmap::IndexMap;
use schemars::JsonSchema;

/// A step in a workflow that executes a component with specific arguments.
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, JsonSchema)]
pub struct Step {
    /// Optional identifier for the step
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

    #[serde(default, skip_serializing_if = "ErrorAction::is_default")]
    pub on_error: ErrorAction,

    /// Arguments to pass to the component for this step
    pub args: IndexMap<String, Expr>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum ErrorAction {
    /// If the step fails, the flow will fail.
    Fail,
    /// If the step fails, mark it as skipped. This allows down-stream steps to handle the skipped step.
    Skip,
    /// If the step fails, use the `default_value` instead.
    UseDefault {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default_value: Option<ValueRef>,
    },
    /// If the step fails, retry it.
    Retry,
}

impl ErrorAction {
    pub fn is_default(&self) -> bool {
        matches!(self, Self::Fail)
    }
}

impl Default for ErrorAction {
    fn default() -> Self {
        Self::Fail
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::Component;
    use crate::workflow::ValueRef;
    use indexmap::IndexMap;

    #[test]
    fn test_error_action_serialization() {
        let fail = ErrorAction::Fail;
        assert_eq!(serde_yml::to_string(&fail).unwrap(), "action: fail\n");

        let skip = ErrorAction::Skip;
        assert_eq!(serde_yml::to_string(&skip).unwrap(), "action: skip\n");

        let retry = ErrorAction::Retry;
        assert_eq!(serde_yml::to_string(&retry).unwrap(), "action: retry\n");

        let use_default = ErrorAction::UseDefault {
            default_value: Some(ValueRef::from("test_default")),
        };
        assert_eq!(
            serde_yml::to_string(&use_default).unwrap(),
            "action: use_default\ndefault_value: test_default\n"
        );
    }

    #[test]
    fn test_error_action_deserialization() {
        let fail: ErrorAction = serde_yml::from_str("action: fail").unwrap();
        assert_eq!(fail, ErrorAction::Fail);

        let skip: ErrorAction = serde_yml::from_str("action: skip").unwrap();
        assert_eq!(skip, ErrorAction::Skip);

        let retry: ErrorAction = serde_yml::from_str("action: retry").unwrap();
        assert_eq!(retry, ErrorAction::Retry);

        let use_default: ErrorAction =
            serde_yml::from_str("action: use_default\ndefault_value: test_default").unwrap();
        assert_eq!(
            use_default,
            ErrorAction::UseDefault {
                default_value: Some(ValueRef::from("test_default"))
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
                default_value: Some(ValueRef::from("test"))
            }
            .is_default()
        );
    }

    #[test]
    fn test_step_serialization_with_error_action() {
        let step = Step {
            id: "test_step".to_string(),
            component: Component::parse("mock://test_component").unwrap(),
            input_schema: None,
            output_schema: None,
            skip_if: None,
            on_error: ErrorAction::UseDefault {
                default_value: Some(ValueRef::from("fallback")),
            },
            args: IndexMap::new(),
        };

        let yaml = serde_yml::to_string(&step).unwrap();
        assert!(yaml.contains("on_error:"));
        assert!(yaml.contains("action: use_default"));
        assert!(yaml.contains("default_value: fallback"));
    }

    #[test]
    fn test_step_default_error_action_not_serialized() {
        let step = Step {
            id: "test_step".to_string(),
            component: Component::parse("mock://test_component").unwrap(),
            input_schema: None,
            output_schema: None,
            skip_if: None,
            on_error: ErrorAction::Fail,
            args: IndexMap::new(),
        };

        let yaml = serde_yml::to_string(&step).unwrap();
        assert!(!yaml.contains("on_error:"));
    }
}
