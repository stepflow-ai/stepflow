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

use std::path::PathBuf;

use error_stack::ResultExt as _;
use stepflow_core::workflow::WorkflowOverrides;

use crate::error::{MainError, Result};

/// Command line arguments for specifying workflow overrides.
#[derive(clap::Args, Debug, Clone)]
pub struct OverrideArgs {
    /// Path to a JSON or YAML file containing workflow overrides.
    #[arg(
        long = "overrides",
        value_name = "FILE",
        value_hint = clap::ValueHint::FilePath,
        conflicts_with_all = ["overrides_json", "overrides_yaml", "overrides_stdin"]
    )]
    pub overrides_file: Option<PathBuf>,

    /// Workflow overrides as inline JSON.
    #[arg(
        long = "overrides-json",
        value_name = "JSON",
        conflicts_with_all = ["overrides_file", "overrides_yaml", "overrides_stdin"]
    )]
    pub overrides_json: Option<String>,

    /// Workflow overrides as inline YAML.
    #[arg(
        long = "overrides-yaml",
        value_name = "YAML",
        conflicts_with_all = ["overrides_file", "overrides_json", "overrides_stdin"]
    )]
    pub overrides_yaml: Option<String>,

    /// Read workflow overrides from stdin (format auto-detected).
    #[arg(
        long = "overrides-stdin",
        conflicts_with_all = ["overrides_file", "overrides_json", "overrides_yaml"]
    )]
    pub overrides_stdin: bool,
}

impl OverrideArgs {
    /// Parse the overrides from the provided arguments.
    ///
    /// Returns empty overrides if no override arguments are provided.
    pub fn parse_overrides(&self) -> Result<WorkflowOverrides> {
        if let Some(file_path) = &self.overrides_file {
            self.load_overrides_from_file(file_path)
        } else if let Some(json_str) = &self.overrides_json {
            self.parse_overrides_from_json(json_str)
        } else if let Some(yaml_str) = &self.overrides_yaml {
            self.parse_overrides_from_yaml(yaml_str)
        } else if self.overrides_stdin {
            self.read_overrides_from_stdin()
        } else {
            // No overrides provided
            Ok(WorkflowOverrides::new())
        }
    }

    /// Load overrides from a file (JSON or YAML based on extension).
    fn load_overrides_from_file(&self, file_path: &PathBuf) -> Result<WorkflowOverrides> {
        let contents = std::fs::read_to_string(file_path)
            .change_context(MainError::Configuration)
            .attach_printable_lazy(|| format!("Failed to read overrides file: {:?}", file_path))?;

        // Determine format from file extension
        let extension = file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");

        match extension.to_lowercase().as_str() {
            "json" => self.parse_overrides_from_json(&contents),
            "yaml" | "yml" => self.parse_overrides_from_yaml(&contents),
            _ => {
                // Try JSON first, fall back to YAML
                match self.parse_overrides_from_json(&contents) {
                    Ok(overrides) => Ok(overrides),
                    Err(_) => {
                        log::debug!("Failed to parse as JSON, trying YAML");
                        self.parse_overrides_from_yaml(&contents)
                    }
                }
            }
        }
    }

    /// Parse overrides from a JSON string.
    fn parse_overrides_from_json(&self, json_str: &str) -> Result<WorkflowOverrides> {
        serde_json::from_str(json_str)
            .change_context(MainError::Configuration)
            .attach_printable("Failed to parse overrides as JSON")
    }

    /// Parse overrides from a YAML string.
    fn parse_overrides_from_yaml(&self, yaml_str: &str) -> Result<WorkflowOverrides> {
        serde_yaml_ng::from_str(yaml_str)
            .change_context(MainError::Configuration)
            .attach_printable("Failed to parse overrides as YAML")
    }

    /// Read overrides from stdin with format auto-detection.
    fn read_overrides_from_stdin(&self) -> Result<WorkflowOverrides> {
        use std::io::Read as _;

        let mut contents = String::new();
        std::io::stdin()
            .read_to_string(&mut contents)
            .change_context(MainError::Configuration)
            .attach_printable("Failed to read overrides from stdin")?;

        // Try JSON first, fall back to YAML
        match self.parse_overrides_from_json(contents.trim()) {
            Ok(overrides) => Ok(overrides),
            Err(_) => {
                log::debug!("Stdin content is not valid JSON, trying YAML");
                self.parse_overrides_from_yaml(contents.trim())
                    .attach_printable("Failed to parse stdin content as either JSON or YAML")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_empty_overrides() {
        let args = OverrideArgs {
            overrides_file: None,
            overrides_json: None,
            overrides_yaml: None,
            overrides_stdin: false,
        };

        let overrides = args.parse_overrides().unwrap();
        assert!(overrides.is_empty());
    }

    #[test]
    fn test_parse_json_overrides() {
        let json_str = r#"
        {
            "stepOverrides": {
                "llm_call": {
                    "temperature": 0.9,
                    "model": "gpt-4"
                },
                "processing": {
                    "batch_size": 32
                }
            }
        }"#;

        let args = OverrideArgs {
            overrides_file: None,
            overrides_json: Some(json_str.to_string()),
            overrides_yaml: None,
            overrides_stdin: false,
        };

        let overrides = args.parse_overrides().unwrap();
        assert!(!overrides.is_empty());
        assert!(overrides.has_step_overrides("llm_call"));
        assert!(overrides.has_step_overrides("processing"));

        let llm_overrides = overrides.get_step_overrides("llm_call").unwrap();
        assert!(llm_overrides.get_override("temperature").is_some());
        assert!(llm_overrides.get_override("model").is_some());
    }

    #[test]
    fn test_parse_yaml_overrides() {
        let yaml_str = r#"
stepOverrides:
  llm_call:
    temperature: 0.7
    model: "gpt-3.5-turbo"
  processing:
    debug: true
"#;

        let args = OverrideArgs {
            overrides_file: None,
            overrides_json: None,
            overrides_yaml: Some(yaml_str.to_string()),
            overrides_stdin: false,
        };

        let overrides = args.parse_overrides().unwrap();
        assert!(!overrides.is_empty());
        assert!(overrides.has_step_overrides("llm_call"));
        assert!(overrides.has_step_overrides("processing"));
    }

    #[test]
    fn test_parse_invalid_json() {
        let args = OverrideArgs {
            overrides_file: None,
            overrides_json: Some("{ invalid json".to_string()),
            overrides_yaml: None,
            overrides_stdin: false,
        };

        let result = args.parse_overrides();
        assert!(result.is_err());
    }

    #[test]
    fn test_load_from_file() {
        // Create a temporary file with JSON overrides
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("overrides.json");

        let overrides_content = json!({
            "stepOverrides": {
                "test_step": {
                    "param1": "value1",
                    "param2": 42
                }
            }
        });

        std::fs::write(
            &file_path,
            serde_json::to_string_pretty(&overrides_content).unwrap(),
        )
        .unwrap();

        let args = OverrideArgs {
            overrides_file: Some(file_path),
            overrides_json: None,
            overrides_yaml: None,
            overrides_stdin: false,
        };

        let overrides = args.parse_overrides().unwrap();
        assert!(overrides.has_step_overrides("test_step"));
    }

    #[test]
    fn test_environment_variable_literals() {
        // NOTE: Currently, environment variables are not automatically substituted
        // in override values. They are treated as literal strings.
        // Future enhancement could add environment variable substitution.

        let json_str = r#"
        {
            "stepOverrides": {
                "secret_step": {
                    "api_key": "$SECRET_API_KEY",
                    "env_var": "${HOME}/config"
                }
            }
        }"#;

        let args = OverrideArgs {
            overrides_file: None,
            overrides_json: Some(json_str.to_string()),
            overrides_yaml: None,
            overrides_stdin: false,
        };

        let overrides = args.parse_overrides().unwrap();
        assert!(overrides.has_step_overrides("secret_step"));

        let step_overrides = overrides.get_step_overrides("secret_step").unwrap();

        // Verify that environment variables are currently treated as literals
        if let Some(api_key_override) = step_overrides.get_override("api_key") {
            // This will be the literal string "$SECRET_API_KEY", not the env var value
            if let stepflow_core::values::ValueTemplateRepr::String(literal) =
                api_key_override.as_ref()
            {
                assert_eq!(literal, "$SECRET_API_KEY");
            } else {
                panic!("Expected string literal, got: {:?}", api_key_override);
            }
        }
    }
}
