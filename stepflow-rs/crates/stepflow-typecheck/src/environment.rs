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

//! Type environment for tracking types during type checking.

use indexmap::IndexMap;
use stepflow_core::schema::SchemaRef;
use stepflow_core::workflow::Flow;

use crate::Type;

/// Type environment tracking types for all named entities in scope.
///
/// The environment is built incrementally as steps are type-checked:
/// - Input schema is set from the flow definition
/// - Variable schemas are extracted from the flow's variable schema
/// - Step output types are added as each step is processed
#[derive(Debug, Clone, Default)]
pub struct TypeEnvironment {
    /// Input schema for the workflow.
    input_type: Type,

    /// Variable schemas (from flow.variables).
    variable_types: IndexMap<String, Type>,

    /// Synthesized output types for each step, keyed by step ID.
    step_types: IndexMap<String, Type>,
}

impl TypeEnvironment {
    /// Create a new empty environment.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an environment from a Flow.
    ///
    /// Extracts the input schema and variable schemas from the flow definition.
    pub fn from_flow(flow: &Flow) -> Self {
        let input_type = Type::from_option(flow.schemas.input.clone());

        let variable_types = flow
            .schemas
            .variables
            .as_ref()
            .map(extract_variable_types)
            .unwrap_or_default();

        Self {
            input_type,
            variable_types,
            step_types: IndexMap::new(),
        }
    }

    /// Get the workflow input type.
    pub fn input_type(&self) -> &Type {
        &self.input_type
    }

    /// Set the workflow input type.
    pub fn set_input_type(&mut self, ty: Type) {
        self.input_type = ty;
    }

    /// Get a variable's type.
    pub fn get_variable_type(&self, name: &str) -> Option<&Type> {
        self.variable_types.get(name)
    }

    /// Set a variable's type.
    pub fn set_variable_type(&mut self, name: String, ty: Type) {
        self.variable_types.insert(name, ty);
    }

    /// Get the type for a step output.
    pub fn get_step_type(&self, step_id: &str) -> Option<&Type> {
        self.step_types.get(step_id)
    }

    /// Set the type for a step output.
    ///
    /// This should be called after type-checking each step, adding
    /// its inferred output type to the environment.
    pub fn set_step_type(&mut self, step_id: String, ty: Type) {
        self.step_types.insert(step_id, ty);
    }

    /// Check if a step has been processed (has a type in the environment).
    pub fn has_step(&self, step_id: &str) -> bool {
        self.step_types.contains_key(step_id)
    }

    /// Get all step types.
    pub fn step_types(&self) -> &IndexMap<String, Type> {
        &self.step_types
    }

    /// Get all variable types.
    pub fn variable_types(&self) -> &IndexMap<String, Type> {
        &self.variable_types
    }
}

/// Extract per-variable types from a variable schema.
fn extract_variable_types(schema: &SchemaRef) -> IndexMap<String, Type> {
    let mut result = IndexMap::new();

    let schema_value = schema.as_value();

    if let Some(props) = schema_value.get("properties").and_then(|p| p.as_object()) {
        for (name, prop_schema) in props {
            let schema_ref = SchemaRef::from(prop_schema.clone());
            result.insert(name.clone(), Type::Schema(schema_ref));
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_environment() {
        let env = TypeEnvironment::new();
        assert!(env.input_type().is_any());
        assert!(env.step_types().is_empty());
        assert!(env.variable_types().is_empty());
    }

    #[test]
    fn test_set_and_get_step_type() {
        let mut env = TypeEnvironment::new();

        let string_type = Type::from_json(serde_json::json!({"type": "string"}));
        env.set_step_type("step1".to_string(), string_type.clone());

        assert!(env.has_step("step1"));
        assert!(!env.has_step("step2"));
        assert_eq!(env.get_step_type("step1"), Some(&string_type));
        assert_eq!(env.get_step_type("step2"), None);
    }

    #[test]
    fn test_set_and_get_variable_type() {
        let mut env = TypeEnvironment::new();

        let number_type = Type::from_json(serde_json::json!({"type": "number"}));
        env.set_variable_type("temperature".to_string(), number_type.clone());

        assert_eq!(env.get_variable_type("temperature"), Some(&number_type));
        assert_eq!(env.get_variable_type("unknown"), None);
    }

    #[test]
    fn test_set_input_type() {
        let mut env = TypeEnvironment::new();
        assert!(env.input_type().is_any());

        let object_type = Type::from_json(serde_json::json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        }));
        env.set_input_type(object_type.clone());

        assert_eq!(env.input_type(), &object_type);
    }
}
