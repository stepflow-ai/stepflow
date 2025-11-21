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

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use error_stack::ResultExt as _;
use uuid::Uuid;

use log;

use super::{ValueRef, ValueTemplate, ValueTemplateRepr};
use crate::{
    FlowResult,
    values::Secrets,
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
    #[error("Undefined variable: {0:?}")]
    UndefinedVariable(String),
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
    /// Variable values provided for this workflow execution.
    variables: HashMap<String, ValueRef>,
    flow: Arc<Flow>,
}

impl<L: ValueLoader> ValueResolver<L> {
    pub fn new(run_id: Uuid, input: ValueRef, loader: L, flow: Arc<Flow>) -> Self {
        Self::new_with_variables(run_id, input, loader, flow, HashMap::new())
    }

    pub fn new_with_variables(
        run_id: Uuid,
        input: ValueRef,
        loader: L,
        flow: Arc<Flow>,
        variables: HashMap<String, ValueRef>,
    ) -> Self {
        let step_id_to_index = flow
            .latest()
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
            variables,
            flow,
        }
    }

    /// Validate that all required variables are provided.
    pub fn validate_variables(&self) -> ValueResolverResult<()> {
        if let Some(variables_schema) = self.flow.variables() {
            // Convert ValueRef hashmap to serde_json::Value for validation
            let variable_values: HashMap<String, serde_json::Value> = self
                .variables
                .iter()
                .map(|(k, v)| (k.clone(), v.as_ref().clone()))
                .collect();

            variables_schema
                .validate_variables(&variable_values)
                .map_err(|_| ValueResolverError::Internal)?;
        }
        Ok(())
    }

    /// Expand a ValueTemplate, returning a FlowResult.
    pub async fn resolve_template(
        &self,
        template: &ValueTemplate,
    ) -> ValueResolverResult<FlowResult> {
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

    pub fn resolve_variable(
        &self,
        variable: &str,
        default: Option<ValueRef>,
    ) -> ValueResolverResult<FlowResult> {
        let secrets = if let Some(variables_schema) = self.flow.variables() {
            variables_schema.secrets().field(variable)
        } else {
            log::debug!(
                "No variables schema defined; using empty secrets for variable '{}'",
                variable
            );
            Secrets::empty()
        };

        let value = if let Some(value) = self.variables.get(variable) {
            // 1. The variable was provided.
            value.clone()
        } else if let Some(value) = default {
            // 2. The reference provided a default value.
            value.clone()
        } else if let Some(schema) = self.flow.variables()
            && let Some(value) = schema.default_value(variable)
        {
            // 3. The schema provided a default value.
            value.clone()
        } else {
            return Err(ValueResolverError::UndefinedVariable(variable.to_string()).into());
        };

        // Log variable resolution with secret-aware sanitization
        let redacted = value.redacted(secrets);
        log::debug!("Resolved variable '{}' to: {}", variable, redacted);
        Ok(FlowResult::Success(value))
    }

    /// Resolve an expression, returning a FlowResult.
    pub async fn resolve_expr(&self, expr: &Expr) -> ValueResolverResult<FlowResult> {
        // Handle literal expressions
        if let Expr::EscapedLiteral { literal } = expr {
            return Ok(FlowResult::Success(literal.clone()));
        } else if let Expr::Literal(literal) = expr {
            return Ok(FlowResult::Success(literal.clone()));
        }

        // Get the base reference
        let base_ref = expr.base_ref().ok_or(ValueResolverError::Internal)?;

        let base_result = match base_ref {
            BaseRef::Workflow(_) => {
                // Return the workflow input
                FlowResult::Success(self.input.clone())
            }
            BaseRef::Step { step: step_id } => self.resolve_step(step_id).await?,
            BaseRef::Variable { variable, default } => {
                self.resolve_variable(variable, default.clone())?
            }
        };

        // Apply path if specified
        let path_result = if let Some(path) = expr.path() {
            match base_result {
                FlowResult::Success(result) => {
                    // For path resolution, we need to be careful about logging values
                    // We'll log the path resolution without exposing the full value
                    log::debug!("Resolving path '{}' on value", path);
                    if let Some(sub_value) = result.resolve_json_path(path) {
                        log::debug!("Path '{}' resolved successfully", path);
                        FlowResult::Success(sub_value)
                    } else {
                        log::debug!("Path '{path}' not found in value");
                        return Err(ValueResolverError::UndefinedField {
                            field: path.to_string(),
                            value: result,
                        }
                        .into());
                    }
                }
                FlowResult::Skipped { reason: _ } => FlowResult::Skipped { reason: None },
                other => other,
            }
        } else {
            base_result
        };

        // Handle skip actions.
        // NOTE: Skip actions are applied after path resolution.
        match path_result {
            FlowResult::Success(result) => Ok(FlowResult::Success(result)),
            FlowResult::Skipped { reason } => {
                match expr.on_skip() {
                    Some(SkipAction::UseDefault { default_value }) => {
                        let default = default_value
                            .as_ref()
                            .map(|v| v.as_ref())
                            .unwrap_or(&serde_json::Value::Null);
                        Ok(FlowResult::Success(ValueRef::new(default.clone())))
                    }
                    _ => {
                        // No on_skip action specified - propagate the skip
                        Ok(FlowResult::Skipped { reason })
                    }
                }
            }
            FlowResult::Failed(error) => Ok(FlowResult::Failed(error)),
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
            ValueTemplateRepr::Null => {
                Ok(FlowResult::Success(ValueRef::new(serde_json::Value::Null)))
            }
            ValueTemplateRepr::Bool(b) => Ok(FlowResult::Success(ValueRef::new(
                serde_json::Value::Bool(*b),
            ))),
            ValueTemplateRepr::Number(n) => Ok(FlowResult::Success(ValueRef::new(
                serde_json::Value::Number(n.clone()),
            ))),
            ValueTemplateRepr::String(s) => Ok(FlowResult::Success(ValueRef::new(
                serde_json::Value::String(s.clone()),
            ))),
            ValueTemplateRepr::Array(arr) => {
                // Process array recursively
                let mut result_array = Vec::new();
                for template in arr {
                    match Box::pin(self.resolve_template_rec(template)).await? {
                        FlowResult::Success(result) => {
                            result_array.push(result.as_ref().clone());
                        }
                        FlowResult::Skipped { reason } => {
                            return Ok(FlowResult::Skipped { reason });
                        }
                        FlowResult::Failed(error) => {
                            return Ok(FlowResult::Failed(error));
                        }
                    }
                }
                Ok(FlowResult::Success(ValueRef::new(
                    serde_json::Value::Array(result_array),
                )))
            }
            ValueTemplateRepr::Object(obj) => {
                // Process object recursively
                let mut result_map = serde_json::Map::new();
                for (k, template) in obj {
                    match Box::pin(self.resolve_template_rec(template)).await? {
                        FlowResult::Success(result) => {
                            result_map.insert(k.clone(), result.as_ref().clone());
                        }
                        FlowResult::Skipped { reason } => {
                            return Ok(FlowResult::Skipped { reason });
                        }
                        FlowResult::Failed(error) => {
                            return Ok(FlowResult::Failed(error));
                        }
                    }
                }
                Ok(FlowResult::Success(ValueRef::new(
                    serde_json::Value::Object(result_map),
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::SchemaRef;
    use crate::workflow::{FlowV1, JsonPath, VariableSchema};
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
        use crate::workflow::{FlowBuilder, StepBuilder};

        Arc::new(
            FlowBuilder::new()
                .step(
                    StepBuilder::mock_step("step1")
                        .input_literal(json!({}))
                        .build(),
                )
                .output(ValueTemplate::literal(json!(null)))
                .build(),
        )
    }

    #[tokio::test]
    async fn test_resolve_template() {
        let workflow_input = ValueRef::new(json!({"name": "Alice"}));
        let loader = MockValueLoader::new(workflow_input.clone());
        let run_id = Uuid::now_v7();
        let flow = create_test_flow();

        let resolver = ValueResolver::new(run_id, workflow_input, loader, flow);

        // Test resolving ValueTemplate - create a template with an expression by deserializing from JSON
        let template = ValueTemplate::workflow_input(JsonPath::from("name"));
        let resolved = resolver.resolve_template(&template).await.unwrap();
        match resolved {
            FlowResult::Success(result) => {
                assert_eq!(result.as_ref(), &json!("Alice"));
            }
            _ => panic!("Expected successful result, got: {resolved:?}"),
        }
    }

    #[tokio::test]
    async fn test_resolve_variable() {
        let workflow_input = ValueRef::new(json!({}));
        let loader = MockValueLoader::new(workflow_input.clone());
        let run_id = Uuid::now_v7();

        // Create flow with variables schema
        let variables_schema_json = json!({
            "type": "object",
            "properties": {
                "api_key": {
                    "type": "string",
                    "description": "API key for external service"
                },
                "temperature": {
                    "type": "number",
                    "default": 0.7
                }
            },
            "required": ["api_key"]
        });

        let variables_schema =
            VariableSchema::new(SchemaRef::parse_json(&variables_schema_json.to_string()).unwrap());

        let flow = Arc::new(Flow::V1(FlowV1 {
            variables: Some(variables_schema),
            ..Default::default()
        }));

        // Set up variables
        let mut variables = HashMap::new();
        variables.insert("api_key".to_string(), ValueRef::new(json!("test-key-123")));

        let resolver =
            ValueResolver::new_with_variables(run_id, workflow_input, loader, flow, variables);

        // Test resolving provided variable
        let api_key_template = ValueTemplate::variable_ref("api_key", None, JsonPath::default());
        let resolved = resolver.resolve_template(&api_key_template).await.unwrap();
        match resolved {
            FlowResult::Success(result) => {
                assert_eq!(result.as_ref(), &json!("test-key-123"));
            }
            _ => panic!("Expected successful result, got: {resolved:?}"),
        }

        // Test resolving variable with default value in schema
        let temperature_template =
            ValueTemplate::variable_ref("temperature", None, JsonPath::default());
        let resolved = resolver
            .resolve_template(&temperature_template)
            .await
            .unwrap();
        match resolved {
            FlowResult::Success(result) => {
                assert_eq!(result.as_ref(), &json!(0.7));
            }
            _ => panic!("Expected successful result, got: {resolved:?}"),
        }

        // Test resolving variable with default value in reference.
        let temperature_template = ValueTemplate::variable_ref(
            "temperature",
            Some(json!(0.8).into()),
            JsonPath::default(),
        );
        let resolved = resolver
            .resolve_template(&temperature_template)
            .await
            .unwrap();
        match resolved {
            FlowResult::Success(result) => {
                assert_eq!(result.as_ref(), &json!(0.8));
            }
            _ => panic!("Expected successful result, got: {resolved:?}"),
        }
    }

    #[tokio::test]
    async fn test_resolve_undefined_variable() {
        let workflow_input = ValueRef::new(json!({}));
        let loader = MockValueLoader::new(workflow_input.clone());
        let run_id = Uuid::now_v7();
        let flow = create_test_flow(); // Flow without variables

        let resolver = ValueResolver::new(run_id, workflow_input, loader, flow);

        // Test resolving undefined variable
        let undefined_template =
            ValueTemplate::variable_ref("undefined_var", None, JsonPath::default());
        let resolved = resolver.resolve_template(&undefined_template).await;

        assert!(resolved.is_err());
        // The error should be about undefined value
        match resolved.unwrap_err().current_context() {
            ValueResolverError::UndefinedVariable(variable) => {
                assert_eq!(variable, "undefined_var");
            }
            _ => panic!("Expected UndefinedValue error"),
        }
    }

    #[tokio::test]
    async fn test_resolve_variable_secret_sanitization() {
        use crate::schema::SchemaRef;
        use crate::workflow::{FlowBuilder, VariableSchema};
        use serde_json::json;
        use std::collections::HashMap;

        // Create a flow with variables schema that marks api_key as secret
        let schema_json = json!({
            "type": "object",
            "properties": {
                "api_key": {
                    "type": "string",
                    "is_secret": true
                },
                "username": {
                    "type": "string"
                }
            },
            "required": ["api_key"]
        });

        let variables_schema =
            VariableSchema::new(SchemaRef::parse_json(&schema_json.to_string()).unwrap());

        // Build flow with variables schema
        let flow = Arc::new(
            FlowBuilder::new()
                .variables(variables_schema)
                .output(ValueTemplate::literal(json!(null)))
                .build(),
        );

        // Create variables with secret data
        let mut variables = HashMap::new();
        variables.insert(
            "api_key".to_string(),
            ValueRef::new(json!("secret-key-123")),
        );
        variables.insert("username".to_string(), ValueRef::new(json!("alice")));

        let run_id = Uuid::now_v7();
        let workflow_input = ValueRef::new(json!({}));
        let loader = MockValueLoader::new(workflow_input.clone());

        let resolver = ValueResolver::new_with_variables(
            run_id,
            workflow_input,
            loader,
            flow.clone(),
            variables,
        );

        // Test that variable resolution doesn't expose secrets in logs
        // We can't easily test the log output directly, but we can test the SanitizedVariable
        // formatting that would be used in logs
        let api_key_result = resolver.resolve_variable("api_key", None).unwrap();
        if let FlowResult::Success(api_key_value) = api_key_result {
            if let Some(variables_schema) = flow.variables() {
                let sanitized_string = variables_schema
                    .secrets()
                    .field("api_key")
                    .redacted(&api_key_value.value())
                    .to_string();

                // The sanitized version should redact the secret
                assert!(sanitized_string.contains("[REDACTED]"));
                assert!(!sanitized_string.contains("secret-key-123"));
            }
        } else {
            panic!("Expected successful variable resolution");
        }

        // Test that non-secret variables are not redacted
        let username_result = resolver.resolve_variable("username", None).unwrap();
        if let FlowResult::Success(username_value) = username_result {
            if let Some(variables_schema) = flow.variables() {
                let sanitized_string = variables_schema
                    .secrets()
                    .field("username")
                    .redacted(&username_value.value())
                    .to_string();

                // The sanitized version should show the non-secret value
                assert!(sanitized_string.contains("alice"));
                assert!(!sanitized_string.contains("[REDACTED]"));
            }
        } else {
            panic!("Expected successful variable resolution");
        }
    }

    #[tokio::test]
    async fn test_resolve_variable_with_path() {
        let workflow_input = ValueRef::new(json!({}));
        let loader = MockValueLoader::new(workflow_input.clone());
        let run_id = Uuid::now_v7();
        let flow = create_test_flow();

        // Set up variables with nested object
        let mut variables = HashMap::new();
        variables.insert(
            "config".to_string(),
            ValueRef::new(json!({
                "api": {
                    "key": "nested-key",
                    "timeout": 30
                }
            })),
        );

        let resolver =
            ValueResolver::new_with_variables(run_id, workflow_input, loader, flow, variables);

        // Test resolving variable with JSON path
        let config_key_template =
            ValueTemplate::variable_ref("config", None, JsonPath::from("$.api.key"));
        let resolved = resolver
            .resolve_template(&config_key_template)
            .await
            .unwrap();
        match resolved {
            FlowResult::Success(result) => {
                assert_eq!(result.as_ref(), &json!("nested-key"));
            }
            _ => panic!("Expected successful result, got: {resolved:?}"),
        }
    }
}
