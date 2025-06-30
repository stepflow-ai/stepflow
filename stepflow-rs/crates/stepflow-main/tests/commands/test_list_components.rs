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

use super::stepflow;
use insta_cmd::assert_cmd_snapshot;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_list_components_help() {
    assert_cmd_snapshot!(
        stepflow()
            .arg("--log-level=error")
            .arg("list-components")
            .arg("--help")
    );
}

#[test]
fn test_list_components_default() {
    // Use the builtins config from tests/
    assert_cmd_snapshot!(
        stepflow()
            .arg("--log-level=error")
            .arg("list-components")
            .arg("--config=../../tests/stepflow-config.yml")
            .current_dir(env!("CARGO_MANIFEST_DIR"))
    );
}

#[test]
fn test_list_components_format_json() {
    assert_cmd_snapshot!(
        stepflow()
            .arg("--log-level=error")
            .arg("list-components")
            .arg("--config=../../tests/stepflow-config.yml")
            .arg("--format=json")
            .current_dir(env!("CARGO_MANIFEST_DIR"))
    );
}

#[test]
fn test_list_components_format_yaml() {
    assert_cmd_snapshot!(
        stepflow()
            .arg("--log-level=error")
            .arg("list-components")
            .arg("--config=../../tests/stepflow-config.yml")
            .arg("--format=yaml")
            .current_dir(env!("CARGO_MANIFEST_DIR"))
    );
}

#[test]
fn test_list_components_with_schemas() {
    assert_cmd_snapshot!(
        stepflow()
            .arg("--log-level=error")
            .arg("list-components")
            .arg("--config=../../tests/stepflow-config.yml")
            .arg("--schemas=true")
            .current_dir(env!("CARGO_MANIFEST_DIR"))
    );
}

#[test]
fn test_list_components_json_no_schemas() {
    assert_cmd_snapshot!(
        stepflow()
            .arg("--log-level=error")
            .arg("list-components")
            .arg("--config=../../tests/stepflow-config.yml")
            .arg("--format=json")
            .arg("--schemas=false")
            .current_dir(env!("CARGO_MANIFEST_DIR"))
    );
}

#[test]
fn test_list_components_custom_config() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let config_path = temp_dir.path().join("custom-config.yml");

    // Create a custom config with builtin components
    let config_content = r#"
working_directory: .
plugins:
  - name: test-builtins
    type: builtin
"#;

    fs::write(&config_path, config_content).expect("Failed to write config file");

    assert_cmd_snapshot!(
        stepflow()
            .arg("--log-level=error")
            .arg("list-components")
            .arg(format!("--config={}", config_path.display()))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
    );
}

#[test]
fn test_list_components_nonexistent_config() {
    // Use a fixed path that doesn't exist for consistent snapshots
    assert_cmd_snapshot!(
        stepflow()
            .arg("--log-level=error")
            .arg("list-components")
            .arg("--config=/tmp/nonexistent-stepflow-config.yml")
            .current_dir(env!("CARGO_MANIFEST_DIR"))
    );
}
