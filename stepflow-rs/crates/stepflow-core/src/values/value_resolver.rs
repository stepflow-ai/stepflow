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

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use error_stack::ResultExt as _;
use uuid::Uuid;

use tracing;

use super::{ValueRef, ValueTemplate, ValueTemplateRepr};
use crate::{
    FlowResult,
    workflow::{BaseRef, Expr, Flow, SkipAction, StepId},
};

/// Trait for loading values from external sources (like state stores).
///
/// This abstraction allows the value resolver to load step results and workflow
/// inputs without being tightly coupled to specific storage implementations.
#[async_trait]
pub trait ValueLoader: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Load the result of a completed step by its index.
    async fn load_step_result(
        &self,
        run_id: Uuid,
        step_index: usize,
    ) -> std::result::Result<FlowResult, Self::Error>;

    /// Load the input value for the workflow.
    async fn load_workflow_input(&self, run_id: Uuid)
    -> std::result::Result<ValueRef, Self::Error>;
}

/// Errors that can occur during value resolution
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ValueResolverError {
    #[error("Undefined value reference: {0:?}")]
    UndefinedValue(BaseRef),
    #[error("Undefined field '{field}' in value")]
    UndefinedField { field: String, value: ValueRef },
    #[error("Internal error")]
    Internal,
    #[error("State error")]
    StateError,
    #[error("Value loader error: {0}")]
    LoaderError(String),
}

pub type ValueResolverResult<T> = error_stack::Result<T, ValueResolverError>;

/// Value resolver for handling expression and JSON value resolution
#[derive(Clone)]
pub struct ValueResolver<L: ValueLoader> {
    /// Execution ID of the workflow we are resolving for.
    ///
    /// This is used to scope the state store interactions.
    run_id: Uuid,
    /// Input value for the workflow.
    ///
    /// This is used to resolve references to the workflow input.
    input: ValueRef,
    /// Value loader to use for resolving values.
    ///
    /// This is used to load step results and workflow input.
    loader: L,
    /// Map from step ID to step index for cache lookups.
    step_id_to_index: HashMap<String, usize>,
    flow: Arc<Flow>,
}

impl<L: ValueLoader> ValueResolver<L> {
    pub fn new(run_id: Uuid, input: ValueRef, loader: L, flow: Arc<Flow>) -> Self {
        let step_id_to_index = flow
            .steps
            .iter()
            .enumerate()
            .map(|(index, step)| (step.id.clone(), index))
            .collect();
        Self {
            run_id,
            input,
            loader,
            step_id_to_index,
            flow,
        }
    }

    /// Resolve a ValueTemplate, returning a FlowResult.
    pub async fn resolve(&self, template: &ValueTemplate) -> ValueResolverResult<FlowResult> {
        self.resolve_template_rec(template).await
    }

    /// Retrieve the StepId (index and ID) for a given step_id string.
    fn get_step_id(&self, step_id: &str) -> ValueResolverResult<StepId> {
        // TODO: Ideally, the step index would be put into the workflow during
        // analysis, so this wouldn't need to be looked up.
        if let Some(&step_index) = self.step_id_to_index.get(step_id) {
            Ok(StepId {
                index: step_index,
                flow: self.flow.clone(),
            })
        } else {
            Err(ValueResolverError::UndefinedValue(BaseRef::Step {
                step: step_id.to_string(),
            })
            .into())
        }
    }

    pub async fn resolve_step(&self, step: &str) -> ValueResolverResult<FlowResult> {
        let step_id = self.get_step_id(step)?;
        self.loader
            .load_step_result(self.run_id, step_id.index)
            .await
            .change_context(ValueResolverError::StateError)
    }

    /// Resolve an expression, returning a FlowResult.
    pub async fn resolve_expr(&self, expr: &Expr) -> ValueResolverResult<FlowResult> {
        // Handle literal expressions
        if let Expr::EscapedLiteral { literal } = expr {
            return Ok(FlowResult::Success {
                result: literal.clone(),
            });
        } else if let Expr::Literal(literal) = expr {
            return Ok(FlowResult::Success {
                result: literal.clone(),
            });
        }

        // Get the base reference
        let base_ref = expr.base_ref().ok_or(ValueResolverError::Internal)?;

        let base_result = match base_ref {
            BaseRef::Workflow(_) => {
                // Return the workflow input
                FlowResult::Success {
                    result: self.input.clone(),
                }
            }
            BaseRef::Step { step: step_id } => self.resolve_step(step_id).await?,
        };

        // Apply path if specified
        let path_result = if let Some(path) = expr.path() {
            match base_result {
                FlowResult::Success { result } => {
                    tracing::debug!("Resolving path '{}' on value: {:?}", path, result.as_ref());
                    if let Some(sub_value) = result.resolve_json_path(path) {
                        tracing::debug!("Path '{}' resolved to: {:?}", path, sub_value.as_ref());
                        FlowResult::Success { result: sub_value }
                    } else {
                        tracing::debug!("Path '{}' not found in value", path);
                        return Err(ValueResolverError::UndefinedField {
                            field: path.to_string(),
                            value: result,
                        }
                        .into());
                    }
                }
                FlowResult::Skipped => FlowResult::Skipped,
                other => other,
            }
        } else {
            base_result
        };

        // Handle skip actions.
        // NOTE: Skip actions are applied after path resolution.
        match path_result {
            FlowResult::Success { result } => Ok(FlowResult::Success { result }),
            FlowResult::Skipped => {
                match expr.on_skip() {
                    Some(SkipAction::UseDefault { default_value }) => {
                        let default = default_value
                            .as_ref()
                            .map(|v| v.as_ref())
                            .unwrap_or(&serde_json::Value::Null);
                        Ok(FlowResult::Success {
                            result: ValueRef::new(default.clone()),
                        })
                    }
                    _ => {
                        // No on_skip action specified - propagate the skip
                        Ok(FlowResult::Skipped)
                    }
                }
            }
            FlowResult::Failed { error } => Ok(FlowResult::Failed { error }),
        }
    }

    /// Recursive resolution of ValueTemplate structures, returning FlowResult.
    /// This is the new clean implementation that works with pre-parsed templates.
    async fn resolve_template_rec(
        &self,
        template: &ValueTemplate,
    ) -> ValueResolverResult<FlowResult> {
        match template.as_ref() {
            ValueTemplateRepr::Expression(expr) => {
                // Resolve the expression directly
                self.resolve_expr(expr).await
            }
            ValueTemplateRepr::Null => Ok(FlowResult::Success {
                result: ValueRef::new(serde_json::Value::Null),
            }),
            ValueTemplateRepr::Bool(b) => Ok(FlowResult::Success {
                result: ValueRef::new(serde_json::Value::Bool(*b)),
            }),
            ValueTemplateRepr::Number(n) => Ok(FlowResult::Success {
                result: ValueRef::new(serde_json::Value::Number(n.clone())),
            }),
            ValueTemplateRepr::String(s) => Ok(FlowResult::Success {
                result: ValueRef::new(serde_json::Value::String(s.clone())),
            }),
            ValueTemplateRepr::Array(arr) => {
                // Process array recursively
                let mut result_array = Vec::new();
                for template in arr {
                    match Box::pin(self.resolve_template_rec(template)).await? {
                        FlowResult::Success { result } => {
                            result_array.push(result.as_ref().clone());
                        }
                        FlowResult::Skipped => {
                            return Ok(FlowResult::Skipped);
                        }
                        FlowResult::Failed { error } => {
                            return Ok(FlowResult::Failed { error });
                        }
                    }
                }
                Ok(FlowResult::Success {
                    result: ValueRef::new(serde_json::Value::Array(result_array)),
                })
            }
            ValueTemplateRepr::Object(obj) => {
                // Process object recursively
                let mut result_map = serde_json::Map::new();
                for (k, template) in obj {
                    match Box::pin(self.resolve_template_rec(template)).await? {
                        FlowResult::Success { result } => {
                            result_map.insert(k.clone(), result.as_ref().clone());
                        }
                        FlowResult::Skipped => {
                            return Ok(FlowResult::Skipped);
                        }
                        FlowResult::Failed { error } => {
                            return Ok(FlowResult::Failed { error });
                        }
                    }
                }
                Ok(FlowResult::Success {
                    result: ValueRef::new(serde_json::Value::Object(result_map)),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::{Component, ErrorAction, JsonPath, Step};
    use async_trait::async_trait;
    use serde_json::json;

    // Mock implementation of ValueLoader for testing
    struct MockValueLoader {
        workflow_input: ValueRef,
        step_results: HashMap<usize, FlowResult>,
    }

    impl MockValueLoader {
        fn new(workflow_input: ValueRef) -> Self {
            Self {
                workflow_input,
                step_results: HashMap::new(),
            }
        }

        fn with_step_result(mut self, step_index: usize, result: FlowResult) -> Self {
            self.step_results.insert(step_index, result);
            self
        }
    }

    #[derive(Debug, thiserror::Error)]
    enum MockError {
        #[error("{0}")]
        Generic(String),
    }

    #[async_trait]
    impl ValueLoader for MockValueLoader {
        type Error = MockError;

        async fn load_step_result(
            &self,
            _run_id: Uuid,
            step_index: usize,
        ) -> std::result::Result<FlowResult, Self::Error> {
            self.step_results.get(&step_index).cloned().ok_or_else(|| {
                MockError::Generic(format!("Step result not found for index {step_index}"))
            })
        }

        async fn load_workflow_input(
            &self,
            _run_id: Uuid,
        ) -> std::result::Result<ValueRef, Self::Error> {
            Ok(self.workflow_input.clone())
        }
    }

    fn create_test_flow() -> Arc<Flow> {
        Arc::new(Flow {
            name: None,
            description: None,
            version: None,
            input_schema: None,
            output_schema: None,
            steps: vec![Step {
                id: "step1".to_string(),
                component: Component::from_string("mock://test"),
                input: ValueTemplate::literal(json!({})),
                input_schema: None,
                output_schema: None,
                skip_if: None,
                on_error: ErrorAction::Fail,
            }],
            output: ValueTemplate::literal(json!(null)),
            test: None,
            examples: vec![],
        })
    }

    #[tokio::test]
    async fn test_resolve_template() {
        let workflow_input = ValueRef::new(json!({"name": "Alice"}));
        let loader = MockValueLoader::new(workflow_input.clone());
        let run_id = Uuid::new_v4();
        let flow = create_test_flow();

        let resolver = ValueResolver::new(run_id, workflow_input, loader, flow);

        // Test resolving ValueTemplate - create a template with an expression by deserializing from JSON
        let template = ValueTemplate::workflow_input(JsonPath::from("name"));
        let resolved = resolver.resolve(&template).await.unwrap();
        match resolved {
            FlowResult::Success { result } => {
                assert_eq!(result.as_ref(), &json!("Alice"));
            }
            _ => panic!("Expected successful result, got: {resolved:?}"),
        }
    }
}
