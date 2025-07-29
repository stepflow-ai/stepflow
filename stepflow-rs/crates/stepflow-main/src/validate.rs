#![allow(clippy::print_stdout)]
// Licensed to the Apache Software Foundation (ASF) under one or more contributor license agreements.
// See the NOTICE file distributed with this work for additional information regarding copyright
// ownership.  The ASF licenses this file to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance with the License.  You may obtain a
// copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied.  See the License for the specific language governing permissions and limitations under
// the License.

use crate::args::{ConfigArgs, load};
use crate::{MainError, Result, stepflow_config::StepflowConfig};
use error_stack::ResultExt as _;
use std::path::Path;
use stepflow_analysis::{DiagnosticLevel, validate_workflow};
use stepflow_core::workflow::Flow;

/// Validate workflow files and configuration
///
/// Returns the number of validation failures (errors + fatal diagnostics).
pub async fn validate(flow_path: &Path, config_path: Option<&Path>) -> Result<usize> {
    let mut failures = 0;

    // Validate configuration
    match validate_config(config_path).await {
        Ok(config_failures) => {
            failures += config_failures;
        }
        Err(e) => {
            print_error("Configuration validation failed", &e);
            failures += 1;
        }
    }

    // Validate workflow
    match validate_flow(flow_path).await {
        Ok(flow_failures) => {
            failures += flow_failures;
        }
        Err(e) => {
            print_error(
                &format!("Workflow validation failed for {}", flow_path.display()),
                &e,
            );
            failures += 1;
        }
    }

    // Print summary
    if failures == 0 {
        println!("✅ Validation passed with no errors");
    } else {
        println!("❌ Validation completed with {failures} failure(s)");
    }

    Ok(failures)
}

/// Validate a StepFlow configuration file
async fn validate_config(config_path: Option<&Path>) -> Result<usize> {
    println!("🔧 Validating configuration...");

    // Try to load the configuration
    let config_args = ConfigArgs {
        config_path: config_path.map(|p| p.to_path_buf()),
    };

    match config_args.load_config(None) {
        Ok(config) => {
            let failures = validate_config_structure(&config);
            if failures == 0 {
                println!("✅ Configuration is valid");
            } else {
                println!("❌ Configuration has {failures} issue(s)");
            }
            Ok(failures)
        }
        Err(e) => {
            print_error("Failed to load configuration", &e);
            Ok(1)
        }
    }
}

/// Validate the structure and content of a loaded configuration
fn validate_config_structure(config: &StepflowConfig) -> usize {
    let mut failures = 0;

    // Check that at least one plugin is configured
    if config.plugins.is_empty() {
        println!("❌ No plugins configured in stepflow-config.yml");
        failures += 1;
    } else {
        println!("✅ Found {} plugin(s) configured", config.plugins.len());
    }

    // Check that routing rules exist
    if config.routing.routes.is_empty() {
        println!("❌ No routing rules configured in stepflow-config.yml");
        failures += 1;
    } else {
        let total_rules: usize = config
            .routing
            .routes
            .values()
            .map(|rules| rules.len())
            .sum();
        println!("✅ Found {total_rules} routing rule(s) configured");
    }

    // Validate routing rules reference existing plugins
    for (path, rules) in &config.routing.routes {
        for (rule_index, rule) in rules.iter().enumerate() {
            if !config.plugins.contains_key(rule.plugin.as_ref()) {
                println!(
                    "❌ Routing rule {} for path '{}' references unknown plugin '{}'",
                    rule_index + 1,
                    path,
                    rule.plugin
                );
                failures += 1;
            }
        }
    }

    // Check for unused plugins (plugins not referenced by any routing rule)
    for plugin_name in config.plugins.keys() {
        let is_referenced = config
            .routing
            .routes
            .values()
            .flatten()
            .any(|rule| rule.plugin.as_ref() == plugin_name);
        if !is_referenced {
            println!("⚠️  Plugin '{plugin_name}' is not referenced by any routing rule");
            // This is a warning, not a failure
        }
    }

    failures
}

/// Validate a workflow file
async fn validate_flow(flow_path: &Path) -> Result<usize> {
    println!("📋 Validating workflow: {}", flow_path.display());

    // Load the workflow
    let flow: Flow = load(flow_path)?;

    // Run workflow validation using stepflow-analysis
    let diagnostics = validate_workflow(&flow).change_context(MainError::ValidationError(
        "Workflow validation failed".to_string(),
    ))?;

    // Count failures and display results
    let (fatal_count, error_count, warning_count) = diagnostics.counts();
    let failure_count = fatal_count + error_count;

    // Display diagnostics
    if diagnostics.is_empty() {
        println!("✅ Workflow is valid");
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

    Ok(failure_count)
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

/// Print a formatted error message
fn print_error(context: &str, error: &error_stack::Report<MainError>) {
    println!("❌ {context}: {error}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_validate_minimal_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("stepflow-config.yml");

        // Create a minimal valid config
        let config_content = r#"
plugins:
  builtin:
    type: builtin

routing:
  - match: "*"
    target: builtin
"#;
        fs::write(&config_path, config_content).unwrap();

        let failures = validate_config(Some(&config_path)).await.unwrap();
        assert_eq!(failures, 0);
    }

    #[tokio::test]
    async fn test_validate_invalid_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("stepflow-config.yml");

        // Create an invalid config (routing references non-existent plugin)
        let config_content = r#"
plugins:
  builtin:
    type: builtin

routing:
  - match: "*"
    target: nonexistent
"#;
        fs::write(&config_path, config_content).unwrap();

        let failures = validate_config(Some(&config_path)).await.unwrap();
        assert!(failures > 0);
    }
}
