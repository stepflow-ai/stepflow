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

//! Integration tests for workflow override functionality

use std::path::PathBuf;

use serde_json::json;
use stepflow_cli::args::OverrideArgs;

#[tokio::test]
async fn test_override_args_parsing() {
    // Test empty overrides
    let args = OverrideArgs::default();
    let overrides = args.parse_overrides().unwrap();
    assert!(overrides.is_empty());

    // Test JSON overrides parsing
    let args = OverrideArgs {
        overrides_json: Some(
            r#"{"step1": {"value": {"input": {"temperature": 0.8}}}}"#.to_string(),
        ),
        ..Default::default()
    };
    let overrides = args.parse_overrides().unwrap();
    assert!(!overrides.is_empty());
    assert!(overrides.steps.contains_key("step1"));

    // Test YAML overrides parsing
    let args = OverrideArgs {
        overrides_yaml: Some("step1:\n  value:\n    input:\n      temperature: 0.8".to_string()),
        ..Default::default()
    };
    let overrides = args.parse_overrides().unwrap();
    assert!(!overrides.is_empty());
    assert!(overrides.steps.contains_key("step1"));
}

#[tokio::test]
async fn test_override_args_validation() {
    // Test invalid JSON
    let args = OverrideArgs {
        overrides_json: Some("invalid json".to_string()),
        ..Default::default()
    };
    let result = args.parse_overrides();
    assert!(result.is_err());

    // Test invalid YAML
    let args = OverrideArgs {
        overrides_yaml: Some("invalid: yaml: content: :::".to_string()),
        ..Default::default()
    };
    let result = args.parse_overrides();
    assert!(result.is_err());
}

#[tokio::test]
async fn test_override_args_file_loading() {
    use std::fs;
    use tempfile::tempdir;

    // Create temporary directory and files
    let temp_dir = tempdir().unwrap();

    // Test JSON file
    let json_file = temp_dir.path().join("overrides.json");
    fs::write(
        &json_file,
        r#"{"step1": {"value": {"input": {"temperature": 0.8}}}}"#,
    )
    .unwrap();

    let args = OverrideArgs {
        overrides_file: Some(json_file),
        ..Default::default()
    };
    let overrides = args.parse_overrides().unwrap();
    assert!(!overrides.is_empty());
    assert!(overrides.steps.contains_key("step1"));

    // Test YAML file
    let yaml_file = temp_dir.path().join("overrides.yaml");
    fs::write(
        &yaml_file,
        "step1:\n  value:\n    input:\n      temperature: 0.8",
    )
    .unwrap();

    let args = OverrideArgs {
        overrides_file: Some(yaml_file),
        ..Default::default()
    };
    let overrides = args.parse_overrides().unwrap();
    assert!(!overrides.is_empty());
    assert!(overrides.steps.contains_key("step1"));
}

#[test]
fn test_override_args_conflicts() {
    // These tests ensure clap would reject conflicting arguments
    // In a real CLI invocation, clap would prevent these combinations

    let args_with_conflicts = OverrideArgs {
        overrides_file: Some(PathBuf::from("file.yaml")),
        overrides_json: Some("{}".to_string()),
        overrides_yaml: None,
    };

    // We can still test the logic manually - it should prefer one source
    // In practice, clap's conflicts_with_all prevents this
    assert!(args_with_conflicts.has_overrides());
}

#[tokio::test]
async fn test_complex_override_structures() {
    // Test complex nested overrides
    let complex_json = json!({
        "step1": {
            "$type": "merge_patch",
            "value": {
                "input": {
                    "temperature": 0.8,
                    "max_tokens": 1000,
                    "system_prompt": "You are a helpful assistant."
                },
                "component": "/builtin/openai",
                "metadata": {
                    "updated_by": "test",
                    "version": "2.0"
                }
            }
        },
        "step2": {
            "value": {
                "input": {
                    "model": "gpt-4"
                }
            }
        }
    });

    let args = OverrideArgs {
        overrides_json: Some(complex_json.to_string()),
        ..Default::default()
    };
    let overrides = args.parse_overrides().unwrap();

    assert!(!overrides.is_empty());
    assert_eq!(overrides.steps.len(), 2);
    assert!(overrides.steps.contains_key("step1"));
    assert!(overrides.steps.contains_key("step2"));

    // Check that step1 has explicit type
    let step1 = &overrides.steps["step1"];
    assert!(matches!(
        step1.override_type,
        stepflow_core::workflow::OverrideType::MergePatch
    ));

    // Check that step2 uses default type
    let step2 = &overrides.steps["step2"];
    assert!(matches!(
        step2.override_type,
        stepflow_core::workflow::OverrideType::MergePatch
    ));
}

#[tokio::test]
async fn test_override_serialization_roundtrip() {
    use stepflow_core::workflow::{OverrideType, StepOverride, WorkflowOverrides};

    // Create overrides programmatically
    let mut overrides = WorkflowOverrides::new();
    overrides.add_step_override(
        "step1".to_string(),
        StepOverride::with_type(
            OverrideType::MergePatch,
            json!({"input": {"temperature": 0.7}}),
        ),
    );

    // Serialize to JSON
    let json_str = serde_json::to_string(&overrides).unwrap();

    // Parse back through OverrideArgs
    let args = OverrideArgs {
        overrides_json: Some(json_str),
        ..Default::default()
    };
    let parsed_overrides = args.parse_overrides().unwrap();

    // Verify roundtrip
    assert_eq!(overrides.steps.len(), parsed_overrides.steps.len());
    assert!(parsed_overrides.steps.contains_key("step1"));
}
