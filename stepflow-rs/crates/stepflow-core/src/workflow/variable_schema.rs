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

use crate::{
    schema::SchemaRef,
    values::{Secrets, ValueRef},
};
use log::debug;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Variable schema for workflow variables using JSON Schema format.
///
/// This allows flows to declare required variables with their types,
/// default values, descriptions, and secret annotations.
///
/// Example:
/// ```yaml
/// variables:
///   type: object
///   properties:
///     api_key:
///       type: string
///       is_secret: true
///       description: "OpenAI API key"
///     temperature:
///       type: number
///       default: 0.7
///       minimum: 0
///       maximum: 2
///   required: ["api_key"]
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(from = "SchemaRef", into = "SchemaRef")]
pub struct VariableSchema {
    schema: SchemaRef,
    variables: Vec<String>,
    defaults: HashMap<String, ValueRef>,
    secrets: Secrets,
    required: HashSet<String>,
}

impl utoipa::PartialSchema for VariableSchema {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        SchemaRef::schema()
    }
}

impl utoipa::ToSchema for VariableSchema {
    fn name() -> std::borrow::Cow<'static, str> {
        SchemaRef::name()
    }

    fn schemas(
        schemas: &mut Vec<(
            String,
            utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
        )>,
    ) {
        SchemaRef::schemas(schemas);
    }
}

impl From<SchemaRef> for VariableSchema {
    fn from(schema: SchemaRef) -> Self {
        Self::new(schema)
    }
}

impl From<VariableSchema> for SchemaRef {
    fn from(var_schema: VariableSchema) -> Self {
        var_schema.schema
    }
}

impl VariableSchema {
    /// Create a new variable schema from a JSON Schema.
    pub fn new(schema: SchemaRef) -> Self {
        let schema_value = schema.as_value();

        let mut required = HashSet::new();
        if let Some(required_array) = schema_value.get("required").and_then(|r| r.as_array()) {
            for req in required_array {
                if let Some(req_str) = req.as_str() {
                    required.insert(req_str.to_string());
                }
            }
        }

        let mut variables = Vec::new();
        let mut defaults = HashMap::new();
        if let Some(properties) = schema_value.get("properties").and_then(|p| p.as_object()) {
            for (var_name, var_schema) in properties {
                variables.push(var_name.clone());
                let var_type = var_schema.get("type");
                let var_default = if let Some(default_value) = var_schema.get("default") {
                    Some(default_value.clone())
                } else if !required.contains(var_name) {
                    match var_type {
                        Some(serde_json::Value::String(type_str)) => match type_str.as_str() {
                            "string" => Some(serde_json::Value::String("".to_string())),
                            "number" | "integer" => Some(serde_json::Value::Number(0.into())),
                            "boolean" => Some(serde_json::Value::Bool(false)),
                            _ => None,
                        },
                        Some(serde_json::Value::Array(type_array)) => {
                            if type_array
                                .iter()
                                .any(|t| t.as_str().is_some_and(|t| t == "null"))
                            {
                                Some(serde_json::Value::Null)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    }
                } else {
                    None
                };

                if let Some(var_default) = var_default {
                    defaults.insert(var_name.clone(), ValueRef::new(var_default));
                } else {
                    debug!(
                        "Variable '{}' has no default and is not required; no default value inferred.",
                        var_name
                    );
                }
            }
        }

        let secrets = Secrets::from_schema(schema_value);
        Self {
            schema,
            variables,
            defaults,
            secrets,
            required,
        }
    }

    pub fn secrets(&self) -> &Secrets {
        &self.secrets
    }

    /// Return variable names from the schema properties.
    pub fn variables(&self) -> &'_ [String] {
        &self.variables
    }

    /// Get the list of required variables.
    pub fn required_variables(&self) -> impl Iterator<Item = &'_ str> + '_ {
        // Variables in `required` that don't have a default value.
        self.required.iter().map(|s| s.as_ref())
    }

    /// Get the default value for a variable, if specified.
    pub fn default_value(&self, variable_name: &str) -> Option<ValueRef> {
        self.defaults.get(variable_name).cloned()
    }

    /// Validate that provided variable values match the schema requirements.
    pub fn validate_variables(
        &self,
        variables: &HashMap<String, serde_json::Value>,
    ) -> Result<(), VariableValidationError> {
        // Check that all required variables are provided
        for required_var in self.required_variables() {
            if !variables.contains_key(required_var) {
                return Err(VariableValidationError::MissingVariable(
                    required_var.to_string(),
                ));
            }
        }

        // TODO: Add JSON Schema validation for variable values
        // This would require integrating with a JSON Schema validation library

        Ok(())
    }
}

/// Errors that can occur during variable validation.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum VariableValidationError {
    #[error("Missing required variable: {0}")]
    MissingVariable(String),
    #[error("Invalid variable value for '{variable}': {message}")]
    InvalidValue { variable: String, message: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_variable_schema_creation() {
        let schema_json = json!({
            "type": "object",
            "properties": {
                "api_key": {
                    "type": "string",
                    "is_secret": true,
                    "description": "API key for external service"
                },
                "temperature": {
                    "type": "number",
                    "default": 0.7,
                    "minimum": 0,
                    "maximum": 2
                }
            },
            "required": ["api_key"]
        });

        let schema = SchemaRef::parse_json(&schema_json.to_string()).unwrap();
        let var_schema = VariableSchema::new(schema);

        let variable_names = var_schema.variables();
        assert_eq!(variable_names.len(), 2);
        assert!(variable_names.contains(&"api_key".to_string()));
        assert!(variable_names.contains(&"temperature".to_string()));

        let required: Vec<_> = var_schema.required_variables().collect();
        assert_eq!(required, vec!["api_key"]);

        assert!(var_schema.secrets.field("api_key").is_secret());
        assert!(!var_schema.secrets.field("temperature").is_secret());

        assert_eq!(
            var_schema
                .default_value("temperature")
                .map(|v| v.clone_value()),
            Some(json!(0.7))
        );
        assert_eq!(var_schema.default_value("api_key"), None);
    }

    #[test]
    fn test_variable_validation() {
        let schema_json = json!({
            "type": "object",
            "properties": {
                "api_key": { "type": "string" },
                "temperature": { "type": "number", "default": 0.7 }
            },
            "required": ["api_key"]
        });

        let schema = SchemaRef::parse_json(&schema_json.to_string()).unwrap();
        let var_schema = VariableSchema::new(schema);

        // Valid variables
        let mut variables = HashMap::new();
        variables.insert("api_key".to_string(), json!("test-key"));
        variables.insert("temperature".to_string(), json!(0.8));
        assert!(var_schema.validate_variables(&variables).is_ok());

        // Missing required variable
        let mut missing_required = HashMap::new();
        missing_required.insert("temperature".to_string(), json!(0.8));
        match var_schema.validate_variables(&missing_required) {
            Err(VariableValidationError::MissingVariable(var)) => {
                assert_eq!(var, "api_key");
            }
            _ => panic!("Expected missing variable error"),
        }

        // Optional variable missing is OK
        let mut only_required = HashMap::new();
        only_required.insert("api_key".to_string(), json!("test-key"));
        assert!(var_schema.validate_variables(&only_required).is_ok());
    }

    #[test]
    fn test_default_variable_schema() {
        let default_schema = VariableSchema::default();
        assert!(default_schema.variables().is_empty());
        assert_eq!(default_schema.required_variables().count(), 0);
    }

    #[test]
    fn test_default_value() {
        let schema_json = json!({
            "type": "object",
            "properties": {
                "default_bool": { "type": "boolean", "default": true },
                "default_str": { "type": "string", "default": "hello" },
                "default_num": { "type": "number", "default": 3.15 },
                "optional_bool": { "type": "boolean" },
                "optional_str": { "type": "string" },
                "optional_num": { "type": "number" },
                "optional_str_or_none": { "type": ["string", "null"] },
                "required_str": { "type": "string" },
            },
            "required": ["required_str"]
        });

        let schema = SchemaRef::parse_json(&schema_json.to_string()).unwrap();
        let variable_schema = VariableSchema::new(schema);

        assert_eq!(
            variable_schema
                .default_value("default_bool")
                .map(|v| v.clone_value()),
            Some(json!(true))
        );
        assert_eq!(
            variable_schema
                .default_value("default_str")
                .map(|v| v.clone_value()),
            Some(json!("hello"))
        );
        assert_eq!(
            variable_schema
                .default_value("default_num")
                .map(|v| v.clone_value()),
            Some(json!(3.15))
        );
        assert_eq!(
            variable_schema
                .default_value("optional_bool")
                .map(|v| v.clone_value()),
            Some(json!(false))
        );
        assert_eq!(
            variable_schema
                .default_value("optional_str")
                .map(|v| v.clone_value()),
            Some(json!(""))
        );
        assert_eq!(
            variable_schema
                .default_value("optional_num")
                .map(|v| v.clone_value()),
            Some(json!(0))
        );
        assert_eq!(
            variable_schema
                .default_value("optional_str_or_none")
                .map(|v| v.clone_value()),
            Some(serde_json::Value::Null)
        );
        assert_eq!(variable_schema.default_value("required_str"), None);
    }
}
