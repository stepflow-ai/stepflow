use error_stack::ResultExt as _;
use std::path::PathBuf;
use stepflow_core::workflow::ValueRef;

use crate::{MainError, Result, args::file_loader::load};

/// Input format for stdin
#[derive(clap::ValueEnum, Clone, Debug, Default)]
pub enum InputFormat {
    #[default]
    Json,
    Yaml,
}

/// Shared input arguments used by multiple commands
#[derive(clap::Args, Debug, Clone, Default)]
pub struct InputArgs {
    /// The path to the input file to execute the workflow with.
    ///
    /// Should be JSON or YAML. Format is inferred from file extension.
    #[arg(long = "input", value_name = "FILE", value_hint = clap::ValueHint::FilePath, 
          conflicts_with_all = ["input_json", "input_yaml", "stdin_format"])]
    pub input: Option<PathBuf>,

    /// The input value as a JSON string.
    #[arg(long = "input-json", value_name = "JSON", 
          conflicts_with_all = ["input", "input_yaml", "stdin_format"])]
    pub input_json: Option<String>,

    /// The input value as a YAML string.
    #[arg(long = "input-yaml", value_name = "YAML", 
          conflicts_with_all = ["input", "input_json", "stdin_format"])]
    pub input_yaml: Option<String>,

    /// The format for stdin input (json or yaml).
    ///
    /// Only used when reading from stdin (no other input options specified).
    #[arg(long = "stdin-format", value_name = "FORMAT", default_value = "json",
          conflicts_with_all = ["input", "input_json", "input_yaml"])]
    pub stdin_format: InputFormat,
}

impl InputArgs {
    /// Parse input from the various input sources into a ValueRef
    ///
    /// # Arguments
    ///
    /// * `allow_stdin` - Whether to allow reading from stdin when no input is provided
    pub fn parse_input(&self, allow_stdin: bool) -> Result<ValueRef> {
        match (&self.input, &self.input_json, &self.input_yaml) {
            (Some(path), None, None) => {
                // Load from file
                load(path)
            }
            (None, Some(json), None) => {
                // Parse JSON string
                serde_json::from_str(json)
                    .change_context(MainError::ReplCommand("Invalid JSON input".to_string()))
            }
            (None, None, Some(yaml)) => {
                // Parse YAML string
                serde_yaml_ng::from_str(yaml)
                    .change_context(MainError::ReplCommand("Invalid YAML input".to_string()))
            }
            (None, None, None) => {
                if allow_stdin {
                    // Read from stdin using specified format (CLI only)
                    let stdin = std::io::stdin();
                    match self.stdin_format {
                        InputFormat::Json => serde_json::from_reader(stdin).change_context(
                            MainError::ReplCommand("Invalid JSON from stdin".to_string()),
                        ),
                        InputFormat::Yaml => serde_yaml_ng::from_reader(stdin).change_context(
                            MainError::ReplCommand("Invalid YAML from stdin".to_string()),
                        ),
                    }
                } else {
                    Err(MainError::ReplCommand("No input provided".to_string()).into())
                }
            }
            _ => Err(MainError::ReplCommand("Only one input type allowed".to_string()).into()),
        }
    }

    /// Check if any input is provided
    pub fn has_input(&self) -> bool {
        self.input.is_some() || self.input_json.is_some() || self.input_yaml.is_some()
    }

    /// Convert to JSON value for serialization
    pub fn to_json_value(&self) -> Result<serde_json::Value> {
        let value_ref = self.parse_input(true)?;
        Ok(serde_json::to_value(value_ref).unwrap())
    }

    /// Create InputArgs for REPL usage (no stdin support)
    pub fn for_repl(
        input: Option<String>,
        input_json: Option<String>,
        input_yaml: Option<String>,
    ) -> Self {
        Self {
            input: input.map(PathBuf::from),
            input_json,
            input_yaml,
            stdin_format: InputFormat::Json, // Default, but won't be used in REPL
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_input_args_default() {
        let args = InputArgs::default();
        assert!(!args.has_input());
        assert!(args.input.is_none());
        assert!(args.input_json.is_none());
        assert!(args.input_yaml.is_none());
    }

    #[test]
    fn test_input_args_has_input() {
        let args_with_file = InputArgs {
            input: Some(PathBuf::from("test.json")),
            ..Default::default()
        };
        assert!(args_with_file.has_input());

        let args_with_json = InputArgs {
            input_json: Some(r#"{"key": "value"}"#.to_string()),
            ..Default::default()
        };
        assert!(args_with_json.has_input());

        let args_with_yaml = InputArgs {
            input_yaml: Some("key: value".to_string()),
            ..Default::default()
        };
        assert!(args_with_yaml.has_input());
    }

    #[test]
    fn test_parse_input_json_string() {
        let args = InputArgs {
            input_json: Some(r#"{"number": 42, "text": "hello"}"#.to_string()),
            ..Default::default()
        };
        let result = args.parse_input(false).unwrap();
        let json_value = serde_json::to_value(result).unwrap();
        assert_eq!(json_value["number"], 42);
        assert_eq!(json_value["text"], "hello");
    }

    #[test]
    fn test_parse_input_yaml_string() {
        let args = InputArgs {
            input_yaml: Some("number: 42\ntext: hello".to_string()),
            ..Default::default()
        };
        let result = args.parse_input(false).unwrap();
        let json_value = serde_json::to_value(result).unwrap();
        assert_eq!(json_value["number"], 42);
        assert_eq!(json_value["text"], "hello");
    }

    #[test]
    fn test_parse_input_invalid_json() {
        let args = InputArgs {
            input_json: Some("invalid json".to_string()),
            ..Default::default()
        };
        let result = args.parse_input(false);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_input_invalid_yaml() {
        let args = InputArgs {
            input_yaml: Some("invalid: yaml: content: :::".to_string()),
            ..Default::default()
        };
        let result = args.parse_input(false);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_input_no_input_no_stdin() {
        let args = InputArgs::default();
        let result = args.parse_input(false);
        assert!(result.is_err());
    }

    #[test]
    fn test_for_repl() {
        let args = InputArgs::for_repl(
            Some("test.json".to_string()),
            Some(r#"{"test": true}"#.to_string()),
            None,
        );
        assert_eq!(args.input, Some(PathBuf::from("test.json")));
        assert_eq!(args.input_json, Some(r#"{"test": true}"#.to_string()));
        assert!(args.input_yaml.is_none());
    }

    #[test]
    fn test_to_json_value() {
        let args = InputArgs {
            input_json: Some(r#"{"test": 123}"#.to_string()),
            ..Default::default()
        };
        let json_value = args.to_json_value().unwrap();
        assert_eq!(json_value["test"], 123);
    }
}
