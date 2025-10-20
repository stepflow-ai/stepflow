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

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Diagnostic level indicating severity and impact
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    JsonSchema,
    utoipa::ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub enum DiagnosticLevel {
    /// Fatal: Prevents analysis from proceeding
    Fatal = 0,
    /// Error: Will definitely fail during execution
    Error = 1,
    /// Warning: Likely to cause problems during execution
    Warning = 2,
}

impl DiagnosticLevel {
    /// Check if this level blocks analysis
    pub fn blocks_analysis(&self) -> bool {
        matches!(self, DiagnosticLevel::Fatal)
    }

    /// Check if this level indicates execution failure
    pub fn indicates_execution_failure(&self) -> bool {
        matches!(self, DiagnosticLevel::Fatal | DiagnosticLevel::Error)
    }
}

/// Specific diagnostic message with context
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum DiagnosticMessage {
    // Fatal diagnostics (prevent analysis)
    #[serde(rename_all = "camelCase")]
    DuplicateStepId { step_id: String },
    #[serde(rename_all = "camelCase")]
    EmptyStepId,
    #[serde(rename_all = "camelCase")]
    ForwardReference { from_step: String, to_step: String },
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

    // Warning diagnostics (potential issues)
    #[serde(rename_all = "camelCase")]
    MockComponent { step_id: String },
    #[serde(rename_all = "camelCase")]
    UnreachableStep { step_id: String },
    #[serde(rename_all = "camelCase")]
    MissingWorkflowName,
    #[serde(rename_all = "camelCase")]
    MissingWorkflowDescription,
    #[serde(rename_all = "camelCase")]
    UnvalidatedFieldAccess {
        step_id: String,
        field: String,
        reason: String,
    },

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
            DiagnosticMessage::ForwardReference { .. } => DiagnosticLevel::Fatal,
            DiagnosticMessage::SelfReference { .. } => DiagnosticLevel::Fatal,
            DiagnosticMessage::UndefinedStepReference { .. } => DiagnosticLevel::Fatal,
            DiagnosticMessage::InvalidReferenceExpression { .. } => DiagnosticLevel::Fatal,

            // Error diagnostics
            DiagnosticMessage::InvalidFieldAccess { .. } => DiagnosticLevel::Error,
            DiagnosticMessage::InvalidComponent { .. } => DiagnosticLevel::Error,
            DiagnosticMessage::EmptyComponentName { .. } => DiagnosticLevel::Error,
            DiagnosticMessage::SchemaViolation { .. } => DiagnosticLevel::Error,

            // Warning diagnostics
            DiagnosticMessage::MockComponent { .. } => DiagnosticLevel::Warning,
            DiagnosticMessage::UnreachableStep { .. } => DiagnosticLevel::Warning,
            DiagnosticMessage::MissingWorkflowName => DiagnosticLevel::Warning,
            DiagnosticMessage::MissingWorkflowDescription => DiagnosticLevel::Warning,
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
            DiagnosticMessage::ForwardReference { from_step, to_step } => {
                format!("Step '{from_step}' references forward-declared step '{to_step}'")
            }
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
            DiagnosticMessage::MissingWorkflowName => "Workflow has no name defined".to_string(),
            DiagnosticMessage::MissingWorkflowDescription => {
                "Workflow has no description defined".to_string()
            }
            DiagnosticMessage::UnvalidatedFieldAccess {
                step_id,
                field,
                reason,
            } => {
                format!("Field access '{field}' on step '{step_id}' cannot be validated: {reason}")
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

    /// Get the step ID associated with this diagnostic (if any)
    pub fn step_id(&self) -> Option<&str> {
        match self {
            DiagnosticMessage::DuplicateStepId { step_id } => Some(step_id),
            DiagnosticMessage::EmptyStepId => None,
            DiagnosticMessage::ForwardReference { from_step, .. } => Some(from_step),
            DiagnosticMessage::SelfReference { step_id } => Some(step_id),
            DiagnosticMessage::UndefinedStepReference { from_step, .. } => from_step.as_deref(),
            DiagnosticMessage::InvalidReferenceExpression { step_id, .. } => step_id.as_deref(),
            DiagnosticMessage::InvalidFieldAccess { step_id, .. } => Some(step_id),
            DiagnosticMessage::InvalidComponent { step_id, .. } => Some(step_id),
            DiagnosticMessage::EmptyComponentName { step_id } => Some(step_id),
            DiagnosticMessage::SchemaViolation { step_id, .. } => Some(step_id),
            DiagnosticMessage::MockComponent { step_id } => Some(step_id),
            DiagnosticMessage::UnreachableStep { step_id } => Some(step_id),
            DiagnosticMessage::MissingWorkflowName => None,
            DiagnosticMessage::MissingWorkflowDescription => None,
            DiagnosticMessage::UnvalidatedFieldAccess { step_id, .. } => Some(step_id),
            DiagnosticMessage::NoPluginsConfigured => None,
            DiagnosticMessage::NoRoutingRulesConfigured => None,
            DiagnosticMessage::InvalidRouteReference { .. } => None,
            DiagnosticMessage::UnusedPlugin { .. } => None,
        }
    }

    /// Get the field associated with this diagnostic (if any)
    pub fn field(&self) -> Option<&str> {
        match self {
            DiagnosticMessage::InvalidReferenceExpression { field, .. } => field.as_deref(),
            DiagnosticMessage::InvalidFieldAccess { field, .. } => Some(field),
            DiagnosticMessage::SchemaViolation { field, .. } => Some(field),
            DiagnosticMessage::UnvalidatedFieldAccess { field, .. } => Some(field),
            _ => None,
        }
    }
}

/// A single diagnostic with its context
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    /// The diagnostic message and type
    pub message: DiagnosticMessage,
    /// The severity level
    pub level: DiagnosticLevel,
    /// Human-readable message text
    pub text: String,
    /// JSON path to the field with the issue
    pub path: Vec<String>,
    /// Whether this diagnostic should be ignored by default
    pub ignore: bool,
}

impl Diagnostic {
    /// Create a new diagnostic from a message with an empty path
    pub fn new(message: DiagnosticMessage) -> Self {
        Self::new_with_path(message, vec![])
    }

    /// Create a new diagnostic from a message with a specific path
    pub fn new_with_path(message: DiagnosticMessage, path: Vec<String>) -> Self {
        let level = message.level();
        let text = message.message();
        let ignore = matches!(message, DiagnosticMessage::UnvalidatedFieldAccess { .. });

        Self {
            message,
            level,
            text,
            path,
            ignore,
        }
    }

    /// Check if this diagnostic blocks analysis
    pub fn blocks_analysis(&self) -> bool {
        self.level.blocks_analysis()
    }

    /// Check if this diagnostic indicates execution failure
    pub fn indicates_execution_failure(&self) -> bool {
        self.level.indicates_execution_failure()
    }
}

/// Collection of diagnostics with utility methods
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    /// All diagnostics found
    pub diagnostics: Vec<Diagnostic>,
}

impl Diagnostics {
    /// Create a new empty diagnostics collection
    pub fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
        }
    }

    /// Add a diagnostic with a specific path
    pub fn add(&mut self, message: DiagnosticMessage, path: Vec<String>) {
        self.diagnostics
            .push(Diagnostic::new_with_path(message, path));
    }

    /// Check if there are any fatal diagnostics
    pub fn has_fatal(&self) -> bool {
        self.diagnostics.iter().any(|d| d.blocks_analysis())
    }

    /// Get all diagnostics at a specific level
    pub fn at_level(&self, level: DiagnosticLevel) -> Vec<&Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.level == level)
            .collect()
    }

    /// Get all diagnostics that should be shown by default (excludes ignored)
    pub fn shown_by_default(&self) -> Vec<&Diagnostic> {
        self.diagnostics.iter().filter(|d| !d.ignore).collect()
    }

    /// Get count of diagnostics at each level (fatal, error, warning)
    pub fn counts(&self) -> (usize, usize, usize) {
        let mut fatal = 0;
        let mut error = 0;
        let mut warning = 0;

        for diagnostic in &self.diagnostics {
            match diagnostic.level {
                DiagnosticLevel::Fatal => fatal += 1,
                DiagnosticLevel::Error => error += 1,
                DiagnosticLevel::Warning => warning += 1,
            }
        }

        (fatal, error, warning)
    }

    /// Check if diagnostics are empty
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Get total count of diagnostics
    pub fn len(&self) -> usize {
        self.diagnostics.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_levels() {
        let fatal = Diagnostic::new(DiagnosticMessage::DuplicateStepId {
            step_id: "test".to_string(),
        });
        assert_eq!(fatal.level, DiagnosticLevel::Fatal);
        assert!(fatal.blocks_analysis());
        assert!(fatal.indicates_execution_failure());

        let error = Diagnostic::new(DiagnosticMessage::InvalidFieldAccess {
            step_id: "test".to_string(),
            field: "field".to_string(),
            reason: "missing".to_string(),
        });
        assert_eq!(error.level, DiagnosticLevel::Error);
        assert!(!error.blocks_analysis());
        assert!(error.indicates_execution_failure());

        let warning = Diagnostic::new(DiagnosticMessage::MockComponent {
            step_id: "test".to_string(),
        });
        assert_eq!(warning.level, DiagnosticLevel::Warning);
        assert!(!warning.blocks_analysis());
        assert!(!warning.indicates_execution_failure());
    }

    #[test]
    fn test_diagnostics_collection() {
        let mut diagnostics = Diagnostics::new();

        diagnostics.add(
            DiagnosticMessage::DuplicateStepId {
                step_id: "test".to_string(),
            },
            vec![],
        );
        diagnostics.add(
            DiagnosticMessage::MockComponent {
                step_id: "test".to_string(),
            },
            vec![],
        );

        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics.has_fatal());

        let (fatal, error, warning) = diagnostics.counts();
        assert_eq!(fatal, 1);
        assert_eq!(error, 0);
        assert_eq!(warning, 1);
    }
}
