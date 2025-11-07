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

use indexmap::IndexMap;
use schemars::JsonSchema;

use super::ValueTemplate;

/// Override values for a specific step's inputs.
///
/// This struct uses IndexMap to preserve insertion order for deterministic serialization.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct StepOverrides {
    /// Map from input field name to override value.
    pub input_overrides: IndexMap<String, ValueTemplate>,
}

impl StepOverrides {
    /// Create a new empty StepOverrides.
    pub fn new() -> Self {
        Self {
            input_overrides: IndexMap::new(),
        }
    }

    /// Add an override for a specific input field.
    pub fn override_input(mut self, field: impl Into<String>, value: ValueTemplate) -> Self {
        self.input_overrides.insert(field.into(), value);
        self
    }

    /// Get an override value for a specific input field.
    pub fn get_override(&self, field: &str) -> Option<&ValueTemplate> {
        self.input_overrides.get(field)
    }

    /// Check if there are any overrides defined.
    pub fn is_empty(&self) -> bool {
        self.input_overrides.is_empty()
    }

    /// Get all override field names.
    pub fn field_names(&self) -> impl Iterator<Item = &str> {
        self.input_overrides.keys().map(|s| s.as_str())
    }
}

impl Default for StepOverrides {
    fn default() -> Self {
        Self::new()
    }
}

/// Override values for multiple steps in a workflow.
///
/// This struct uses IndexMap to preserve insertion order for deterministic serialization.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowOverrides {
    /// Map from step ID to step-specific input overrides.
    pub step_overrides: IndexMap<String, StepOverrides>,
}

impl WorkflowOverrides {
    /// Create a new empty WorkflowOverrides.
    pub fn new() -> Self {
        Self {
            step_overrides: IndexMap::new(),
        }
    }

    /// Add overrides for a specific step.
    pub fn override_step(mut self, step_id: impl Into<String>, overrides: StepOverrides) -> Self {
        self.step_overrides.insert(step_id.into(), overrides);
        self
    }

    /// Get overrides for a specific step.
    pub fn get_step_overrides(&self, step_id: &str) -> Option<&StepOverrides> {
        self.step_overrides.get(step_id)
    }

    /// Check if there are any overrides defined for any step.
    pub fn is_empty(&self) -> bool {
        self.step_overrides.is_empty()
    }

    /// Get all step IDs that have overrides.
    pub fn step_ids(&self) -> impl Iterator<Item = &str> {
        self.step_overrides.keys().map(|s| s.as_str())
    }

    /// Check if a specific step has any overrides.
    pub fn has_step_overrides(&self, step_id: &str) -> bool {
        self.step_overrides
            .get(step_id)
            .map(|overrides| !overrides.is_empty())
            .unwrap_or(false)
    }
}

impl Default for WorkflowOverrides {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_step_overrides_creation() {
        let overrides = StepOverrides::new()
            .override_input("temperature", ValueTemplate::literal(json!(0.5)))
            .override_input("model", ValueTemplate::literal(json!("gpt-4")));

        assert_eq!(overrides.input_overrides.len(), 2);
        assert!(overrides.get_override("temperature").is_some());
        assert!(overrides.get_override("model").is_some());
        assert!(overrides.get_override("nonexistent").is_none());
    }

    #[test]
    fn test_workflow_overrides_creation() {
        let step1_overrides =
            StepOverrides::new().override_input("param1", ValueTemplate::literal(json!("value1")));

        let step2_overrides =
            StepOverrides::new().override_input("param2", ValueTemplate::literal(json!(42)));

        let workflow_overrides = WorkflowOverrides::new()
            .override_step("step1", step1_overrides)
            .override_step("step2", step2_overrides);

        assert_eq!(workflow_overrides.step_overrides.len(), 2);
        assert!(workflow_overrides.has_step_overrides("step1"));
        assert!(workflow_overrides.has_step_overrides("step2"));
        assert!(!workflow_overrides.has_step_overrides("step3"));
    }

    #[test]
    fn test_serialization_order_preservation() {
        let overrides = WorkflowOverrides::new()
            .override_step(
                "z_step",
                StepOverrides::new()
                    .override_input("z_param", ValueTemplate::literal(json!("z_value")))
                    .override_input("a_param", ValueTemplate::literal(json!("a_value"))),
            )
            .override_step(
                "a_step",
                StepOverrides::new()
                    .override_input("param", ValueTemplate::literal(json!("value"))),
            );

        // Serialize to JSON
        let json_str = serde_json::to_string_pretty(&overrides).unwrap();

        // The order should be preserved (z_step before a_step, z_param before a_param)
        let json_lines: Vec<&str> = json_str.lines().collect();
        let z_step_line = json_lines
            .iter()
            .position(|line| line.contains("z_step"))
            .unwrap();
        let a_step_line = json_lines
            .iter()
            .position(|line| line.contains("a_step"))
            .unwrap();
        assert!(
            z_step_line < a_step_line,
            "Order should be preserved in JSON serialization"
        );
    }

    #[test]
    fn test_yaml_serialization() {
        let overrides = WorkflowOverrides::new()
            .override_step(
                "llm_call",
                StepOverrides::new()
                    .override_input("temperature", ValueTemplate::literal(json!(0.1)))
                    .override_input("model", ValueTemplate::literal(json!("gpt-4"))),
            )
            .override_step(
                "data_processing",
                StepOverrides::new()
                    .override_input("batch_size", ValueTemplate::literal(json!(32))),
            );

        let yaml_str = serde_yaml_ng::to_string(&overrides).unwrap();
        println!("YAML serialization:\n{}", yaml_str);

        // Test round-trip
        let deserialized: WorkflowOverrides = serde_yaml_ng::from_str(&yaml_str).unwrap();
        assert_eq!(deserialized, overrides);
    }

    #[test]
    fn test_complex_override_values() {
        let complex_value = json!({
            "config": {
                "temperature": 0.7,
                "max_tokens": 1000
            },
            "system_prompt": "You are a helpful assistant"
        });

        let overrides = StepOverrides::new().override_input(
            "complex_param",
            ValueTemplate::literal(complex_value.clone()),
        );

        // Test serialization and deserialization
        let json_str = serde_json::to_string(&overrides).unwrap();
        let deserialized: StepOverrides = serde_json::from_str(&json_str).unwrap();

        if let Some(override_value) = deserialized.get_override("complex_param") {
            // The override value should preserve the complex structure
            match override_value.as_ref() {
                crate::values::ValueTemplateRepr::Object(obj) => {
                    assert!(obj.contains_key("config"));
                    assert!(obj.contains_key("system_prompt"));
                }
                _ => panic!("Expected object value template"),
            }
        } else {
            panic!("Override value should be present");
        }
    }

    #[test]
    fn test_empty_overrides() {
        let empty_step = StepOverrides::new();
        assert!(empty_step.is_empty());
        assert_eq!(empty_step.field_names().count(), 0);

        let empty_workflow = WorkflowOverrides::new();
        assert!(empty_workflow.is_empty());
        assert_eq!(empty_workflow.step_ids().count(), 0);
    }
}
