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
pub use message::{Diagnostic, DiagnosticKind};

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

    /// Add a diagnostic
    pub fn add(&mut self, diagnostic: Diagnostic) {
        match diagnostic.level {
            DiagnosticLevel::Fatal => self.num_fatal += 1,
            DiagnosticLevel::Error => self.num_error += 1,
            DiagnosticLevel::Warning => self.num_warning += 1,
        }
        self.diagnostics.push(diagnostic);
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

    /// Get all diagnostics that should be shown by default (excludes experimental)
    pub fn shown_by_default(&self) -> impl Iterator<Item = &Diagnostic> + '_ {
        self.diagnostics.iter().filter(|d| !d.experimental)
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
    use crate::{Path, diagnostic};

    #[test]
    fn test_diagnostic_levels() {
        let step_id = "test";
        let fatal = diagnostic!(
            DiagnosticKind::DuplicateStepId,
            "Duplicate step ID '{step_id}'",
            { step_id }
        );
        assert_eq!(fatal.level, DiagnosticLevel::Fatal);

        let field = "field";
        let reason = "missing";
        let error = diagnostic!(
            DiagnosticKind::InvalidFieldAccess,
            "Invalid field access '{field}' in step '{step_id}': {reason}",
            { step_id, field, reason }
        );
        assert_eq!(error.level, DiagnosticLevel::Error);

        let warning = diagnostic!(
            DiagnosticKind::MockComponent,
            "Step '{step_id}' uses mock component",
            { step_id }
        );
        assert_eq!(warning.level, DiagnosticLevel::Warning);
    }

    #[test]
    fn test_diagnostics_collection() {
        let mut diagnostics = Diagnostics::new();

        let step_id = "test";
        diagnostics.add(
            diagnostic!(
                DiagnosticKind::DuplicateStepId,
                "Duplicate step ID '{step_id}'",
                { step_id }
            )
            .at(Path::new()),
        );
        diagnostics.add(
            diagnostic!(
                DiagnosticKind::MockComponent,
                "Step '{step_id}' uses mock component",
                { step_id }
            )
            .at(Path::new()),
        );

        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics.has_fatal());

        assert_eq!(diagnostics.num_fatal, 1);
        assert_eq!(diagnostics.num_error, 0);
        assert_eq!(diagnostics.num_warning, 1);
    }

    #[test]
    fn test_experimental_filtering() {
        let mut diagnostics = Diagnostics::new();

        let step_id = "test";
        diagnostics.add(diagnostic!(
            DiagnosticKind::DuplicateStepId,
            "Duplicate step ID '{step_id}'",
            { step_id }
        ));
        diagnostics.add(
            diagnostic!(
                DiagnosticKind::UnvalidatedFieldAccess,
                "Field access cannot be validated"
            )
            .experimental(),
        );

        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics.shown_by_default().count(), 1);
    }
}
