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

use std::borrow::Cow;

use serde::{Serialize, Serializer};
use strum::IntoStaticStr;

use crate::DiagnosticLevel;

/// Specific diagnostic message with context.
///
/// Serializes to a stable format with three fields:
/// - `formatted`: Human-readable message text
/// - `kind`: The diagnostic variant name in camelCase (e.g., "duplicateStepId")
/// - `data`: Structured data fields for this diagnostic (camelCase keys)
#[derive(Debug, Clone, PartialEq, Eq, IntoStaticStr)]
#[strum(serialize_all = "camelCase")]
pub enum DiagnosticMessage {
    // Fatal diagnostics (prevent analysis)
    DuplicateStepId {
        step_id: String,
    },
    EmptyStepId,
    SelfReference {
        step_id: String,
    },
    UndefinedStepReference {
        from_step: Option<String>,
        referenced_step: String,
    },
    InvalidReferenceExpression {
        step_id: Option<String>,
        field: Option<String>,
        error: String,
    },

    // Error diagnostics (will fail during execution)
    InvalidFieldAccess {
        step_id: String,
        field: String,
        reason: String,
    },
    InvalidComponent {
        step_id: String,
        component: String,
        error: Cow<'static, str>,
    },
    EmptyComponentName {
        step_id: String,
    },
    SchemaViolation {
        step_id: String,
        field: String,
        violation: String,
    },
    InvalidSubflowLiteral {
        error: String,
    },

    // Warning diagnostics (potential issues)
    MockComponent {
        step_id: String,
    },
    UnreachableStep {
        step_id: String,
    },
    MissingFlowName,
    MissingFlowDescription,
    UnvalidatedFieldAccess {
        step_id: String,
        field: String,
        reason: String,
    },

    // Variable validation diagnostics (Error level)
    UndefinedVariable {
        variable: String,
        context: String,
    },
    UndefinedRequiredVariable {
        variable: String,
        context: String,
    },

    // Schema availability warnings (Warning level)
    MissingVariableSchema,

    // Type checking diagnostics
    TypeMismatch {
        step_id: String,
        expected: String,
        actual: String,
        detail: String,
    },
    UntypedComponentOutput {
        step_id: String,
        component: String,
    },
    UnknownPropertyInPath {
        step_id: Option<String>,
        path: String,
        property: String,
    },
    TypeCheckIndeterminate {
        step_id: Option<String>,
        reason: String,
    },

    // Configuration validation diagnostics
    NoPluginsConfigured,
    NoRoutingRulesConfigured,
    InvalidRouteReference {
        route_path: String,
        rule_index: usize,
        plugin: String,
    },
    UnusedPlugin {
        plugin: String,
    },
}

impl DiagnosticMessage {
    /// Get the kind (variant name) in camelCase
    fn kind(&self) -> &'static str {
        self.into()
    }

    /// Get the data fields as a JSON object with camelCase keys.
    /// Returns None for unit variants.
    fn data(&self) -> Option<serde_json::Value> {
        use serde_json::json;

        match self {
            DiagnosticMessage::DuplicateStepId { step_id } => Some(json!({ "stepId": step_id })),
            DiagnosticMessage::EmptyStepId => None,
            DiagnosticMessage::SelfReference { step_id } => Some(json!({ "stepId": step_id })),
            DiagnosticMessage::UndefinedStepReference {
                from_step,
                referenced_step,
            } => Some(json!({ "fromStep": from_step, "referencedStep": referenced_step })),
            DiagnosticMessage::InvalidReferenceExpression {
                step_id,
                field,
                error,
            } => Some(json!({ "stepId": step_id, "field": field, "error": error })),
            DiagnosticMessage::InvalidFieldAccess {
                step_id,
                field,
                reason,
            } => Some(json!({ "stepId": step_id, "field": field, "reason": reason })),
            DiagnosticMessage::InvalidComponent {
                step_id,
                component,
                error,
            } => {
                Some(json!({ "stepId": step_id, "component": component, "error": error.as_ref() }))
            }
            DiagnosticMessage::EmptyComponentName { step_id } => Some(json!({ "stepId": step_id })),
            DiagnosticMessage::SchemaViolation {
                step_id,
                field,
                violation,
            } => Some(json!({ "stepId": step_id, "field": field, "violation": violation })),
            DiagnosticMessage::InvalidSubflowLiteral { error } => Some(json!({ "error": error })),
            DiagnosticMessage::MockComponent { step_id } => Some(json!({ "stepId": step_id })),
            DiagnosticMessage::UnreachableStep { step_id } => Some(json!({ "stepId": step_id })),
            DiagnosticMessage::MissingFlowName => None,
            DiagnosticMessage::MissingFlowDescription => None,
            DiagnosticMessage::UnvalidatedFieldAccess {
                step_id,
                field,
                reason,
            } => Some(json!({ "stepId": step_id, "field": field, "reason": reason })),
            DiagnosticMessage::UndefinedVariable { variable, context } => {
                Some(json!({ "variable": variable, "context": context }))
            }
            DiagnosticMessage::UndefinedRequiredVariable { variable, context } => {
                Some(json!({ "variable": variable, "context": context }))
            }
            DiagnosticMessage::MissingVariableSchema => None,
            DiagnosticMessage::TypeMismatch {
                step_id,
                expected,
                actual,
                detail,
            } => Some(
                json!({ "stepId": step_id, "expected": expected, "actual": actual, "detail": detail }),
            ),
            DiagnosticMessage::UntypedComponentOutput { step_id, component } => {
                Some(json!({ "stepId": step_id, "component": component }))
            }
            DiagnosticMessage::UnknownPropertyInPath {
                step_id,
                path,
                property,
            } => Some(json!({ "stepId": step_id, "path": path, "property": property })),
            DiagnosticMessage::TypeCheckIndeterminate { step_id, reason } => {
                Some(json!({ "stepId": step_id, "reason": reason }))
            }
            DiagnosticMessage::NoPluginsConfigured => None,
            DiagnosticMessage::NoRoutingRulesConfigured => None,
            DiagnosticMessage::InvalidRouteReference {
                route_path,
                rule_index,
                plugin,
            } => {
                Some(json!({ "routePath": route_path, "ruleIndex": rule_index, "plugin": plugin }))
            }
            DiagnosticMessage::UnusedPlugin { plugin } => Some(json!({ "plugin": plugin })),
        }
    }
}

impl Serialize for DiagnosticMessage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap as _;

        let data = self.data();
        let map_len = if data.is_some() { 3 } else { 2 };
        let mut map = serializer.serialize_map(Some(map_len))?;

        map.serialize_entry("formatted", &self.message())?;
        map.serialize_entry("kind", self.kind())?;
        if let Some(data) = data {
            map.serialize_entry("data", &data)?;
        }

        map.end()
    }
}

// Custom OpenAPI schema: simple object with formatted, kind, and data
impl utoipa::PartialSchema for DiagnosticMessage {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::schema::AdditionalProperties;
        use utoipa::openapi::*;

        RefOr::T(Schema::Object(
            ObjectBuilder::new()
                .schema_type(schema::SchemaType::Type(schema::Type::Object))
                .description(Some(
                    "A diagnostic message with human-readable text and structured data",
                ))
                .property(
                    "formatted",
                    ObjectBuilder::new()
                        .schema_type(schema::SchemaType::Type(schema::Type::String))
                        .description(Some("Human-readable formatted message")),
                )
                .property(
                    "kind",
                    ObjectBuilder::new()
                        .schema_type(schema::SchemaType::Type(schema::Type::String))
                        .description(Some("The diagnostic kind/variant name")),
                )
                .property(
                    "data",
                    ObjectBuilder::new()
                        .schema_type(schema::SchemaType::Type(schema::Type::Object))
                        .description(Some("Structured data fields for this diagnostic"))
                        .additional_properties(Some(AdditionalProperties::FreeForm(true))),
                )
                .required("formatted")
                .required("kind")
                .build(),
        ))
    }
}

impl utoipa::ToSchema for DiagnosticMessage {
    fn name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("DiagnosticMessage")
    }
}

impl DiagnosticMessage {
    /// Get the severity level for this diagnostic
    pub fn level(&self) -> DiagnosticLevel {
        match self {
            // Fatal diagnostics
            DiagnosticMessage::DuplicateStepId { .. } => DiagnosticLevel::Fatal,
            DiagnosticMessage::EmptyStepId => DiagnosticLevel::Fatal,
            DiagnosticMessage::SelfReference { .. } => DiagnosticLevel::Fatal,
            DiagnosticMessage::UndefinedStepReference { .. } => DiagnosticLevel::Fatal,
            DiagnosticMessage::InvalidReferenceExpression { .. } => DiagnosticLevel::Fatal,

            // Error diagnostics
            DiagnosticMessage::InvalidFieldAccess { .. } => DiagnosticLevel::Error,
            DiagnosticMessage::InvalidComponent { .. } => DiagnosticLevel::Error,
            DiagnosticMessage::EmptyComponentName { .. } => DiagnosticLevel::Error,
            DiagnosticMessage::SchemaViolation { .. } => DiagnosticLevel::Error,
            DiagnosticMessage::UndefinedVariable { .. } => DiagnosticLevel::Error,
            DiagnosticMessage::UndefinedRequiredVariable { .. } => DiagnosticLevel::Error,
            DiagnosticMessage::InvalidSubflowLiteral { .. } => DiagnosticLevel::Error,

            // Type checking diagnostics
            DiagnosticMessage::TypeMismatch { .. } => DiagnosticLevel::Error,
            DiagnosticMessage::UntypedComponentOutput { .. } => DiagnosticLevel::Warning,
            DiagnosticMessage::UnknownPropertyInPath { .. } => DiagnosticLevel::Error,
            DiagnosticMessage::TypeCheckIndeterminate { .. } => DiagnosticLevel::Warning,

            // Warning diagnostics
            DiagnosticMessage::MissingVariableSchema => DiagnosticLevel::Warning,
            DiagnosticMessage::MockComponent { .. } => DiagnosticLevel::Warning,
            DiagnosticMessage::UnreachableStep { .. } => DiagnosticLevel::Warning,
            DiagnosticMessage::MissingFlowName => DiagnosticLevel::Warning,
            DiagnosticMessage::MissingFlowDescription => DiagnosticLevel::Warning,
            DiagnosticMessage::UnvalidatedFieldAccess { .. } => DiagnosticLevel::Warning,

            // Configuration diagnostics
            DiagnosticMessage::NoPluginsConfigured => DiagnosticLevel::Warning,
            DiagnosticMessage::NoRoutingRulesConfigured => DiagnosticLevel::Warning,
            DiagnosticMessage::InvalidRouteReference { .. } => DiagnosticLevel::Error,
            DiagnosticMessage::UnusedPlugin { .. } => DiagnosticLevel::Warning,
        }
    }

    /// Get a human-readable message for this diagnostic
    pub fn message(&self) -> String {
        match self {
            DiagnosticMessage::DuplicateStepId { step_id } => {
                format!("Duplicate step ID: '{step_id}'")
            }
            DiagnosticMessage::EmptyStepId => "Step ID cannot be empty".to_string(),
            DiagnosticMessage::SelfReference { step_id } => {
                format!("Step '{step_id}' cannot reference itself")
            }
            DiagnosticMessage::UndefinedStepReference {
                from_step,
                referenced_step,
            } => match from_step {
                Some(from) => {
                    format!("Step '{from}' references undefined step '{referenced_step}'")
                }
                None => format!("Reference to undefined step '{referenced_step}'"),
            },
            DiagnosticMessage::InvalidReferenceExpression {
                step_id,
                field,
                error,
            } => match (step_id, field) {
                (Some(step), Some(field)) => {
                    format!("Invalid reference in step '{step}' field '{field}': {error}")
                }
                (Some(step), None) => format!("Invalid reference in step '{step}': {error}"),
                (None, Some(field)) => format!("Invalid reference in field '{field}': {error}"),
                (None, None) => format!("Invalid reference: {error}"),
            },
            DiagnosticMessage::InvalidFieldAccess {
                step_id,
                field,
                reason,
            } => {
                format!("Invalid field access '{field}' on step '{step_id}': {reason}")
            }
            DiagnosticMessage::InvalidComponent {
                step_id,
                component,
                error,
            } => {
                format!("Invalid component '{component}' in step '{step_id}': {error}")
            }
            DiagnosticMessage::EmptyComponentName { step_id } => {
                format!("Empty component name in step '{step_id}'")
            }
            DiagnosticMessage::SchemaViolation {
                step_id,
                field,
                violation,
            } => {
                format!("Schema violation in step '{step_id}' field '{field}': {violation}")
            }
            DiagnosticMessage::MockComponent { step_id } => {
                format!(
                    "Step '{step_id}' uses mock component - ensure this is intentional for testing"
                )
            }
            DiagnosticMessage::UnreachableStep { step_id } => {
                format!("Step '{step_id}' is not referenced by any other step or workflow output")
            }
            DiagnosticMessage::MissingFlowName => "Workflow has no name defined".to_string(),
            DiagnosticMessage::MissingFlowDescription => {
                "Workflow has no description defined".to_string()
            }
            DiagnosticMessage::UnvalidatedFieldAccess {
                step_id,
                field,
                reason,
            } => {
                format!("Field access '{field}' on step '{step_id}' cannot be validated: {reason}")
            }
            DiagnosticMessage::UndefinedVariable { variable, context } => {
                format!("Undefined variable '{variable}' referenced in {context}")
            }
            DiagnosticMessage::UndefinedRequiredVariable { variable, context } => {
                format!("Undefined required variable '{variable}' referenced in {context}")
            }
            DiagnosticMessage::InvalidSubflowLiteral { error } => {
                format!("Invalid subflow literal: {error}")
            }

            DiagnosticMessage::MissingVariableSchema => {
                "Workflow has no variable schema defined - variable references cannot be validated"
                    .to_string()
            }
            DiagnosticMessage::TypeMismatch {
                step_id,
                expected,
                actual,
                detail,
            } => {
                format!(
                    "Type mismatch in step '{step_id}': expected {expected}, got {actual}. {detail}"
                )
            }
            DiagnosticMessage::UntypedComponentOutput { step_id, component } => {
                format!(
                    "Component '{component}' in step '{step_id}' does not provide output schema"
                )
            }
            DiagnosticMessage::UnknownPropertyInPath {
                step_id,
                path,
                property,
            } => match step_id {
                Some(step) => {
                    format!("Unknown property '{property}' in path '{path}' in step '{step}'")
                }
                None => format!("Unknown property '{property}' in path '{path}'"),
            },
            DiagnosticMessage::TypeCheckIndeterminate { step_id, reason } => match step_id {
                Some(step) => format!("Cannot determine type in step '{step}': {reason}"),
                None => format!("Cannot determine type: {reason}"),
            },
            DiagnosticMessage::NoPluginsConfigured => "No plugins configured".to_string(),
            DiagnosticMessage::NoRoutingRulesConfigured => {
                "No routing rules configured".to_string()
            }
            DiagnosticMessage::InvalidRouteReference {
                route_path,
                rule_index,
                plugin,
            } => {
                format!(
                    "Routing rule {} for path '{}' references unknown plugin '{}'",
                    rule_index + 1,
                    route_path,
                    plugin
                )
            }
            DiagnosticMessage::UnusedPlugin { plugin } => {
                format!("Plugin '{plugin}' is not referenced by any routing rule")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_message_serialization() {
        let msg = DiagnosticMessage::DuplicateStepId {
            step_id: "foo".to_string(),
        };

        let json = serde_json::to_value(&msg).unwrap();

        assert_eq!(json.get("formatted").unwrap(), "Duplicate step ID: 'foo'");
        assert_eq!(json.get("kind").unwrap(), "duplicateStepId");
        assert_eq!(json.get("data").unwrap().get("stepId").unwrap(), "foo");
    }

    #[test]
    fn test_diagnostic_message_unit_variant_serialization() {
        let msg = DiagnosticMessage::EmptyStepId;

        let json = serde_json::to_value(&msg).unwrap();

        assert_eq!(json.get("formatted").unwrap(), "Step ID cannot be empty");
        assert_eq!(json.get("kind").unwrap(), "emptyStepId");
        // Unit variants have no data field
        assert!(json.get("data").is_none());
    }

    #[test]
    fn test_strum_kind() {
        // Verify strum gives us camelCase variant names
        let msg = DiagnosticMessage::DuplicateStepId {
            step_id: "foo".to_string(),
        };
        assert_eq!(msg.kind(), "duplicateStepId");

        let msg = DiagnosticMessage::EmptyStepId;
        assert_eq!(msg.kind(), "emptyStepId");

        let msg = DiagnosticMessage::InvalidRouteReference {
            route_path: "/foo".to_string(),
            rule_index: 0,
            plugin: "bar".to_string(),
        };
        assert_eq!(msg.kind(), "invalidRouteReference");
    }
}
