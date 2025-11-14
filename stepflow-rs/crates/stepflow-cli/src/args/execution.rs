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

use super::{OverrideArgs, VariableArgs};

/// Shared execution arguments used by commands that run workflows.
/// Contains variables and overrides but not input (input is handled separately).
#[derive(clap::Args, Debug, Clone, Default)]
pub struct ExecutionArgs {
    /// Variable arguments (file, JSON, YAML, environment fallback)
    #[command(flatten)]
    pub variable_args: VariableArgs,

    /// Override arguments (file, JSON, YAML)
    #[command(flatten)]
    pub override_args: OverrideArgs,
}

impl ExecutionArgs {
    /// Create ExecutionArgs for REPL usage
    pub fn for_repl(
        variables: Option<String>,
        variables_json: Option<String>,
        variables_yaml: Option<String>,
        env_variables: bool,
        overrides: Option<String>,
        overrides_json: Option<String>,
        overrides_yaml: Option<String>,
    ) -> Self {
        Self {
            variable_args: VariableArgs::for_repl(
                variables,
                variables_json,
                variables_yaml,
                env_variables,
            ),
            override_args: OverrideArgs::for_repl(overrides, overrides_json, overrides_yaml),
        }
    }

    /// Check if any variables are provided
    pub fn has_variables(&self) -> bool {
        self.variable_args.has_variables()
    }

    /// Check if any overrides are provided
    pub fn has_overrides(&self) -> bool {
        self.override_args.has_overrides()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_args_default() {
        let args = ExecutionArgs::default();
        assert!(!args.has_variables());
        assert!(!args.has_overrides());
    }

    #[test]
    fn test_execution_args_has_variables() {
        let args = ExecutionArgs {
            variable_args: VariableArgs {
                variables_json: Some(r#"{"api_key": "test"}"#.to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(args.has_variables());
        assert!(!args.has_overrides());
    }

    #[test]
    fn test_execution_args_has_overrides() {
        let args = ExecutionArgs {
            override_args: OverrideArgs {
                overrides_json: Some(r#"{"step1": {"value": {}}}"#.to_string()),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(!args.has_variables());
        assert!(args.has_overrides());
    }

    #[test]
    fn test_for_repl() {
        let args = ExecutionArgs::for_repl(
            Some("vars.json".to_string()),
            None,
            None,
            true,
            Some("overrides.yaml".to_string()),
            None,
            None,
        );
        assert!(args.has_variables());
        assert!(args.has_overrides());
        assert!(args.variable_args.env_variables);
    }
}
