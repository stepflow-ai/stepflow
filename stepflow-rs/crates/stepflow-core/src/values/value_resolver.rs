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

use super::{JsonPath as NewJsonPath, PathPart, Secrets, ValueExpr, ValueRef};
use crate::{
    workflow::{Flow, StepId},
    FlowResult,
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
    #[error("Undefined step reference: {0}")]
    UndefinedValue(String),
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
            Err(ValueResolverError::UndefinedValue(step_id.to_string()).into())
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
        path: &[PathPart],
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

        // Apply path if specified
        if !path.is_empty() {
            let path_json = NewJsonPath::from_parts(path.to_vec());
            log::debug!("Resolving path '{}' on variable '{}'", path_json, variable);
            if let Some(sub_value) = value.resolve_json_path(&path_json) {
                log::debug!("Path '{}' resolved successfully", path_json);
                Ok(FlowResult::Success(sub_value))
            } else {
                log::debug!("Path '{}' not found in variable '{}'", path_json, variable);
                Err(ValueResolverError::UndefinedField {
                    field: path_json.to_string(),
                    value,
                }
                .into())
            }
        } else {
            Ok(FlowResult::Success(value))
        }
    }

    /// Resolve a ValueExpr, returning a FlowResult.
    ///
    /// This is the new resolution method for the simplified ValueExpr type.
    pub async fn resolve(&self, expr: &ValueExpr) -> ValueResolverResult<FlowResult> {
        match expr {
            ValueExpr::Step { step, path } => {
                // Get the step result
                let base_result = self.resolve_step(step).await?;

                // Apply path if specified and not empty
                if !path.is_empty() {
                    match base_result {
                        FlowResult::Success(value) => {
                            log::debug!("Resolving path '{}' on step '{}'", path, step);
                            if let Some(sub_value) = value.resolve_json_path(path) {
                                log::debug!("Path '{}' resolved successfully", path);
                                Ok(FlowResult::Success(sub_value))
                            } else {
                                log::debug!("Path '{}' not found in value", path);
                                Err(ValueResolverError::UndefinedField {
                                    field: path.to_string(),
                                    value,
                                }
                                .into())
                            }
                        }
                        // Skip and Failed propagate through
                        other => Ok(other),
                    }
                } else {
                    // No path, return base result directly
                    Ok(base_result)
                }
            }
            ValueExpr::Input { input } => {
                // Apply the input path to the workflow input
                if !input.is_empty() {
                    log::debug!("Resolving input path '{}'", input);
                    if let Some(sub_value) = self.input.resolve_json_path(input) {
                        log::debug!("Input path '{}' resolved successfully", input);
                        Ok(FlowResult::Success(sub_value))
                    } else {
                        log::debug!("Input path '{}' not found", input);
                        Err(ValueResolverError::UndefinedField {
                            field: input.to_string(),
                            value: self.input.clone(),
                        }
                        .into())
                    }
                } else {
                    // Empty path means root
                    Ok(FlowResult::Success(self.input.clone()))
                }
            }
            ValueExpr::Variable { variable, default } => {
                // Extract variable name from first path part
                let parts = variable.parts();

                if parts.is_empty() {
                    return Err(ValueResolverError::Internal.into());
                }

                // First part should be the variable name
                let var_name = match &parts[0] {
                    PathPart::Field(name) | PathPart::IndexStr(name) => name.as_str(),
                    PathPart::Index(_) => {
                        return Err(ValueResolverError::Internal.into());
                    }
                };

                // Remaining parts form the sub-path (as a slice, no allocation)
                let sub_path = if parts.len() > 1 {
                    &parts[1..]
                } else {
                    &[]
                };

                // Try to resolve the variable with path
                let var_result = self.resolve_variable(var_name, sub_path, None);

                match var_result {
                    Ok(flow_result) => Ok(flow_result),
                    Err(_) => {
                        // Variable not found or path failed, try default if available
                        if let Some(default_expr) = default {
                            log::debug!("Variable '{}' resolution failed, using default", var_name);
                            Box::pin(self.resolve(default_expr)).await
                        } else {
                            // No default, propagate the error
                            var_result
                        }
                    }
                }
            }
            ValueExpr::EscapedLiteral { literal } => {
                Ok(FlowResult::Success(ValueRef::new(literal.clone())))
            }
            ValueExpr::Literal(value) => {
                Ok(FlowResult::Success(ValueRef::new(value.clone())))
            }
            ValueExpr::Array(items) => {
                let mut result_array = Vec::new();
                for item in items {
                    match Box::pin(self.resolve(item)).await? {
                        FlowResult::Success(value) => {
                            result_array.push(value.as_ref().clone());
                        }
                        // Propagate skip/fail from array elements
                        flow_result @ (FlowResult::Skipped { .. } | FlowResult::Failed(_)) => {
                            return Ok(flow_result);
                        }
                    }
                }
                Ok(FlowResult::Success(ValueRef::new(
                    serde_json::Value::Array(result_array),
                )))
            }
            ValueExpr::Object(fields) => {
                let mut result_map = serde_json::Map::new();
                for (k, v) in fields {
                    match Box::pin(self.resolve(v)).await? {
                        FlowResult::Success(value) => {
                            result_map.insert(k.clone(), value.as_ref().clone());
                        }
                        // Propagate skip/fail from object fields
                        flow_result @ (FlowResult::Skipped { .. } | FlowResult::Failed(_)) => {
                            return Ok(flow_result);
                        }
                    }
                }
                Ok(FlowResult::Success(ValueRef::new(
                    serde_json::Value::Object(result_map),
                )))
            }
            ValueExpr::If {
                condition,
                then,
                else_expr,
            } => {
                // Resolve the condition
                let condition_result = Box::pin(self.resolve(condition)).await?;

                // Check if condition is truthy
                let is_truthy = match condition_result {
                    FlowResult::Success(ref value) => {
                        // Check truthiness: not null, not false, not empty
                        match value.as_ref() {
                            serde_json::Value::Null => false,
                            serde_json::Value::Bool(b) => *b,
                            _ => true, // Non-null, non-false values are truthy
                        }
                    }
                    FlowResult::Skipped { .. } => false, // Skipped is falsy
                    FlowResult::Failed(_) => {
                        // Propagate errors from condition evaluation
                        return Ok(condition_result);
                    }
                };

                // Resolve appropriate branch
                if is_truthy {
                    Box::pin(self.resolve(then)).await
                } else if let Some(else_val) = else_expr {
                    Box::pin(self.resolve(else_val)).await
                } else {
                    // No else branch, default to null
                    Ok(FlowResult::Success(ValueRef::new(serde_json::Value::Null)))
                }
            }
            ValueExpr::Coalesce { values } => {
                // Try each value in order, return first non-skipped, non-null result
                for (i, value_expr) in values.iter().enumerate() {
                    let result = Box::pin(self.resolve(value_expr)).await?;

                    match result {
                        FlowResult::Success(ref value) if !value.as_ref().is_null() => {
                            // Non-null success - return it
                            log::debug!(
                                "Coalesce: value at index {} succeeded with non-null result",
                                i
                            );
                            return Ok(result);
                        }
                        FlowResult::Success(_) => {
                            // Null success - continue to next value
                            log::debug!("Coalesce: value at index {} was null, trying next", i);
                            continue;
                        }
                        FlowResult::Skipped { .. } => {
                            // Skipped - continue to next value
                            log::debug!("Coalesce: value at index {} was skipped, trying next", i);
                            continue;
                        }
                        FlowResult::Failed(err) => {
                            // Error - propagate immediately, don't continue
                            log::debug!("Coalesce: value at index {} failed with error", i);
                            return Ok(FlowResult::Failed(err));
                        }
                    }
                }

                // All values were null or skipped - return null
                log::debug!("Coalesce: all values were null or skipped, returning null");
                Ok(FlowResult::Success(ValueRef::new(serde_json::Value::Null)))
            }
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;
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
                .output(ValueExpr::literal(json!(null)))
                .build(),
        )
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
                .output(ValueExpr::literal(json!(null)))
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
        let api_key_result = resolver.resolve_variable("api_key", &[], None).unwrap();
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
        let username_result = resolver.resolve_variable("username", &[], None).unwrap();
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

    // Tests for new ValueExpr resolution
    #[tokio::test]
    async fn test_resolve_literal() {
        use crate::values::ValueExpr;

        let workflow_input = ValueRef::new(json!({}));
        let loader = MockValueLoader::new(workflow_input.clone());
        let run_id = Uuid::now_v7();
        let flow = create_test_flow();
        let resolver = ValueResolver::new(run_id, workflow_input, loader, flow);

        // Test literal values
        let expr = ValueExpr::Literal(json!("hello"));
        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(value.as_ref(), &json!("hello"));

        // Test escaped literal
        let expr = ValueExpr::escaped_literal(json!({"$step": "not_a_ref"}));
        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(value.as_ref(), &json!({"$step": "not_a_ref"}));
    }

    #[tokio::test]
    async fn test_resolve_input() {
        use crate::values::{JsonPath as NewJsonPath, ValueExpr};

        let workflow_input = ValueRef::new(json!({"name": "Alice", "age": 30}));
        let loader = MockValueLoader::new(workflow_input.clone());
        let run_id = Uuid::now_v7();
        let flow = create_test_flow();
        let resolver = ValueResolver::new(run_id, workflow_input, loader, flow);

        // Test input with path
        let expr = ValueExpr::workflow_input(NewJsonPath::from("name"));
        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(value.as_ref(), &json!("Alice"));

        // Test input root (empty path)
        let expr = ValueExpr::workflow_input(NewJsonPath::new());
        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(value.as_ref(), &json!({"name": "Alice", "age": 30}));
    }

    #[tokio::test]
    async fn test_resolve_variable() {
        use crate::values::{JsonPath as NewJsonPath, ValueExpr};

        let workflow_input = ValueRef::new(json!({}));
        let loader = MockValueLoader::new(workflow_input.clone());
        let run_id = Uuid::now_v7();
        let flow = create_test_flow();

        let mut variables = HashMap::new();
        variables.insert("api_key".to_string(), ValueRef::new(json!("secret123")));
        variables.insert(
            "config".to_string(),
            ValueRef::new(json!({"timeout": 30, "retries": 3})),
        );

        let resolver =
            ValueResolver::new_with_variables(run_id, workflow_input, loader, flow, variables);

        // Test simple variable
        let expr = ValueExpr::variable("api_key", None);
        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(value.as_ref(), &json!("secret123"));

        // Test variable with path
        let expr = ValueExpr::Variable {
            variable: NewJsonPath::from("$.config.timeout"),
            default: None,
        };
        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(value.as_ref(), &json!(30));

        // Test variable with default
        let default_expr = Box::new(ValueExpr::Literal(json!("default_value")));
        let expr = ValueExpr::variable("missing_var", Some(default_expr));
        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(value.as_ref(), &json!("default_value"));
    }

    #[tokio::test]
    async fn test_resolve_composable() {
        use crate::values::{JsonPath as NewJsonPath, ValueExpr};

        let workflow_input = ValueRef::new(json!({"x": 10, "y": 20}));
        let loader = MockValueLoader::new(workflow_input.clone());
        let run_id = Uuid::now_v7();
        let flow = create_test_flow();

        let mut variables = HashMap::new();
        variables.insert("multiplier".to_string(), ValueRef::new(json!(2)));

        let resolver =
            ValueResolver::new_with_variables(run_id, workflow_input, loader, flow, variables);

        // Test array composition
        let expr = ValueExpr::Array(vec![
            ValueExpr::workflow_input(NewJsonPath::from("x")),
            ValueExpr::workflow_input(NewJsonPath::from("y")),
            ValueExpr::variable("multiplier", None),
        ]);
        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(value.as_ref(), &json!([10, 20, 2]));

        // Test object composition
        let expr = ValueExpr::Object(vec![
            (
                "input_x".to_string(),
                ValueExpr::workflow_input(NewJsonPath::from("x")),
            ),
            (
                "multiplier".to_string(),
                ValueExpr::variable("multiplier", None),
            ),
            ("constant".to_string(), ValueExpr::Literal(json!(100))),
        ]);
        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(
            value.as_ref(),
            &json!({"input_x": 10, "multiplier": 2, "constant": 100})
        );
    }

    #[tokio::test]
    async fn test_resolve_step_reference() {
        use crate::values::{JsonPath as NewJsonPath, ValueExpr};
        use crate::workflow::{FlowBuilder, StepBuilder};

        let workflow_input = ValueRef::new(json!({}));

        // Create a flow with a step
        let flow = Arc::new(
            FlowBuilder::new()
                .step(
                    StepBuilder::mock_step("step1")
                        .input(ValueExpr::literal(json!({})))
                        .build(),
                )
                .output(ValueExpr::literal(json!(null)))
                .build(),
        );

        // Create loader with step result
        let mut loader = MockValueLoader::new(workflow_input.clone());
        loader.step_results.insert(
            0,
            FlowResult::Success(ValueRef::new(json!({"result": "success", "count": 42}))),
        );

        let run_id = Uuid::now_v7();
        let resolver = ValueResolver::new(run_id, workflow_input, loader, flow);

        // Test step reference without path (gets entire result)
        let expr = ValueExpr::step("step1", NewJsonPath::default());
        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(value.as_ref(), &json!({"result": "success", "count": 42}));

        // Test step reference with simple path
        let expr = ValueExpr::step("step1", NewJsonPath::from("result"));
        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(value.as_ref(), &json!("success"));

        // Test step reference with JSONPath
        let expr = ValueExpr::step("step1", NewJsonPath::from("$.count"));
        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(value.as_ref(), &json!(42));
    }

    #[tokio::test]
    async fn test_resolve_step_with_nested_path() {
        use crate::values::{JsonPath as NewJsonPath, ValueExpr};
        use crate::workflow::{FlowBuilder, StepBuilder};

        let workflow_input = ValueRef::new(json!({}));

        // Create a flow with a step that returns nested data
        let flow = Arc::new(
            FlowBuilder::new()
                .step(
                    StepBuilder::mock_step("data_step")
                        .input(ValueExpr::literal(json!({})))
                        .build(),
                )
                .output(ValueExpr::literal(json!(null)))
                .build(),
        );

        // Create loader with nested step result
        let mut loader = MockValueLoader::new(workflow_input.clone());
        loader.step_results.insert(
            0,
            FlowResult::Success(ValueRef::new(json!({
                "user": {
                    "profile": {
                        "name": "Alice",
                        "age": 30
                    },
                    "settings": {
                        "theme": "dark"
                    }
                }
            }))),
        );

        let run_id = Uuid::now_v7();
        let resolver = ValueResolver::new(run_id, workflow_input, loader, flow);

        // Test deeply nested path
        let expr = ValueExpr::step("data_step", NewJsonPath::from("$.user.profile.name"));
        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(value.as_ref(), &json!("Alice"));

        // Test another nested path
        let expr = ValueExpr::step("data_step", NewJsonPath::from("$.user.settings.theme"));
        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(value.as_ref(), &json!("dark"));
    }

    #[tokio::test]
    async fn test_resolve_variable_with_expression_default() {
        use crate::values::{JsonPath as NewJsonPath, ValueExpr};

        let workflow_input = ValueRef::new(json!({"fallback_value": 100}));
        let loader = MockValueLoader::new(workflow_input.clone());
        let run_id = Uuid::now_v7();
        let flow = create_test_flow();

        // No variables provided - will use defaults
        let resolver = ValueResolver::new(run_id, workflow_input.clone(), loader, flow);

        // Test variable with expression default (references input)
        let default_expr = Box::new(ValueExpr::workflow_input(NewJsonPath::from("fallback_value")));
        let expr = ValueExpr::variable("missing_var", Some(default_expr));

        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(value.as_ref(), &json!(100));
    }

    #[tokio::test]
    async fn test_resolve_nested_with_step_refs() {
        use crate::values::{JsonPath as NewJsonPath, ValueExpr};
        use crate::workflow::{FlowBuilder, StepBuilder};

        let workflow_input = ValueRef::new(json!({"multiplier": 2}));

        // Create a flow with multiple steps
        let flow = Arc::new(
            FlowBuilder::new()
                .step(
                    StepBuilder::mock_step("step1")
                        .input(ValueExpr::literal(json!({})))
                        .build(),
                )
                .step(
                    StepBuilder::mock_step("step2")
                        .input(ValueExpr::literal(json!({})))
                        .build(),
                )
                .output(ValueExpr::literal(json!(null)))
                .build(),
        );

        // Create loader with step results
        let mut loader = MockValueLoader::new(workflow_input.clone());
        loader.step_results.insert(0, FlowResult::Success(ValueRef::new(json!({"x": 10}))));
        loader.step_results.insert(1, FlowResult::Success(ValueRef::new(json!({"y": 20}))));

        let run_id = Uuid::now_v7();
        let resolver = ValueResolver::new(run_id, workflow_input, loader, flow);

        // Test nested object with mixed references
        let expr = ValueExpr::Object(vec![
            (
                "from_step1".to_string(),
                ValueExpr::step("step1", NewJsonPath::from("x")),
            ),
            (
                "from_step2".to_string(),
                ValueExpr::step("step2", NewJsonPath::from("y")),
            ),
            (
                "nested".to_string(),
                ValueExpr::Object(vec![
                    (
                        "step1_full".to_string(),
                        ValueExpr::step("step1", NewJsonPath::default()),
                    ),
                    ("literal".to_string(), ValueExpr::Literal(json!("value"))),
                ]),
            ),
        ]);

        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(
            value.as_ref(),
            &json!({
                "from_step1": 10,
                "from_step2": 20,
                "nested": {
                    "step1_full": {"x": 10},
                    "literal": "value"
                }
            })
        );
    }

    #[tokio::test]
    async fn test_resolve_array_with_step_refs() {
        use crate::values::{JsonPath as NewJsonPath, ValueExpr};
        use crate::workflow::{FlowBuilder, StepBuilder};

        let workflow_input = ValueRef::new(json!({}));

        // Create a flow with steps
        let flow = Arc::new(
            FlowBuilder::new()
                .step(
                    StepBuilder::mock_step("step1")
                        .input(ValueExpr::literal(json!({})))
                        .build(),
                )
                .step(
                    StepBuilder::mock_step("step2")
                        .input(ValueExpr::literal(json!({})))
                        .build(),
                )
                .output(ValueExpr::literal(json!(null)))
                .build(),
        );

        // Create loader with step results
        let mut loader = MockValueLoader::new(workflow_input.clone());
        loader.step_results.insert(0, FlowResult::Success(ValueRef::new(json!(1))));
        loader.step_results.insert(1, FlowResult::Success(ValueRef::new(json!(2))));

        let run_id = Uuid::now_v7();
        let resolver = ValueResolver::new(run_id, workflow_input, loader, flow);

        // Test array with step references
        let expr = ValueExpr::Array(vec![
            ValueExpr::step("step1", NewJsonPath::default()),
            ValueExpr::step("step2", NewJsonPath::default()),
            ValueExpr::Literal(json!(3)),
        ]);

        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(value.as_ref(), &json!([1, 2, 3]));
    }

    #[tokio::test]
    async fn test_resolve_error_invalid_step() {
        use crate::values::{JsonPath as NewJsonPath, ValueExpr};

        let workflow_input = ValueRef::new(json!({}));
        let loader = MockValueLoader::new(workflow_input.clone());
        let run_id = Uuid::now_v7();
        let flow = create_test_flow();

        let resolver = ValueResolver::new(run_id, workflow_input, loader, flow);

        // Test step reference to non-existent step
        let expr = ValueExpr::step("nonexistent_step", NewJsonPath::default());
        let result = resolver.resolve(&expr).await;

        // Should be an error (step not found in flow)
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_resolve_if_expression_truthy() {
        use crate::values::ValueExpr;

        let workflow_input = ValueRef::new(json!({"enabled": true}));
        let loader = MockValueLoader::new(workflow_input.clone());
        let run_id = Uuid::now_v7();
        let flow = create_test_flow();

        let resolver = ValueResolver::new(run_id, workflow_input, loader, flow);

        // Test $if with truthy condition
        let expr = ValueExpr::if_expr(
            ValueExpr::literal(json!(true)),
            ValueExpr::literal(json!("then_value")),
            Some(ValueExpr::literal(json!("else_value"))),
        );

        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(value.as_ref(), &json!("then_value"));
    }

    #[tokio::test]
    async fn test_resolve_if_expression_falsy() {
        use crate::values::ValueExpr;

        let workflow_input = ValueRef::new(json!({"enabled": false}));
        let loader = MockValueLoader::new(workflow_input.clone());
        let run_id = Uuid::now_v7();
        let flow = create_test_flow();

        let resolver = ValueResolver::new(run_id, workflow_input, loader, flow);

        // Test $if with falsy condition
        let expr = ValueExpr::if_expr(
            ValueExpr::literal(json!(false)),
            ValueExpr::literal(json!("then_value")),
            Some(ValueExpr::literal(json!("else_value"))),
        );

        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(value.as_ref(), &json!("else_value"));
    }

    #[tokio::test]
    async fn test_resolve_if_expression_default_null() {
        use crate::values::ValueExpr;

        let workflow_input = ValueRef::new(json!({}));
        let loader = MockValueLoader::new(workflow_input.clone());
        let run_id = Uuid::now_v7();
        let flow = create_test_flow();

        let resolver = ValueResolver::new(run_id, workflow_input, loader, flow);

        // Test $if with no else clause (defaults to null)
        let expr = ValueExpr::if_expr(
            ValueExpr::literal(json!(false)),
            ValueExpr::literal(json!("then_value")),
            None,
        );

        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(value.as_ref(), &json!(null));
    }

    #[tokio::test]
    async fn test_resolve_if_with_input_condition() {
        use crate::values::{JsonPath as NewJsonPath, ValueExpr};

        let workflow_input = ValueRef::new(json!({"shouldSkip": true}));
        let loader = MockValueLoader::new(workflow_input.clone());
        let run_id = Uuid::now_v7();
        let flow = create_test_flow();

        let resolver = ValueResolver::new(run_id, workflow_input, loader, flow);

        // Test $if with condition from input
        let expr = ValueExpr::if_expr(
            ValueExpr::workflow_input(NewJsonPath::from("shouldSkip")),
            ValueExpr::literal(json!("skipped")),
            Some(ValueExpr::literal(json!("not_skipped"))),
        );

        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(value.as_ref(), &json!("skipped"));
    }

    #[tokio::test]
    async fn test_resolve_coalesce_first_value() {
        use crate::values::ValueExpr;

        let workflow_input = ValueRef::new(json!({}));
        let loader = MockValueLoader::new(workflow_input.clone());
        let run_id = Uuid::now_v7();
        let flow = create_test_flow();

        let resolver = ValueResolver::new(run_id, workflow_input, loader, flow);

        // Test $coalesce - first value is non-null
        let expr = ValueExpr::coalesce(vec![
            ValueExpr::literal(json!("first")),
            ValueExpr::literal(json!("second")),
        ]);

        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(value.as_ref(), &json!("first"));
    }

    #[tokio::test]
    async fn test_resolve_coalesce_skip_nulls() {
        use crate::values::ValueExpr;

        let workflow_input = ValueRef::new(json!({}));
        let loader = MockValueLoader::new(workflow_input.clone());
        let run_id = Uuid::now_v7();
        let flow = create_test_flow();

        let resolver = ValueResolver::new(run_id, workflow_input, loader, flow);

        // Test $coalesce - skip null values
        let expr = ValueExpr::coalesce(vec![
            ValueExpr::literal(json!(null)),
            ValueExpr::literal(json!("second")),
            ValueExpr::literal(json!("third")),
        ]);

        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(value.as_ref(), &json!("second"));
    }

    #[tokio::test]
    async fn test_resolve_coalesce_all_null() {
        use crate::values::ValueExpr;

        let workflow_input = ValueRef::new(json!({}));
        let loader = MockValueLoader::new(workflow_input.clone());
        let run_id = Uuid::now_v7();
        let flow = create_test_flow();

        let resolver = ValueResolver::new(run_id, workflow_input, loader, flow);

        // Test $coalesce - all values null, return null
        let expr = ValueExpr::coalesce(vec![
            ValueExpr::literal(json!(null)),
            ValueExpr::literal(json!(null)),
        ]);

        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(value.as_ref(), &json!(null));
    }

    #[tokio::test]
    async fn test_resolve_coalesce_with_variable_fallback() {
        use crate::values::{JsonPath as NewJsonPath, ValueExpr};

        let workflow_input = ValueRef::new(json!({"fallback": "fallback_value"}));
        let loader = MockValueLoader::new(workflow_input.clone());
        let run_id = Uuid::now_v7();
        let flow = create_test_flow();

        let resolver = ValueResolver::new(run_id, workflow_input.clone(), loader, flow);

        // Test $coalesce with missing variable, fallback to input
        let _expr = ValueExpr::coalesce(vec![
            ValueExpr::variable("missing_var", None),
            ValueExpr::workflow_input(NewJsonPath::from("fallback")),
        ]);

        // This should fall through to the input since variable will error
        // Actually, errors propagate, so this needs a default on the variable
        let expr = ValueExpr::coalesce(vec![
            ValueExpr::variable("missing_var", Some(Box::new(ValueExpr::null()))),
            ValueExpr::workflow_input(NewJsonPath::from("fallback")),
        ]);

        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(value.as_ref(), &json!("fallback_value"));
    }

    #[tokio::test]
    async fn test_resolve_complex_mixed_composition() {
        use crate::values::{JsonPath as NewJsonPath, ValueExpr};
        use crate::workflow::{FlowBuilder, StepBuilder};

        let workflow_input = ValueRef::new(json!({
            "config": {
                "enabled": true,
                "timeout": 30
            }
        }));

        // Create a flow with a step
        let flow = Arc::new(
            FlowBuilder::new()
                .step(
                    StepBuilder::mock_step("data_step")
                        .input(ValueExpr::literal(json!({})))
                        .build(),
                )
                .output(ValueExpr::literal(json!(null)))
                .build(),
        );

        // Create loader with step result
        let mut loader = MockValueLoader::new(workflow_input.clone());
        loader.step_results.insert(
            0,
            FlowResult::Success(ValueRef::new(json!({"items": [1, 2, 3]}))),
        );

        let mut variables = HashMap::new();
        variables.insert("api_version".to_string(), ValueRef::new(json!("v2")));

        let run_id = Uuid::now_v7();
        let resolver = ValueResolver::new_with_variables(
            run_id,
            workflow_input,
            loader,
            flow,
            variables,
        );

        // Test complex nested structure with all expression types
        let expr = ValueExpr::Object(vec![
            (
                "metadata".to_string(),
                ValueExpr::Object(vec![
                    (
                        "version".to_string(),
                        ValueExpr::variable("api_version", None),
                    ),
                    (
                        "config_enabled".to_string(),
                        ValueExpr::workflow_input(NewJsonPath::from("$.config.enabled")),
                    ),
                ]),
            ),
            (
                "data".to_string(),
                ValueExpr::step("data_step", NewJsonPath::from("items")),
            ),
            (
                "mixed_array".to_string(),
                ValueExpr::Array(vec![
                    ValueExpr::workflow_input(NewJsonPath::from("$.config.timeout")),
                    ValueExpr::Literal(json!("constant")),
                    ValueExpr::step("data_step", NewJsonPath::from("$.items[0]")),
                ]),
            ),
        ]);

        let value = resolver.resolve(&expr).await.unwrap().unwrap_success();
        assert_eq!(
            value.as_ref(),
            &json!({
                "metadata": {
                    "version": "v2",
                    "config_enabled": true
                },
                "data": [1, 2, 3],
                "mixed_array": [30, "constant", 1]
            })
        );
    }
}
