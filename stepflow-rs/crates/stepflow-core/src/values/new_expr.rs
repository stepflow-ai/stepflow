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
use serde_json::Value;

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
        default: Option<Value>,
    },
}

impl BaseRef {
    pub const WORKFLOW_INPUT: Self = Self::Workflow(WorkflowRef::Input);

    pub fn step_output(step: impl Into<String>) -> Self {
        Self::Step { step: step.into() }
    }

    pub fn variable(variable: impl Into<String>, default: Option<Value>) -> Self {
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
#[derive(Debug, Clone, PartialEq, JsonSchema, utoipa::ToSchema, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Expr {
    /// The source of the reference.
    #[serde(rename = "$from")]
    from: BaseRef,
    // /// JSON path expression to apply to the referenced value.
    // ///
    // /// Defaults to `$` (the whole referenced value).
    // /// May also be a bare field name (without the leading $) if
    // /// the referenced value is an object.
    // #[serde(default, skip_serializing_if = "JsonPath::is_empty")]
    // path: JsonPath,
}

impl Expr {
    pub fn step_output(step: impl Into<String>) -> Self {
        Self {
            from: BaseRef::step_output(step),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_tests() {
        panic!("Write tests")
    }
}
