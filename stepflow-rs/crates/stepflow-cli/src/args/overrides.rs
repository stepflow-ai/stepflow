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

use crate::{MainError, Result, args::file_loader::load};

/// Shared override arguments used by multiple commands
#[derive(clap::Args, Debug, Clone, Default)]
pub struct OverrideArgs {
    /// Path to a file containing workflow overrides (JSON or YAML format).
    ///
    /// Overrides allow you to modify step properties at runtime without changing
    /// the original workflow file. Format is inferred from file extension.
    #[arg(long = "overrides", value_name = "FILE", value_hint = clap::ValueHint::FilePath,
          conflicts_with_all = ["overrides_json", "overrides_yaml"])]
    pub overrides_file: Option<PathBuf>,

    /// Workflow overrides as a JSON string.
    ///
    /// Specify overrides inline as JSON. Example:
    /// --overrides-json '{"step1": {"value": {"input": {"temperature": 0.8}}}}'
    #[arg(long = "overrides-json", value_name = "JSON",
          conflicts_with_all = ["overrides_file", "overrides_yaml"])]
    pub overrides_json: Option<String>,

    /// Workflow overrides as a YAML string.
    ///
    /// Specify overrides inline as YAML. Example:
    /// --overrides-yaml 'step1: {value: {input: {temperature: 0.8}}}'
    #[arg(long = "overrides-yaml", value_name = "YAML",
          conflicts_with_all = ["overrides_file", "overrides_json"])]
    pub overrides_yaml: Option<String>,
}

impl OverrideArgs {
    /// Parse overrides from the various sources into WorkflowOverrides
    ///
    /// Returns empty overrides if no override sources are specified.
    pub fn parse_overrides(&self) -> Result<WorkflowOverrides> {
        match (
            &self.overrides_file,
            &self.overrides_json,
            &self.overrides_yaml,
        ) {
            (Some(path), None, None) => {
                // Load from file
                load(path)
            }
            (None, Some(json), None) => {
                // Parse JSON string
                serde_json::from_str(json)
                    .change_context(MainError::ReplCommand("Invalid JSON overrides".to_string()))
            }
            (None, None, Some(yaml)) => {
                // Parse YAML string
                serde_yaml_ng::from_str(yaml)
                    .change_context(MainError::ReplCommand("Invalid YAML overrides".to_string()))
            }
            (None, None, None) => {
                // No overrides specified
                Ok(WorkflowOverrides::new())
            }
            _ => {
                // This should be prevented by clap conflicts_with_all, but just in case
                Err(MainError::ReplCommand("Only one override type allowed".to_string()).into())
            }
        }
    }

    /// Check if any overrides are provided
    pub fn has_overrides(&self) -> bool {
        self.overrides_file.is_some()
            || self.overrides_json.is_some()
            || self.overrides_yaml.is_some()
    }

    /// Create OverrideArgs for REPL usage
    pub fn for_repl(
        overrides_file: Option<String>,
        overrides_json: Option<String>,
        overrides_yaml: Option<String>,
    ) -> Self {
        Self {
            overrides_file: overrides_file.map(PathBuf::from),
            overrides_json,
            overrides_yaml,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_override_args_default() {
        let args = OverrideArgs::default();
        assert!(!args.has_overrides());
        assert!(args.overrides_file.is_none());
        assert!(args.overrides_json.is_none());
        assert!(args.overrides_yaml.is_none());
    }

    #[test]
    fn test_override_args_has_overrides() {
        let args_with_file = OverrideArgs {
            overrides_file: Some(PathBuf::from("overrides.json")),
            ..Default::default()
        };
        assert!(args_with_file.has_overrides());

        let args_with_json = OverrideArgs {
            overrides_json: Some(r#"{"step1": {"value": {"temperature": 0.8}}}"#.to_string()),
            ..Default::default()
        };
        assert!(args_with_json.has_overrides());

        let args_with_yaml = OverrideArgs {
            overrides_yaml: Some("step1: {value: {temperature: 0.8}}".to_string()),
            ..Default::default()
        };
        assert!(args_with_yaml.has_overrides());
    }

    #[test]
    fn test_parse_overrides_empty() {
        let args = OverrideArgs::default();
        let overrides = args.parse_overrides().unwrap();
        assert!(overrides.is_empty());
    }

    #[test]
    fn test_parse_overrides_json_string() {
        let args = OverrideArgs {
            overrides_json: Some(
                r#"{"step1": {"value": {"input": {"temperature": 0.8}}}}"#.to_string(),
            ),
            ..Default::default()
        };
        let overrides = args.parse_overrides().unwrap();
        assert!(!overrides.is_empty());
        assert!(overrides.steps.contains_key("step1"));
    }

    #[test]
    fn test_parse_overrides_yaml_string() {
        let args = OverrideArgs {
            overrides_yaml: Some(
                "step1:\n  value:\n    input:\n      temperature: 0.8".to_string(),
            ),
            ..Default::default()
        };
        let overrides = args.parse_overrides().unwrap();
        assert!(!overrides.is_empty());
        assert!(overrides.steps.contains_key("step1"));
    }

    #[test]
    fn test_parse_overrides_invalid_json() {
        let args = OverrideArgs {
            overrides_json: Some("invalid json".to_string()),
            ..Default::default()
        };
        let result = args.parse_overrides();
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_overrides_invalid_yaml() {
        let args = OverrideArgs {
            overrides_yaml: Some("invalid: yaml: content: :::".to_string()),
            ..Default::default()
        };
        let result = args.parse_overrides();
        assert!(result.is_err());
    }

    #[test]
    fn test_for_repl() {
        let args = OverrideArgs::for_repl(
            Some("overrides.yaml".to_string()),
            None,
            Some("step1: {value: {temperature: 0.8}}".to_string()),
        );
        assert_eq!(args.overrides_file, Some(PathBuf::from("overrides.yaml")));
        assert!(args.overrides_json.is_none());
        assert_eq!(
            args.overrides_yaml,
            Some("step1: {value: {temperature: 0.8}}".to_string())
        );
    }
}
