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

use super::stepflow;
use insta_cmd::assert_cmd_snapshot;

#[test]
fn test_validate_help() {
    assert_cmd_snapshot!(stepflow().arg("validate").arg("--help"));
}

#[test]
fn test_validate_empty_flow() {
    assert_cmd_snapshot!(
        stepflow()
            .arg("validate")
            .arg("--flow=tests/empty/empty_flow.yaml")
            .arg("--config=tests/empty/stepflow-config.yml")
    );
}

#[test]
fn test_validate_basic_workflow() {
    assert_cmd_snapshot!(stepflow().arg("validate").arg("--flow=tests/basic.yaml"));
}

#[test]
fn test_validate_nonexistent_workflow() {
    assert_cmd_snapshot!(stepflow().arg("validate").arg("--flow=nonexistent.yaml"));
}

#[test]
fn test_validate_with_type_check() {
    // Validate with type checking enabled
    assert_cmd_snapshot!(
        stepflow()
            .arg("validate")
            .arg("--flow=tests/basic.yaml")
            .arg("--type-check")
    );
}

#[test]
fn test_validate_builtins_with_type_check() {
    // Validate builtin components with type checking
    assert_cmd_snapshot!(
        stepflow()
            .arg("validate")
            .arg("--flow=tests/builtins/blob_test.yaml")
            .arg("--config=tests/builtins/stepflow-config.yml")
            .arg("--type-check")
    );
}
