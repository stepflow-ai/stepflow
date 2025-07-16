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

#[test]
fn test_run_help() {
    assert_cmd_snapshot!(stepflow().arg("run").arg("--help"));
}

#[test]
fn test_run_basic_workflow_with_file_input() {
    assert_cmd_snapshot!(
        stepflow()
            .arg("run")
            .arg("--flow=tests/mock/basic.yaml")
            .arg("--input=tests/mock/input.json")
    );
}

#[test]
fn test_run_basic_workflow_with_json_input() {
    assert_cmd_snapshot!(
        stepflow()
            .arg("run")
            .arg("--flow=tests/mock/basic.yaml")
            .arg(r#"--input-json={"name": "hello"}"#)
    );
}

#[test]
fn test_run_basic_workflow_with_yaml_input() {
    assert_cmd_snapshot!(
        stepflow()
            .arg("run")
            .arg("--flow=tests/mock/basic.yaml")
            .arg("--input-yaml=name: hello")
    );
}

#[test]
fn test_run_with_custom_config() {
    assert_cmd_snapshot!(
        stepflow()
            .arg("run")
            .arg("--flow=tests/builtins/blob_test.yaml")
            .arg("--config=tests/builtins/stepflow-config.yml")
            .arg(r#"--input-json={"data": "hello"}"#)
    );
}

#[test]
fn test_run_nonexistent_workflow() {
    assert_cmd_snapshot!(
        stepflow()
            .arg("run")
            .arg("--flow=nonexistent.yaml")
            .arg(r#"--input-json={}"#)
    );
}

#[test]
fn test_run_invalid_input_json() {
    assert_cmd_snapshot!(
        stepflow()
            .arg("run")
            .arg("--flow=tests/mock/basic.yaml")
            .arg("--input-json=invalid json")
    );
}
