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

use crate::args::{load, ConfigArgs};
use crate::{MainError, Result, stepflow_config::StepflowConfig};
use error_stack::ResultExt as _;
use std::path::Path;
use stepflow_analysis::{validate_workflow, DiagnosticLevel};
use stepflow_core::workflow::Flow;

/// Validate workflow files and configuration
///
/// Returns the number of validation failures (errors + fatal diagnostics).
pub async fn validate(
    flow_path: Option<&Path>,
    config_path: Option<&Path>,
    config_only: bool,
    flow_only: bool,
) -> Result<usize> {
    let mut failures = 0;

    // Validate arguments
    if config_only && flow_only {
        return Err(MainError::InvalidArgument(
            "Cannot specify both --config-only and --flow-only".to_string(),
        )
        .into());
    }

    if flow_only && flow_path.is_none() {
        return Err(MainError::InvalidArgument(
            "Must specify --flow when using --flow-only".to_string(),
        )
        .into());
    }

    // Validate configuration unless flow-only is specified
    if !flow_only {
        match validate_config(config_path).await {
            Ok(config_failures) => {
                failures += config_failures;
            }
            Err(e) => {
                print_error("Configuration validation failed", &e);
                failures += 1;
            }
        }
    }

    // Validate workflow unless config-only is specified
    if !config_only {
        if let Some(flow_path) = flow_path {
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
        } else if !config_only {
            println!(
                "Warning: No workflow file specified for validation. Use --flow to specify a workflow file."
            );
        }
    }

    // Print summary
    if failures == 0 {
        println!("✅ Validation passed with no errors");
    } else {
        println!("❌ Validation completed with {} failure(s)", failures);
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
                println!("❌ Configuration has {} issue(s)", failures);
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
    if config.routing.rules.is_empty() {
        println!("❌ No routing rules configured in stepflow-config.yml");
        failures += 1;
    } else {
        println!("✅ Found {} routing rule(s) configured", config.routing.rules.len());
    }

    // Validate routing rules reference existing plugins
    for (index, rule) in config.routing.rules.iter().enumerate() {
        if !config.plugins.contains_key(&rule.target) {
            println!(
                "❌ Routing rule {} references unknown plugin '{}'",
                index + 1,
                rule.target
            );
            failures += 1;
        }
    }

    // Check for unused plugins (plugins not referenced by any routing rule)
    for plugin_name in config.plugins.keys() {
        let is_referenced = config.routing.rules.iter().any(|rule| rule.target == *plugin_name);
        if !is_referenced {
            println!(
                "⚠️  Plugin '{}' is not referenced by any routing rule",
                plugin_name
            );
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
    let diagnostics = validate_workflow(&flow)
        .change_context(MainError::ValidationError(
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
            "📊 Validation results: {} fatal, {} errors, {} warnings",
            fatal_count, error_count, warning_count
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
    println!("❌ {}: {}", context, error);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

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