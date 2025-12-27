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

//! Type checking errors.
//!
//! These errors are specific to the type checking process. They are designed
//! to be mapped to diagnostic messages when integrated with `stepflow-analysis`.

use thiserror::Error;

use crate::Type;

/// Errors produced during type checking.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum TypeError {
    /// Type mismatch between expected and actual types.
    #[error("expected {expected}, found {actual}: {detail}")]
    Mismatch {
        /// The expected type.
        expected: Type,
        /// The actual type that was found.
        actual: Type,
        /// Additional detail about the mismatch.
        detail: String,
    },

    /// Property not found when projecting through a path.
    #[error("property '{property}' not found in path '{path}'")]
    PropertyNotFound {
        /// The full path being accessed.
        path: String,
        /// The property that was not found.
        property: String,
    },

    /// Array index access on non-array type.
    #[error("cannot index into non-array type at '{path}'")]
    NotAnArray {
        /// The path where array access was attempted.
        path: String,
    },

    /// Unknown step referenced in expression.
    #[error("unknown step '{step_id}'")]
    UnknownStep {
        /// The step ID that was not found.
        step_id: String,
    },

    /// Unknown variable referenced in expression.
    #[error("unknown variable '{variable}'")]
    UnknownVariable {
        /// The variable name that was not found.
        variable: String,
    },

    /// Cannot determine type statically.
    #[error("cannot determine type: {reason}")]
    Indeterminate {
        /// The reason the type cannot be determined.
        reason: String,
    },

    /// Missing required property in input.
    #[error("missing required property '{property}'")]
    MissingRequired {
        /// The property that is required but missing.
        property: String,
    },

    /// Extra property in strict object.
    #[error("unexpected property '{property}' at '{path}', allowed: {allowed:?}")]
    UnexpectedProperty {
        /// The property that is not allowed.
        property: String,
        /// The path where the property was found.
        path: String,
        /// The allowed properties (if known).
        allowed: Vec<String>,
    },

    /// Component output schema is not available.
    #[error("component '{component}' does not provide output schema")]
    UntypedOutput {
        /// The component that lacks output schema.
        component: String,
    },
}

impl TypeError {
    /// Create a type mismatch error.
    pub fn mismatch(expected: Type, actual: Type, detail: impl Into<String>) -> Self {
        TypeError::Mismatch {
            expected,
            actual,
            detail: detail.into(),
        }
    }

    /// Create a property not found error.
    pub fn property_not_found(path: impl Into<String>, property: impl Into<String>) -> Self {
        TypeError::PropertyNotFound {
            path: path.into(),
            property: property.into(),
        }
    }

    /// Create an unknown step error.
    pub fn unknown_step(step_id: impl Into<String>) -> Self {
        TypeError::UnknownStep {
            step_id: step_id.into(),
        }
    }

    /// Create an unknown variable error.
    pub fn unknown_variable(variable: impl Into<String>) -> Self {
        TypeError::UnknownVariable {
            variable: variable.into(),
        }
    }

    /// Create an indeterminate type error.
    pub fn indeterminate(reason: impl Into<String>) -> Self {
        TypeError::Indeterminate {
            reason: reason.into(),
        }
    }
}

/// Location information for type errors.
///
/// Provides context about where in the workflow a type error occurred.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeErrorLocation {
    /// The step ID where the error occurred, if applicable.
    pub step_id: Option<String>,
    /// The JSON path to the error location (e.g., `$.steps\[0\].input.foo``).
    pub path: String,
}

impl TypeErrorLocation {
    /// Create a location for a step.
    pub fn step(step_id: impl Into<String>, path: impl Into<String>) -> Self {
        TypeErrorLocation {
            step_id: Some(step_id.into()),
            path: path.into(),
        }
    }

    /// Create a location at the flow level (not within a specific step).
    pub fn flow(path: impl Into<String>) -> Self {
        TypeErrorLocation {
            step_id: None,
            path: path.into(),
        }
    }
}

impl std::fmt::Display for TypeErrorLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(step_id) = &self.step_id {
            write!(f, "step '{}' at {}", step_id, self.path)
        } else {
            write!(f, "{}", self.path)
        }
    }
}

/// A type error with location information.
#[derive(Debug, Clone, PartialEq)]
pub struct LocatedTypeError {
    /// The type error.
    pub error: TypeError,
    /// Where the error occurred.
    pub location: TypeErrorLocation,
}

impl LocatedTypeError {
    /// Create a new located error.
    pub fn new(error: TypeError, location: TypeErrorLocation) -> Self {
        LocatedTypeError { error, location }
    }

    /// Create a located error for a step.
    pub fn at_step(error: TypeError, step_id: impl Into<String>, path: impl Into<String>) -> Self {
        LocatedTypeError {
            error,
            location: TypeErrorLocation::step(step_id, path),
        }
    }

    /// Create a located error at the flow level.
    pub fn at_flow(error: TypeError, path: impl Into<String>) -> Self {
        LocatedTypeError {
            error,
            location: TypeErrorLocation::flow(path),
        }
    }
}

impl std::fmt::Display for LocatedTypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.location, self.error)
    }
}

impl std::error::Error for LocatedTypeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_error_display() {
        let err = TypeError::mismatch(
            Type::from_json(serde_json::json!({"type": "string"})),
            Type::from_json(serde_json::json!({"type": "number"})),
            "property 'name'",
        );
        assert!(err.to_string().contains("string"));
        assert!(err.to_string().contains("number"));
    }

    #[test]
    fn test_located_error_display() {
        let err = LocatedTypeError::at_step(
            TypeError::unknown_step("missing_step"),
            "current_step",
            "$.input.data",
        );
        assert!(err.to_string().contains("current_step"));
        assert!(err.to_string().contains("missing_step"));
    }

    #[test]
    fn test_location_display() {
        let step_loc = TypeErrorLocation::step("my_step", "$.input");
        assert_eq!(step_loc.to_string(), "step 'my_step' at $.input");

        let flow_loc = TypeErrorLocation::flow("$.output");
        assert_eq!(flow_loc.to_string(), "$.output");
    }
}
