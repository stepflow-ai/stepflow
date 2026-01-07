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

use error_stack::ResultExt as _;
use std::{collections::HashMap, env, path::PathBuf};
use stepflow_core::workflow::ValueRef;

use crate::{MainError, Result, args::file_loader::load};

/// Shared variable arguments used by multiple commands
#[derive(clap::Args, Debug, Clone, Default)]
pub struct VariableArgs {
    /// The path to the variables file.
    ///
    /// Should be JSON or YAML. Format is inferred from file extension.
    #[arg(long = "variables", value_name = "FILE", value_hint = clap::ValueHint::FilePath,
          conflicts_with_all = ["variables_json", "variables_yaml"])]
    pub variables: Option<PathBuf>,

    /// The variables as a JSON string.
    #[arg(long = "variables-json", value_name = "JSON",
          conflicts_with_all = ["variables", "variables_yaml"])]
    pub variables_json: Option<String>,

    /// The variables as a YAML string.
    #[arg(long = "variables-yaml", value_name = "YAML",
          conflicts_with_all = ["variables", "variables_json"])]
    pub variables_yaml: Option<String>,

    /// Enable environment variable fallback for missing variables.
    ///
    /// When enabled, missing variables will be looked up from environment
    /// variables using the pattern `STEPFLOW_VAR_<VARIABLE_NAME>`.
    #[arg(long = "env-variables", action = clap::ArgAction::SetTrue)]
    pub env_variables: bool,
}

impl VariableArgs {
    /// Parse variables from the various sources into a HashMap
    ///
    /// This method will:
    /// 1. Load variables from explicit sources (file, JSON, YAML)
    /// 2. If env_variables is enabled, add missing variables from environment
    ///
    /// # Arguments
    ///
    /// * `required_variables` - List of variables required by the workflow
    pub fn parse_variables(
        &self,
        required_variables: &[String],
    ) -> Result<HashMap<String, ValueRef>> {
        // First, load explicitly provided variables
        let mut variables = match (&self.variables, &self.variables_json, &self.variables_yaml) {
            (Some(path), None, None) => {
                // Load from file
                let value_ref = load(path)?;
                self.value_ref_to_variables(value_ref)?
            }
            (None, Some(json), None) => {
                // Parse JSON string
                let value_ref: ValueRef = serde_json::from_str(json)
                    .change_context(MainError::ReplCommand("Invalid JSON variables".to_string()))?;
                self.value_ref_to_variables(value_ref)?
            }
            (None, None, Some(yaml)) => {
                // Parse YAML string
                let value_ref: ValueRef = serde_yaml_ng::from_str(yaml)
                    .change_context(MainError::ReplCommand("Invalid YAML variables".to_string()))?;
                self.value_ref_to_variables(value_ref)?
            }
            (None, None, None) => {
                // No explicit variables provided
                HashMap::new()
            }
            _ => {
                return Err(MainError::ReplCommand(
                    "Only one variables source allowed".to_string(),
                )
                .into());
            }
        };

        // If env_variables is enabled, check for missing variables in environment
        if self.env_variables {
            for var_name in required_variables {
                if !variables.contains_key(var_name) {
                    let env_var_name = format!("STEPFLOW_VAR_{}", var_name.to_uppercase());
                    if let Ok(env_value) = env::var(&env_var_name) {
                        // Try to parse as JSON first, fall back to string
                        let value_ref = if let Ok(json_value) =
                            serde_json::from_str::<serde_json::Value>(&env_value)
                        {
                            ValueRef::new(json_value)
                        } else {
                            ValueRef::new(serde_json::Value::String(env_value))
                        };
                        variables.insert(var_name.clone(), value_ref);
                    }
                }
            }
        }

        Ok(variables)
    }

    /// Convert a ValueRef to a HashMap of variables
    ///
    /// Expects the ValueRef to contain a JSON object where keys are variable names
    /// and values are the variable values.
    fn value_ref_to_variables(&self, value_ref: ValueRef) -> Result<HashMap<String, ValueRef>> {
        let json_value = value_ref.as_ref();
        if let Some(object) = json_value.as_object() {
            let mut variables = HashMap::new();
            for (key, value) in object {
                variables.insert(key.clone(), ValueRef::new(value.clone()));
            }
            Ok(variables)
        } else {
            Err(MainError::ReplCommand("Variables must be a JSON object".to_string()).into())
        }
    }

    /// Check if any variables are explicitly provided
    pub fn has_variables(&self) -> bool {
        self.variables.is_some() || self.variables_json.is_some() || self.variables_yaml.is_some()
    }

    /// Create VariableArgs for REPL usage
    pub fn for_repl(
        variables: Option<String>,
        variables_json: Option<String>,
        variables_yaml: Option<String>,
        env_variables: bool,
    ) -> Self {
        Self {
            variables: variables.map(PathBuf::from),
            variables_json,
            variables_yaml,
            env_variables,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_variable_args_default() {
        let args = VariableArgs::default();
        assert!(!args.has_variables());
        assert!(args.variables.is_none());
        assert!(args.variables_json.is_none());
        assert!(args.variables_yaml.is_none());
        assert!(!args.env_variables);
    }

    #[test]
    fn test_variable_args_has_variables() {
        let args_with_file = VariableArgs {
            variables: Some(PathBuf::from("vars.json")),
            ..Default::default()
        };
        assert!(args_with_file.has_variables());

        let args_with_json = VariableArgs {
            variables_json: Some(r#"{"api_key": "test-key"}"#.to_string()),
            ..Default::default()
        };
        assert!(args_with_json.has_variables());

        let args_with_yaml = VariableArgs {
            variables_yaml: Some("api_key: test-key".to_string()),
            ..Default::default()
        };
        assert!(args_with_yaml.has_variables());
    }

    #[test]
    fn test_parse_variables_json_string() {
        let args = VariableArgs {
            variables_json: Some(r#"{"api_key": "test-key", "temperature": 0.7}"#.to_string()),
            ..Default::default()
        };
        let result = args.parse_variables(&[]).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains_key("api_key"));
        assert!(result.contains_key("temperature"));

        let api_key_value = serde_json::to_value(result["api_key"].clone()).unwrap();
        assert_eq!(api_key_value, "test-key");
    }

    #[test]
    fn test_parse_variables_yaml_string() {
        let args = VariableArgs {
            variables_yaml: Some("api_key: test-key\ntemperature: 0.7".to_string()),
            ..Default::default()
        };
        let result = args.parse_variables(&[]).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains_key("api_key"));
        assert!(result.contains_key("temperature"));
    }

    #[test]
    fn test_parse_variables_invalid_json() {
        let args = VariableArgs {
            variables_json: Some("invalid json".to_string()),
            ..Default::default()
        };
        let result = args.parse_variables(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_variables_non_object() {
        let args = VariableArgs {
            variables_json: Some(r#"["not", "an", "object"]"#.to_string()),
            ..Default::default()
        };
        let result = args.parse_variables(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_variables_no_variables() {
        let args = VariableArgs::default();
        let result = args.parse_variables(&[]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_env_variables_fallback() {
        // Set a test environment variable
        // SAFETY: This test runs single-threaded and immediately cleans up the env var after use
        unsafe {
            env::set_var("STEPFLOW_VAR_TEST_KEY", "env-value");
        }

        let args = VariableArgs {
            env_variables: true,
            ..Default::default()
        };

        let required_vars = vec!["test_key".to_string()];
        let result = args.parse_variables(&required_vars).unwrap();

        assert_eq!(result.len(), 1);
        assert!(result.contains_key("test_key"));

        let value = serde_json::to_value(result["test_key"].clone()).unwrap();
        assert_eq!(value, "env-value");

        // Clean up
        // SAFETY: This test runs single-threaded and this is the cleanup for the set_var above
        unsafe {
            env::remove_var("STEPFLOW_VAR_TEST_KEY");
        }
    }

    #[test]
    fn test_env_variables_json_parsing() {
        // Set environment variable with JSON value
        // SAFETY: This test runs single-threaded and immediately cleans up the env var after use
        unsafe {
            env::set_var("STEPFLOW_VAR_CONFIG", r#"{"nested": true, "value": 42}"#);
        }

        let args = VariableArgs {
            env_variables: true,
            ..Default::default()
        };

        let required_vars = vec!["config".to_string()];
        let result = args.parse_variables(&required_vars).unwrap();

        assert_eq!(result.len(), 1);
        let value = serde_json::to_value(result["config"].clone()).unwrap();
        assert_eq!(value["nested"], true);
        assert_eq!(value["value"], 42);

        // Clean up
        // SAFETY: This test runs single-threaded and this is the cleanup for the set_var above
        unsafe {
            env::remove_var("STEPFLOW_VAR_CONFIG");
        }
    }

    #[test]
    fn test_for_repl() {
        let args = VariableArgs::for_repl(
            Some("vars.json".to_string()),
            Some(r#"{"test": true}"#.to_string()),
            None,
            true,
        );
        assert_eq!(args.variables, Some(PathBuf::from("vars.json")));
        assert_eq!(args.variables_json, Some(r#"{"test": true}"#.to_string()));
        assert!(args.variables_yaml.is_none());
        assert!(args.env_variables);
    }
}
