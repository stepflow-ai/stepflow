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

use serde::{Deserialize, Serialize};

mod level;
mod message;

pub use level::DiagnosticLevel;
pub use message::DiagnosticMessage;

use crate::Path;

/// A single diagnostic with its context
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    /// The diagnostic message and type (boxed to handle varying sizes)
    message: Box<DiagnosticMessage>,
    /// The severity level
    pub level: DiagnosticLevel,
    /// Human-readable message text
    pub text: String,
    /// JSON path to the field with the issue
    #[serde(skip_serializing_if = "Path::is_empty", default)]
    pub path: Path,
    /// Whether this diagnostic should be ignored by default
    pub ignore: bool,
}

impl Diagnostic {
    /// Create a new diagnostic from a message with an empty path
    pub fn new(message: DiagnosticMessage) -> Self {
        Self::new_with_path(message, Path::new())
    }

    pub fn message(&self) -> &DiagnosticMessage {
        &self.message
    }

    /// Create a new diagnostic from a message with a specific path
    pub fn new_with_path(message: DiagnosticMessage, path: Path) -> Self {
        let level = message.level();
        let text = message.message();
        let ignore = matches!(message, DiagnosticMessage::UnvalidatedFieldAccess { .. });

        Self {
            message: Box::new(message),
            level,
            text,
            path,
            ignore,
        }
    }
}

/// Collection of diagnostics with utility methods
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    /// All diagnostics found
    pub diagnostics: Vec<Diagnostic>,
    pub num_fatal: u32,
    pub num_error: u32,
    pub num_warning: u32,
}

impl Diagnostics {
    /// Create a new empty diagnostics collection
    pub fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
            num_fatal: 0,
            num_error: 0,
            num_warning: 0,
        }
    }

    /// Add a diagnostic with a specific path
    pub fn add(&mut self, message: DiagnosticMessage, path: Path) {
        match message.level() {
            DiagnosticLevel::Fatal => self.num_fatal += 1,
            DiagnosticLevel::Error => self.num_error += 1,
            DiagnosticLevel::Warning => self.num_warning += 1,
        }

        self.diagnostics
            .push(Diagnostic::new_with_path(message, path));
    }

    pub fn extend(&mut self, mut other: Diagnostics) {
        self.num_fatal += other.num_fatal;
        self.num_error += other.num_error;
        self.num_warning += other.num_warning;
        self.diagnostics.append(&mut other.diagnostics);
    }

    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> + '_ {
        self.diagnostics.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Diagnostic> + '_ {
        self.diagnostics.iter_mut()
    }

    /// Check if there are any fatal diagnostics
    pub fn has_fatal(&self) -> bool {
        self.num_fatal > 0
    }

    /// Get all diagnostics at a specific level
    pub fn at_level(&self, level: DiagnosticLevel) -> impl Iterator<Item = &Diagnostic> + '_ {
        self.diagnostics.iter().filter(move |d| d.level == level)
    }

    /// Get all diagnostics that should be shown by default (excludes ignored)
    pub fn shown_by_default(&self) -> impl Iterator<Item = &Diagnostic> + '_ {
        self.diagnostics.iter().filter(|d| !d.ignore)
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

        let error = Diagnostic::new(DiagnosticMessage::InvalidFieldAccess {
            step_id: "test".to_string(),
            field: "field".to_string(),
            reason: "missing".to_string(),
        });
        assert_eq!(error.level, DiagnosticLevel::Error);

        let warning = Diagnostic::new(DiagnosticMessage::MockComponent {
            step_id: "test".to_string(),
        });
        assert_eq!(warning.level, DiagnosticLevel::Warning);
    }

    #[test]
    fn test_diagnostics_collection() {
        let mut diagnostics = Diagnostics::new();

        diagnostics.add(
            DiagnosticMessage::DuplicateStepId {
                step_id: "test".to_string(),
            },
            Path::new(),
        );
        diagnostics.add(
            DiagnosticMessage::MockComponent {
                step_id: "test".to_string(),
            },
            Path::new(),
        );

        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics.has_fatal());

        assert_eq!(diagnostics.num_fatal, 1);
        assert_eq!(diagnostics.num_error, 0);
        assert_eq!(diagnostics.num_warning, 1);
    }
}
