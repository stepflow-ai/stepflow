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

use crate::{DiagnosticMessage, Diagnostics, Path};
use stepflow_typecheck::{LocatedTypeError, TypeCheckResult, TypeError};

/// Convert a `TypeCheckResult` into diagnostics.
///
/// This maps type checking errors and warnings to the appropriate diagnostic messages.
pub fn type_check_to_diagnostics(result: &TypeCheckResult) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();

    // Convert errors
    for error in &result.errors {
        let message = type_error_to_diagnostic(error);
        // Use empty path - the step_id and path are already in the message
        diagnostics.add(message, Path::new());
    }

    // Convert warnings
    for warning in &result.warnings {
        let message = type_error_to_diagnostic(warning);
        diagnostics.add(message, Path::new());
    }

    diagnostics
}

/// Convert a single `LocatedTypeError` to a `DiagnosticMessage`.
fn type_error_to_diagnostic(error: &LocatedTypeError) -> DiagnosticMessage {
    let step_id = error.location.step_id.clone();

    match &error.error {
        TypeError::Mismatch {
            expected,
            actual,
            detail,
        } => DiagnosticMessage::TypeMismatch {
            step_id: step_id.unwrap_or_else(|| "<flow>".to_string()),
            expected: expected.to_string(),
            actual: actual.to_string(),
            detail: detail.clone(),
        },

        TypeError::PropertyNotFound { path, property } => {
            DiagnosticMessage::UnknownPropertyInPath {
                step_id,
                path: path.clone(),
                property: property.clone(),
            }
        }

        TypeError::NotAnArray { path } => DiagnosticMessage::UnknownPropertyInPath {
            step_id,
            path: path.clone(),
            property: "<array index>".to_string(),
        },

        TypeError::UnknownStep { step_id: ref_step } => DiagnosticMessage::UndefinedStepReference {
            from_step: step_id,
            referenced_step: ref_step.clone(),
        },

        TypeError::UnknownVariable { variable } => DiagnosticMessage::UndefinedVariable {
            variable: variable.clone(),
            context: step_id.unwrap_or_else(|| "flow".to_string()),
        },

        TypeError::Indeterminate { reason } => DiagnosticMessage::TypeCheckIndeterminate {
            step_id,
            reason: reason.clone(),
        },

        TypeError::MissingRequired { property, .. } => DiagnosticMessage::TypeMismatch {
            step_id: step_id.unwrap_or_else(|| "<flow>".to_string()),
            expected: format!("required property '{property}'"),
            actual: "missing".to_string(),
            detail: "Required property is not present in the input".to_string(),
        },

        TypeError::UnexpectedProperty {
            property,
            allowed,
            path,
        } => DiagnosticMessage::TypeMismatch {
            step_id: step_id.unwrap_or_else(|| "<flow>".to_string()),
            expected: format!("one of {:?}", allowed),
            actual: format!("property '{property}'"),
            detail: format!("Unexpected property at path '{path}'"),
        },

        TypeError::UntypedOutput { component } => DiagnosticMessage::UntypedComponentOutput {
            step_id: step_id.unwrap_or_else(|| "<flow>".to_string()),
            component: component.clone(),
        },
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

        let message = type_error_to_diagnostic(&error);
        match message {
            DiagnosticMessage::TypeMismatch {
                step_id,
                expected,
                actual,
                ..
            } => {
                assert_eq!(step_id, "step1");
                assert_eq!(expected, "string");
                assert_eq!(actual, "number");
            }
            _ => panic!("Expected TypeMismatch diagnostic"),
        }
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

        let message = type_error_to_diagnostic(&error);
        match message {
            DiagnosticMessage::UnknownPropertyInPath {
                step_id,
                path,
                property,
            } => {
                assert_eq!(step_id, Some("step2".to_string()));
                assert_eq!(path, "$.data");
                assert_eq!(property, "missing_field");
            }
            _ => panic!("Expected UnknownPropertyInPath diagnostic"),
        }
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

        let message = type_error_to_diagnostic(&error);
        match message {
            DiagnosticMessage::UndefinedStepReference {
                from_step,
                referenced_step,
            } => {
                assert_eq!(from_step, Some("step1".to_string()));
                assert_eq!(referenced_step, "nonexistent");
            }
            _ => panic!("Expected UndefinedStepReference diagnostic"),
        }
    }
}
