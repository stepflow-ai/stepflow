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

//! Diagnostic messages with error codes.
//!
//! ## Error Code Scheme
//!
//! Codes are assigned by severity and category:
//!
//! - **1xxx**: Warnings
//!   - 10xx: Flow structure warnings
//!   - 11xx: Type check warnings
//!   - 12xx: Config warnings
//!   - 15xx+: Component warnings
//!
//! - **2xxx**: Errors
//!   - 20xx: Flow structure errors
//!   - 21xx: Type check errors
//!   - 22xx: Config errors
//!   - 25xx+: Component errors
//!
//! - **3xxx**: Fatal (prevents further analysis)
//!   - 30xx: Flow structure fatal

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::{DiagnosticLevel, Path};

/// Diagnostic error codes.
///
/// Codes are organized by severity (thousands digit) and category (hundreds digit).
/// See module documentation for the full scheme.
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::IntoStaticStr)]
#[strum(serialize_all = "camelCase")]
pub enum DiagnosticKind {
    // ==========================================================================
    // Warnings - Flow structure (10xx)
    // ==========================================================================
    /// Step is not reachable
    UnreachableStep = 1000,
    /// Workflow has no name
    MissingFlowName = 1001,
    /// Workflow has no description
    MissingFlowDescription = 1002,
    /// Field access cannot be validated
    UnvalidatedFieldAccess = 1003,
    /// No variable schema defined
    MissingVariableSchema = 1004,

    // ==========================================================================
    // Warnings - Type check (11xx)
    // ==========================================================================
    /// Component has no output schema
    UntypedComponentOutput = 1100,
    /// Type check result is indeterminate
    TypeCheckIndeterminate = 1101,

    // ==========================================================================
    // Warnings - Config (12xx)
    // ==========================================================================
    /// No plugins configured
    NoPluginsConfigured = 1200,
    /// No routing rules configured
    NoRoutingRulesConfigured = 1201,
    /// Plugin not used by any route
    UnusedPlugin = 1202,

    // ==========================================================================
    // Warnings - Component (15xx)
    // ==========================================================================
    /// Step uses mock component
    MockComponent = 1500,

    // ==========================================================================
    // Errors - Flow structure (20xx)
    // ==========================================================================
    /// Invalid field access on step output
    InvalidFieldAccess = 2000,
    /// Empty component name in step
    EmptyComponentName = 2001,
    /// Schema violation in step input
    SchemaViolation = 2002,
    /// Invalid subflow literal
    InvalidSubflowLiteral = 2003,
    /// Undefined variable reference
    UndefinedVariable = 2004,
    /// Undefined required variable
    UndefinedRequiredVariable = 2005,

    // ==========================================================================
    // Errors - Type check (21xx)
    // ==========================================================================
    /// Type mismatch in step input
    TypeMismatch = 2100,
    /// Unknown property in path
    UnknownPropertyInPath = 2101,

    // ==========================================================================
    // Errors - Config (22xx)
    // ==========================================================================
    /// Route references unknown plugin
    InvalidRouteReference = 2200,

    // ==========================================================================
    // Errors - Component (25xx)
    // ==========================================================================
    /// Invalid component
    InvalidComponent = 2500,

    // ==========================================================================
    // Fatal - Flow structure (30xx)
    // ==========================================================================
    /// Duplicate step ID in workflow
    DuplicateStepId = 3000,
    /// Empty step ID
    EmptyStepId = 3001,
    /// Step references itself
    SelfReference = 3002,
    /// Reference to undefined step
    UndefinedStepReference = 3003,
    /// Invalid reference expression syntax
    InvalidReferenceExpression = 3004,
}

impl DiagnosticKind {
    /// Get the numeric error code
    #[inline]
    pub fn code(self) -> u16 {
        self as u16
    }

    /// Get the kind name in camelCase
    #[inline]
    pub fn name(self) -> &'static str {
        self.into()
    }

    /// Get the severity level based on code range
    #[inline]
    pub fn level(self) -> DiagnosticLevel {
        match self.code() {
            3000..=3999 => DiagnosticLevel::Fatal,
            2000..=2999 => DiagnosticLevel::Error,
            _ => DiagnosticLevel::Warning,
        }
    }
}

/// A diagnostic with error code, message, path, and metadata.
///
/// Created via the `diagnostic!` macro with optional builder methods:
///
/// ```ignore
/// diagnostic!(DiagnosticKind::DuplicateStepId, "Duplicate step ID '{step_id}'", { step_id })
///     .at(path)
///     .experimental()
/// ```
///
/// ## JSON Format
///
/// ```json
/// {
///   "kind": "duplicateStepId",
///   "code": 3000,
///   "level": "fatal",
///   "formatted": "Duplicate step ID 'foo'",
///   "data": { "stepId": "foo" },
///   "path": "$.steps[0]",
///   "experimental": false
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    /// The diagnostic kind name (camelCase)
    pub kind: Cow<'static, str>,
    /// Numeric error code
    pub code: u16,
    /// The severity level
    pub level: DiagnosticLevel,
    /// Human-readable formatted message
    pub formatted: String,
    /// Structured data with camelCase keys (null if no data)
    #[serde(skip_serializing_if = "is_null_or_empty")]
    #[serde(default = "serde_json::Value::default")]
    pub data: serde_json::Value,
    /// JSON path to the field with the issue
    #[serde(skip_serializing_if = "Path::is_empty", default)]
    pub path: Path,
    /// Whether this diagnostic is experimental (may have false positives)
    #[serde(skip_serializing_if = "is_false", default)]
    pub experimental: bool,
}

fn is_null_or_empty(value: &serde_json::Value) -> bool {
    value.is_null() || (value.is_object() && value.as_object().unwrap().is_empty())
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl Diagnostic {
    /// Create a new diagnostic
    pub fn new(kind: DiagnosticKind, formatted: String, data: serde_json::Value) -> Self {
        Self {
            kind: Cow::Borrowed(kind.name()),
            code: kind.code(),
            level: kind.level(),
            formatted,
            data,
            path: Path::new(),
            experimental: false,
        }
    }

    /// Set the path for this diagnostic (builder pattern)
    #[must_use]
    pub fn at(mut self, path: Path) -> Self {
        self.path = path;
        self
    }

    /// Mark this diagnostic as experimental (builder pattern)
    #[must_use]
    pub fn experimental(mut self) -> Self {
        self.experimental = true;
        self
    }
}

/// Create a diagnostic from a kind, format string, and arguments.
///
/// # Examples
///
/// ```ignore
/// // With arguments
/// let step_id = "foo";
/// let diag = diagnostic!(
///     DiagnosticKind::DuplicateStepId,
///     "Duplicate step ID '{step_id}'",
///     { step_id }
/// );
///
/// // Without arguments
/// let diag = diagnostic!(
///     DiagnosticKind::MissingFlowName,
///     "Workflow has no name defined"
/// );
///
/// // With path
/// let diag = diagnostic!(
///     DiagnosticKind::DuplicateStepId,
///     "Duplicate step ID '{step_id}'",
///     { step_id }
/// ).at(make_path!("steps", 0));
///
/// // Experimental diagnostic
/// let diag = diagnostic!(
///     DiagnosticKind::UnvalidatedFieldAccess,
///     "Field access cannot be validated",
/// ).experimental();
/// ```
#[macro_export]
macro_rules! diagnostic {
    // With arguments - creates formatted message and JSON data
    ($kind:expr, $fmt:literal, { $($arg:ident),* $(,)? } $(,)?) => {{
        $crate::Diagnostic::new(
            $kind,
            format!($fmt),
            serde_json::json!({ $(stringify!($arg): $arg),* }),
        )
    }};
    // Without arguments (with optional trailing comma)
    ($kind:expr, $fmt:literal $(,)?) => {{
        $crate::Diagnostic::new(
            $kind,
            $fmt.to_string(),
            serde_json::Value::Null,
        )
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_kind_codes() {
        // Fatal codes are 3xxx
        assert_eq!(DiagnosticKind::DuplicateStepId.code(), 3000);
        assert_eq!(
            DiagnosticKind::DuplicateStepId.level(),
            DiagnosticLevel::Fatal
        );

        // Error codes are 2xxx
        assert_eq!(DiagnosticKind::InvalidFieldAccess.code(), 2000);
        assert_eq!(
            DiagnosticKind::InvalidFieldAccess.level(),
            DiagnosticLevel::Error
        );
        assert_eq!(DiagnosticKind::TypeMismatch.code(), 2100);
        assert_eq!(DiagnosticKind::InvalidComponent.code(), 2500);

        // Warning codes are 1xxx
        assert_eq!(DiagnosticKind::UnreachableStep.code(), 1000);
        assert_eq!(
            DiagnosticKind::UnreachableStep.level(),
            DiagnosticLevel::Warning
        );
        assert_eq!(DiagnosticKind::UntypedComponentOutput.code(), 1100);
        assert_eq!(DiagnosticKind::MockComponent.code(), 1500);
    }

    #[test]
    fn test_diagnostic_kind_names() {
        assert_eq!(DiagnosticKind::DuplicateStepId.name(), "duplicateStepId");
        assert_eq!(
            DiagnosticKind::InvalidFieldAccess.name(),
            "invalidFieldAccess"
        );
        assert_eq!(DiagnosticKind::TypeMismatch.name(), "typeMismatch");
    }

    #[test]
    fn test_diagnostic_macro_with_args() {
        let step_id = "foo";
        let diag = diagnostic!(
            DiagnosticKind::DuplicateStepId,
            "Duplicate step ID '{step_id}'",
            { step_id }
        );

        assert_eq!(diag.kind, "duplicateStepId");
        assert_eq!(diag.code, 3000);
        assert_eq!(diag.level, DiagnosticLevel::Fatal);
        assert_eq!(diag.formatted, "Duplicate step ID 'foo'");
        assert_eq!(diag.data.get("step_id").unwrap(), "foo");
        assert!(diag.path.is_empty());
        assert!(!diag.experimental);
    }

    #[test]
    fn test_diagnostic_macro_without_args() {
        let diag = diagnostic!(
            DiagnosticKind::MissingFlowName,
            "Workflow has no name defined"
        );

        assert_eq!(diag.kind, "missingFlowName");
        assert_eq!(diag.code, 1001);
        assert_eq!(diag.level, DiagnosticLevel::Warning);
        assert_eq!(diag.formatted, "Workflow has no name defined");
        assert!(diag.data.is_null());
    }

    #[test]
    fn test_diagnostic_builder_methods() {
        use crate::make_path;

        let step_id = "foo";
        let diag = diagnostic!(
            DiagnosticKind::UnvalidatedFieldAccess,
            "Field access on '{step_id}' cannot be validated",
            { step_id }
        )
        .at(make_path!("steps", 0, "input"))
        .experimental();

        assert_eq!(diag.path, make_path!("steps", 0, "input"));
        assert!(diag.experimental);
    }

    #[test]
    fn test_diagnostic_serialization() {
        let step_id = "foo";
        let diag = diagnostic!(
            DiagnosticKind::DuplicateStepId,
            "Duplicate step ID '{step_id}'",
            { step_id }
        );

        let json = serde_json::to_value(&diag).unwrap();

        assert_eq!(json.get("kind").unwrap(), "duplicateStepId");
        assert_eq!(json.get("code").unwrap(), 3000);
        assert_eq!(json.get("level").unwrap(), "fatal");
        assert_eq!(json.get("formatted").unwrap(), "Duplicate step ID 'foo'");
        assert_eq!(json.get("data").unwrap().get("step_id").unwrap(), "foo");
        // path and experimental are skipped when empty/false
        assert!(json.get("path").is_none());
        assert!(json.get("experimental").is_none());
    }

    #[test]
    fn test_diagnostic_deserialization() {
        let json = serde_json::json!({
            "kind": "duplicateStepId",
            "code": 3000,
            "level": "fatal",
            "formatted": "Duplicate step ID 'foo'",
            "data": { "stepId": "foo" }
        });

        let diag: Diagnostic = serde_json::from_value(json).unwrap();

        assert_eq!(diag.kind, "duplicateStepId");
        assert_eq!(diag.code, 3000);
        assert_eq!(diag.level, DiagnosticLevel::Fatal);
        assert_eq!(diag.formatted, "Duplicate step ID 'foo'");
        assert_eq!(diag.data.get("stepId").unwrap(), "foo");
        assert!(diag.path.is_empty());
        assert!(!diag.experimental);
    }

    #[test]
    fn test_diagnostic_roundtrip() {
        use crate::make_path;

        let step_id = "step1";
        let component = "/builtin/openai";
        let error = "component not found";
        let original = diagnostic!(
            DiagnosticKind::InvalidComponent,
            "Invalid component '{component}' in step '{step_id}': {error}",
            { step_id, component, error }
        )
        .at(make_path!("steps", 0))
        .experimental();

        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: Diagnostic = serde_json::from_str(&serialized).unwrap();

        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_diagnostic_level_from_code() {
        let diag = Diagnostic {
            kind: Cow::Borrowed("test"),
            code: 3000,
            level: DiagnosticLevel::Fatal,
            formatted: "test".to_string(),
            data: serde_json::Value::Null,
            path: Path::new(),
            experimental: false,
        };
        assert_eq!(diag.level, DiagnosticLevel::Fatal);

        let diag = Diagnostic {
            kind: Cow::Borrowed("test"),
            code: 2000,
            level: DiagnosticLevel::Error,
            formatted: "test".to_string(),
            data: serde_json::Value::Null,
            path: Path::new(),
            experimental: false,
        };
        assert_eq!(diag.level, DiagnosticLevel::Error);

        let diag = Diagnostic {
            kind: Cow::Borrowed("test"),
            code: 1000,
            level: DiagnosticLevel::Warning,
            formatted: "test".to_string(),
            data: serde_json::Value::Null,
            path: Path::new(),
            experimental: false,
        };
        assert_eq!(diag.level, DiagnosticLevel::Warning);
    }
}
