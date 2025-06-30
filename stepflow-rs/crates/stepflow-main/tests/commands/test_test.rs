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
fn test_test_help() {
    assert_cmd_snapshot!(stepflow().arg("test").arg("--help"));
}

#[test]
fn test_test_basic_workflow() {
    assert_cmd_snapshot!(stepflow().arg("test").arg("tests/basic.yaml"));
}

#[test]
fn test_test_mock_directory() {
    assert_cmd_snapshot!(stepflow().arg("test").arg("tests/mock"));
}

#[test]
fn test_test_with_custom_config() {
    assert_cmd_snapshot!(
        stepflow()
            .arg("test")
            .arg("--config=tests/builtins/stepflow-config.yml")
            .arg("tests/builtins")
    );
}

#[test]
fn test_test_nonexistent_path() {
    assert_cmd_snapshot!(stepflow().arg("test").arg("nonexistent_path"));
}
