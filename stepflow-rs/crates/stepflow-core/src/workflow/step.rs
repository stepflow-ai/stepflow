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

    #[serde(default, skip_serializing_if = "ErrorAction::is_default")]
    pub on_error: ErrorAction,

    /// Arguments to pass to the component for this step
    #[serde(default, skip_serializing_if = "ValueTemplate::is_null")]
    pub input: ValueTemplate,

    /// Extensible metadata for the step that can be used by tools and frameworks.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(
    Clone, serde::Serialize, serde::Deserialize, Debug, PartialEq, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "camelCase", tag = "action")]
pub enum ErrorAction {
    /// # OnErrorFail
    /// If the step fails, the flow will fail.
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
        let step = Step {
            id: "test_step".to_string(),
            component: Component::from_string("/mock/test_component"),
            input_schema: None,
            output_schema: None,
            skip_if: None,
            on_error: ErrorAction::UseDefault {
                default_value: Some(ValueRef::from("fallback").into()),
            },
            input: ValueTemplate::null(),
            metadata: HashMap::default(),
        };

        let yaml = serde_yaml_ng::to_string(&step).unwrap();
        assert!(yaml.contains("onError:"));
        assert!(yaml.contains("action: useDefault"));
        assert!(yaml.contains("defaultValue: fallback"));
    }

    #[test]
    fn test_step_default_error_action_not_serialized() {
        let step = Step {
            id: "test_step".to_string(),
            component: Component::from_string("/mock/test_component"),
            input_schema: None,
            output_schema: None,
            skip_if: None,
            on_error: ErrorAction::Fail,
            input: ValueTemplate::null(),
            metadata: HashMap::default(),
        };

        let yaml = serde_yaml_ng::to_string(&step).unwrap();
        assert!(!yaml.contains("on_error:"));
    }
}
