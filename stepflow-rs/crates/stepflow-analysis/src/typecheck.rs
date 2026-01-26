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

//! Integration between stepflow-typecheck and stepflow-analysis diagnostics.
//!
//! This module provides conversion from type checking errors to diagnostics.

use crate::{Diagnostic, DiagnosticKind, Diagnostics, diagnostic};
use stepflow_typecheck::{LocatedTypeError, TypeCheckResult, TypeError};

/// Convert a `TypeCheckResult` into diagnostics.
///
/// This maps type checking errors and warnings to the appropriate diagnostic messages.
pub fn type_check_to_diagnostics(result: &TypeCheckResult) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();

    // Convert errors
    for error in &result.errors {
        let diag = type_error_to_diagnostic(error);
        diagnostics.add(diag);
    }

    // Convert warnings
    for warning in &result.warnings {
        let diag = type_error_to_diagnostic(warning);
        diagnostics.add(diag);
    }

    diagnostics
}

/// Convert a single `LocatedTypeError` to a `Diagnostic`.
fn type_error_to_diagnostic(error: &LocatedTypeError) -> Diagnostic {
    let step_id = error
        .location
        .step_id
        .clone()
        .unwrap_or_else(|| "<flow>".to_string());

    match &error.error {
        TypeError::Mismatch {
            expected,
            actual,
            detail,
        } => {
            let expected = expected.to_string();
            let actual = actual.to_string();
            let detail = detail.clone();
            diagnostic!(
                DiagnosticKind::TypeMismatch,
                "Type mismatch in step '{step_id}': expected {expected}, got {actual}. {detail}",
                { step_id, expected, actual, detail }
            )
        }

        TypeError::PropertyNotFound { path, property } => {
            let path = path.clone();
            let property = property.clone();
            diagnostic!(
                DiagnosticKind::UnknownPropertyInPath,
                "Unknown property '{property}' at path '{path}' in step '{step_id}'",
                { step_id, path, property }
            )
        }

        TypeError::NotAnArray { path } => {
            let path = path.clone();
            let property = "<array index>".to_string();
            diagnostic!(
                DiagnosticKind::UnknownPropertyInPath,
                "Cannot index non-array at path '{path}' in step '{step_id}'",
                { step_id, path, property }
            )
        }

        TypeError::UnknownStep { step_id: ref_step } => {
            let from_step = step_id;
            let referenced_step = ref_step.clone();
            diagnostic!(
                DiagnosticKind::UndefinedStepReference,
                "Step '{from_step}' references undefined step '{referenced_step}'",
                { from_step, referenced_step }
            )
        }

        TypeError::UnknownVariable { variable } => {
            let variable = variable.clone();
            let context = step_id;
            diagnostic!(
                DiagnosticKind::UndefinedVariable,
                "Variable '{variable}' is not defined in {context}",
                { variable, context }
            )
        }

        TypeError::Indeterminate { reason } => {
            let reason = reason.clone();
            diagnostic!(
                DiagnosticKind::TypeCheckIndeterminate,
                "Type check indeterminate for step '{step_id}': {reason}",
                { step_id, reason }
            )
        }

        TypeError::MissingRequired { property, .. } => {
            let expected = format!("required property '{property}'");
            let actual = "missing".to_string();
            let detail = "Required property is not present in the input".to_string();
            diagnostic!(
                DiagnosticKind::TypeMismatch,
                "Type mismatch in step '{step_id}': expected {expected}, got {actual}. {detail}",
                { step_id, expected, actual, detail }
            )
        }

        TypeError::UnexpectedProperty {
            property,
            allowed,
            path,
        } => {
            let expected = format!("one of {:?}", allowed);
            let actual = format!("property '{property}'");
            let detail = format!("Unexpected property at path '{path}'");
            diagnostic!(
                DiagnosticKind::TypeMismatch,
                "Type mismatch in step '{step_id}': expected {expected}, got {actual}. {detail}",
                { step_id, expected, actual, detail }
            )
        }

        TypeError::UntypedOutput { component } => {
            let component = component.clone();
            diagnostic!(
                DiagnosticKind::UntypedComponentOutput,
                "Component '{component}' in step '{step_id}' has no output schema",
                { step_id, component }
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use stepflow_typecheck::{Type, TypeErrorLocation};

    #[test]
    fn test_type_mismatch_to_diagnostic() {
        let error = LocatedTypeError {
            error: TypeError::Mismatch {
                expected: Type::from_json(json!({"type": "string"})),
                actual: Type::from_json(json!({"type": "number"})),
                detail: "input type does not match".to_string(),
            },
            location: TypeErrorLocation {
                step_id: Some("step1".to_string()),
                path: "$.input.field".to_string(),
            },
        };

        let diag = type_error_to_diagnostic(&error);
        assert_eq!(diag.kind, DiagnosticKind::TypeMismatch.name());
        assert_eq!(diag.code, DiagnosticKind::TypeMismatch.code());
        assert!(diag.formatted.contains("step1"));
        assert!(diag.formatted.contains("string"));
        assert!(diag.formatted.contains("number"));
    }

    #[test]
    fn test_property_not_found_to_diagnostic() {
        let error = LocatedTypeError {
            error: TypeError::PropertyNotFound {
                path: "$.data".to_string(),
                property: "missing_field".to_string(),
            },
            location: TypeErrorLocation {
                step_id: Some("step2".to_string()),
                path: "$.input".to_string(),
            },
        };

        let diag = type_error_to_diagnostic(&error);
        assert_eq!(diag.kind, DiagnosticKind::UnknownPropertyInPath.name());
        assert!(diag.formatted.contains("missing_field"));
        assert!(diag.formatted.contains("$.data"));
        assert!(diag.formatted.contains("step2"));
    }

    #[test]
    fn test_unknown_step_to_diagnostic() {
        let error = LocatedTypeError {
            error: TypeError::UnknownStep {
                step_id: "nonexistent".to_string(),
            },
            location: TypeErrorLocation {
                step_id: Some("step1".to_string()),
                path: "$.input".to_string(),
            },
        };

        let diag = type_error_to_diagnostic(&error);
        assert_eq!(diag.kind, DiagnosticKind::UndefinedStepReference.name());
        assert!(diag.formatted.contains("step1"));
        assert!(diag.formatted.contains("nonexistent"));
    }
}
