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

#![allow(clippy::print_stdout)]

use stepflow_analysis::{DiagnosticLevel, Diagnostics};

/// Display validation diagnostics and return the number of failures (fatal + error)
pub fn display_diagnostics(diagnostics: &Diagnostics) -> usize {
    let (fatal_count, error_count, warning_count) = diagnostics.counts();
    let failure_count = fatal_count + error_count;

    // Display diagnostics
    if diagnostics.is_empty() {
        println!("✅ Validation passed with no errors");
    } else {
        println!(
            "📊 Validation results: {fatal_count} fatal, {error_count} errors, {warning_count} warnings"
        );

        // Group diagnostics by level
        let fatal_diagnostics = diagnostics.at_level(DiagnosticLevel::Fatal);
        let error_diagnostics = diagnostics.at_level(DiagnosticLevel::Error);
        let warning_diagnostics = diagnostics.at_level(DiagnosticLevel::Warning);

        // Print fatal diagnostics
        if !fatal_diagnostics.is_empty() {
            println!("\n🚨 Fatal Issues:");
            for diagnostic in fatal_diagnostics {
                print_diagnostic("FATAL", diagnostic);
            }
        }

        // Print error diagnostics
        if !error_diagnostics.is_empty() {
            println!("\n❌ Errors:");
            for diagnostic in error_diagnostics {
                print_diagnostic("ERROR", diagnostic);
            }
        }

        // Print warning diagnostics
        if !warning_diagnostics.is_empty() {
            println!("\n⚠️  Warnings:");
            for diagnostic in warning_diagnostics {
                print_diagnostic("WARN", diagnostic);
            }
        }
    }

    failure_count
}

/// Print a formatted diagnostic message
fn print_diagnostic(level: &str, diagnostic: &stepflow_analysis::Diagnostic) {
    let path_str = if diagnostic.path.is_empty() {
        String::new()
    } else {
        format!(" ({})", diagnostic.path.join("."))
    };

    println!("  {} {}{}", level, diagnostic.text, path_str);
}
