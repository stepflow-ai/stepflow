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
pub fn display_diagnostics(diagnostics: &Diagnostics) -> u32 {
    let fatal_count = diagnostics.num_fatal;
    let error_count = diagnostics.num_error;
    let warning_count = diagnostics.num_warning;

    let failure_count = fatal_count + error_count;

    // Display diagnostics
    if diagnostics.is_empty() {
        println!("✅ Validation passed with no errors");
    } else {
        println!(
            "📊 Validation results: {fatal_count} fatal, {error_count} errors, {warning_count} warnings"
        );

        // Print fatal diagnostics
        if fatal_count > 0 {
            println!("\n🚨 Fatal Issues:");
            for diagnostic in diagnostics.at_level(DiagnosticLevel::Fatal) {
                print_diagnostic("FATAL", diagnostic);
            }
        }

        // Print error diagnostics
        if error_count > 0 {
            println!("\n❌ Errors:");
            for diagnostic in diagnostics.at_level(DiagnosticLevel::Error) {
                print_diagnostic("ERROR", diagnostic);
            }
        }

        // Print warning diagnostics
        if warning_count > 0 {
            println!("\n⚠️  Warnings:");
            for diagnostic in diagnostics.at_level(DiagnosticLevel::Warning) {
                print_diagnostic("WARN", diagnostic);
            }
        }
    }

    failure_count
}

/// Print a formatted diagnostic message
fn print_diagnostic(level: &str, diagnostic: &stepflow_analysis::Diagnostic) {
    println!("  {} {} ({})", level, diagnostic.formatted, diagnostic.path);
}
