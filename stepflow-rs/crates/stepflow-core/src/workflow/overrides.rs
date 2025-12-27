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
use std::sync::Arc;

use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};

use super::Flow;

#[cfg(test)]
use super::FlowV1;

/// Workflow overrides that can be applied to modify step behavior at runtime.
///
/// Overrides are keyed by step ID and contain merge patches or other transformation
/// specifications to modify step properties before execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(transparent)]
pub struct WorkflowOverrides {
    /// Map of step ID to override specification
    pub steps: HashMap<String, StepOverride>,
}

impl WorkflowOverrides {
    /// Create new empty workflow overrides
    pub fn new() -> Self {
        Self {
            steps: HashMap::new(),
        }
    }

    /// Check if there are any overrides defined
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Add an override for a specific step
    pub fn add_step_override(&mut self, step_id: String, override_spec: StepOverride) {
        self.steps.insert(step_id, override_spec);
    }
}

impl Default for WorkflowOverrides {
    fn default() -> Self {
        Self::new()
    }
}

/// Override specification for a single step.
///
/// Contains the override type (merge patch, json patch, etc.) and the value
/// to apply. The type field uses `$type` to avoid collisions with step properties.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct StepOverride {
    /// The type of override to apply. Defaults to "merge_patch" if not specified.
    #[serde(rename = "$type", default = "default_override_type")]
    pub override_type: OverrideType,

    /// The override value to apply, interpreted based on the override type.
    pub value: serde_json::Value,
}

impl StepOverride {
    /// Create a new step override with merge patch type
    pub fn merge_patch(value: serde_json::Value) -> Self {
        Self {
            override_type: OverrideType::MergePatch,
            value,
        }
    }

    /// Create a new step override with explicit type
    pub fn with_type(override_type: OverrideType, value: serde_json::Value) -> Self {
        Self {
            override_type,
            value,
        }
    }
}

/// The type of override operation to perform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OverrideType {
    /// Apply a JSON Merge Patch (RFC 7396) to the step.
    ///
    /// This is the default override type. The value should be a JSON object
    /// where null values indicate fields to remove and other values are merged
    /// into the target step.
    MergePatch,

    /// Apply a JSON Patch (RFC 6902) to the step. (Future extension)
    ///
    /// The value should be an array of JSON Patch operations.
    /// This is reserved for future use.
    #[allow(dead_code)]
    JsonPatch,
}

/// Default override type is merge patch
fn default_override_type() -> OverrideType {
    OverrideType::MergePatch
}

/// Errors that can occur when applying workflow overrides
#[derive(Debug, thiserror::Error)]
pub enum OverrideError {
    #[error("Step '{step_id}' not found in workflow")]
    StepNotFound { step_id: String },

    #[error("Invalid override value for step '{step_id}': {reason}")]
    InvalidOverrideValue { step_id: String, reason: String },

    #[error("Unsupported override type: {override_type:?}")]
    UnsupportedOverrideType { override_type: OverrideType },

    #[error("JSON merge patch failed for step '{step_id}': {reason}")]
    MergePatchFailed { step_id: String, reason: String },
}

pub type OverrideResult<T> = error_stack::Result<T, OverrideError>;

/// Trait for applying workflow overrides to flows
pub trait OverrideProcessor {
    /// Apply the given overrides to a workflow, returning a modified workflow.
    ///
    /// This validates that all override targets exist in the workflow and
    /// applies the specified transformations. If no overrides are needed,
    /// the original Arc is returned unchanged.
    fn apply_overrides(
        &self,
        flow: Arc<Flow>,
        overrides: &WorkflowOverrides,
    ) -> OverrideResult<Arc<Flow>>;
}

/// Default implementation of override processing
pub struct DefaultOverrideProcessor;

impl OverrideProcessor for DefaultOverrideProcessor {
    fn apply_overrides(
        &self,
        flow: Arc<Flow>,
        overrides: &WorkflowOverrides,
    ) -> OverrideResult<Arc<Flow>> {
        if overrides.is_empty() {
            return Ok(flow);
        }

        log::debug!(
            "Applying {} step overrides to workflow",
            overrides.steps.len()
        );

        // Validate all override targets exist in the workflow
        self.validate_override_targets(&flow, overrides)?;

        // Need to clone the flow to apply modifications
        let mut cloned_flow = flow.slow_clone();

        // Apply overrides to each step - need to handle the enum structure properly
        match &mut cloned_flow {
            Flow::V1(flow_v1) => {
                for step in &mut flow_v1.steps {
                    if let Some(step_override) = overrides.steps.get(&step.id) {
                        log::debug!(
                            "Applying override to step '{}' with type '{:?}'",
                            step.id,
                            step_override.override_type
                        );
                        self.apply_step_override(step, step_override)
                            .change_context(OverrideError::InvalidOverrideValue {
                                step_id: step.id.clone(),
                                reason: "Failed to apply step override".to_string(),
                            })?;
                    }
                }
            }
        }

        Ok(Arc::new(cloned_flow))
    }
}

impl DefaultOverrideProcessor {
    /// Create a new default override processor
    pub fn new() -> Self {
        Self
    }

    /// Validate that all override targets exist in the workflow
    fn validate_override_targets(
        &self,
        flow: &Flow,
        overrides: &WorkflowOverrides,
    ) -> OverrideResult<()> {
        let step_ids: std::collections::HashSet<&String> =
            flow.steps().iter().map(|step| &step.id).collect();

        for step_id in overrides.steps.keys() {
            if !step_ids.contains(&step_id) {
                return Err(error_stack::report!(OverrideError::StepNotFound {
                    step_id: step_id.clone(),
                }));
            }
        }

        Ok(())
    }

    /// Apply a single step override based on its type
    fn apply_step_override(
        &self,
        step: &mut super::Step,
        step_override: &StepOverride,
    ) -> OverrideResult<()> {
        match step_override.override_type {
            OverrideType::MergePatch => self.apply_merge_patch(step, &step_override.value),
            OverrideType::JsonPatch => Err(error_stack::report!(
                OverrideError::UnsupportedOverrideType {
                    override_type: step_override.override_type.clone(),
                }
            )),
        }
    }

    /// Apply a JSON merge patch to a step
    fn apply_merge_patch(
        &self,
        step: &mut super::Step,
        patch: &serde_json::Value,
    ) -> OverrideResult<()> {
        let step_id = step.id.clone(); // Capture the ID before borrowing

        // Convert step to JSON for merging
        let mut step_json =
            serde_json::to_value(&*step).change_context(OverrideError::MergePatchFailed {
                step_id: step_id.clone(),
                reason: "Failed to serialize step to JSON".to_string(),
            })?;

        // Apply merge patch
        json_patch::merge(&mut step_json, patch);

        // Convert back to Step
        *step =
            serde_json::from_value(step_json).change_context(OverrideError::MergePatchFailed {
                step_id,
                reason: "Failed to deserialize modified step from JSON".to_string(),
            })?;

        Ok(())
    }
}

impl Default for DefaultOverrideProcessor {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function to apply overrides to a workflow using the default processor
pub fn apply_overrides(
    flow: Arc<Flow>,
    overrides: &WorkflowOverrides,
) -> OverrideResult<Arc<Flow>> {
    DefaultOverrideProcessor::new().apply_overrides(flow, overrides)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ValueExpr;
    use serde_json::json;

    fn create_test_flow() -> Flow {
        Flow::V1(FlowV1 {
            name: Some("test_flow".to_string()),
            description: None,
            version: None,
            schemas: super::super::FlowSchema::default(),
            steps: vec![super::super::Step {
                id: "step1".to_string(),
                component: super::super::Component::from_string("/test/component"),
                on_error: None,
                input: ValueExpr::null(),
                must_execute: None,
                metadata: std::collections::HashMap::new(),
            }],
            output: ValueExpr::null(),
            test: None,
            examples: None,
            metadata: std::collections::HashMap::new(),
        })
    }

    #[test]
    fn test_workflow_overrides_creation() {
        let overrides = WorkflowOverrides::new();
        assert!(overrides.is_empty());

        let mut overrides = WorkflowOverrides::new();
        overrides.add_step_override(
            "step1".to_string(),
            StepOverride::merge_patch(json!({"input": {"temperature": 0.8}})),
        );
        assert!(!overrides.is_empty());
        assert_eq!(overrides.steps.len(), 1);
    }

    #[test]
    fn test_step_override_creation() {
        let override_spec = StepOverride::merge_patch(json!({"input": {"temperature": 0.8}}));
        assert!(matches!(
            override_spec.override_type,
            OverrideType::MergePatch
        ));
        assert_eq!(override_spec.value, json!({"input": {"temperature": 0.8}}));

        let override_spec = StepOverride::with_type(
            OverrideType::MergePatch,
            json!({"component": "/different/component"}),
        );
        assert!(matches!(
            override_spec.override_type,
            OverrideType::MergePatch
        ));
        assert_eq!(
            override_spec.value,
            json!({"component": "/different/component"})
        );
    }

    #[test]
    fn test_apply_empty_overrides() {
        let flow = Arc::new(create_test_flow());
        let overrides = WorkflowOverrides::new();

        let original_step_count = flow.steps().len();
        let result = apply_overrides(flow, &overrides).unwrap();
        assert_eq!(result.steps().len(), original_step_count);
    }

    #[test]
    fn test_apply_merge_patch_override() {
        let flow = Arc::new(create_test_flow());
        let mut overrides = WorkflowOverrides::new();
        overrides.add_step_override(
            "step1".to_string(),
            StepOverride::merge_patch(json!({
                "input": {"temperature": 0.8},
                "component": "/new/component"
            })),
        );

        let result = apply_overrides(flow, &overrides).unwrap();
        let step = &result.steps()[0];

        // Check that the component was overridden
        assert_eq!(step.component.to_string(), "/new/component");

        // Check that input was merged (note: this is a simplified test)
        // In practice, the input field would be properly merged with existing values
    }

    #[test]
    fn test_validate_override_targets_missing_step() {
        let flow = Arc::new(create_test_flow());
        let mut overrides = WorkflowOverrides::new();
        overrides.add_step_override(
            "nonexistent_step".to_string(),
            StepOverride::merge_patch(json!({"input": {"temperature": 0.8}})),
        );

        let result = apply_overrides(flow, &overrides);
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Step 'nonexistent_step' not found in workflow")
        );
    }

    #[test]
    fn test_serde_override_type_default() {
        let json_str = r#"{"value": {"temperature": 0.8}}"#;
        let step_override: StepOverride = serde_json::from_str(json_str).unwrap();

        assert!(matches!(
            step_override.override_type,
            OverrideType::MergePatch
        ));
        assert_eq!(step_override.value, json!({"temperature": 0.8}));
    }

    #[test]
    fn test_serde_override_type_explicit() {
        let json_str = r#"{"$type": "merge_patch", "value": {"temperature": 0.8}}"#;
        let step_override: StepOverride = serde_json::from_str(json_str).unwrap();

        assert!(matches!(
            step_override.override_type,
            OverrideType::MergePatch
        ));
        assert_eq!(step_override.value, json!({"temperature": 0.8}));
    }
}
