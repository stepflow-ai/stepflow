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

use serde::{Deserialize, Serialize};

use crate::DiagnosticLevel;

/// Specific diagnostic message with context
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum DiagnosticMessage {
    // Fatal diagnostics (prevent analysis)
    #[serde(rename_all = "camelCase")]
    DuplicateStepId { step_id: String },
    #[serde(rename_all = "camelCase")]
    EmptyStepId,
    #[serde(rename_all = "camelCase")]
    SelfReference { step_id: String },
    #[serde(rename_all = "camelCase")]
    UndefinedStepReference {
        from_step: Option<String>,
        referenced_step: String,
    },
    #[serde(rename_all = "camelCase")]
    InvalidReferenceExpression {
        step_id: Option<String>,
        field: Option<String>,
        error: String,
    },

    // Error diagnostics (will fail during execution)
    #[serde(rename_all = "camelCase")]
    InvalidFieldAccess {
        step_id: String,
        field: String,
        reason: String,
    },
    #[serde(rename_all = "camelCase")]
    InvalidComponent {
        step_id: String,
        component: String,
        error: Cow<'static, str>,
    },
    #[serde(rename_all = "camelCase")]
    EmptyComponentName { step_id: String },
    #[serde(rename_all = "camelCase")]
    SchemaViolation {
        step_id: String,
        field: String,
        violation: String,
    },
    #[serde(rename_all = "camelCase")]
    InvalidSubflowLiteral { error: String },

    // Warning diagnostics (potential issues)
    #[serde(rename_all = "camelCase")]
    MockComponent { step_id: String },
    #[serde(rename_all = "camelCase")]
    UnreachableStep { step_id: String },
    #[serde(rename_all = "camelCase")]
    MissingFlowName,
    #[serde(rename_all = "camelCase")]
    MissingFlowDescription,
    #[serde(rename_all = "camelCase")]
    UnvalidatedFieldAccess {
        step_id: String,
        field: String,
        reason: String,
    },

    // Variable validation diagnostics (Error level)
    #[serde(rename_all = "camelCase")]
    UndefinedVariable { variable: String, context: String },
    #[serde(rename_all = "camelCase")]
    UndefinedRequiredVariable { variable: String, context: String },

    // Schema availability warnings (Warning level)
    #[serde(rename_all = "camelCase")]
    MissingVariableSchema,

    // Configuration validation diagnostics
    #[serde(rename_all = "camelCase")]
    NoPluginsConfigured,
    #[serde(rename_all = "camelCase")]
    NoRoutingRulesConfigured,
    #[serde(rename_all = "camelCase")]
    InvalidRouteReference {
        route_path: String,
        rule_index: usize,
        plugin: String,
    },
    #[serde(rename_all = "camelCase")]
    UnusedPlugin { plugin: String },
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
