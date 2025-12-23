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
fn test_infer_help() {
    assert_cmd_snapshot!(stepflow().arg("infer").arg("--help"));
}

#[test]
fn test_infer_basic_workflow() {
    // Uses /builtin/create_messages which has known input/output schemas
    assert_cmd_snapshot!(stepflow().arg("infer").arg("--flow=tests/basic.yaml"));
}

#[test]
fn test_infer_blob_workflow() {
    // Uses /builtin/put_blob and /builtin/get_blob with known schemas
    assert_cmd_snapshot!(
        stepflow()
            .arg("infer")
            .arg("--flow=tests/builtins/blob_test.yaml")
            .arg("--config=tests/builtins/stepflow-config.yml")
    );
}

#[test]
fn test_infer_nonexistent_workflow() {
    assert_cmd_snapshot!(stepflow().arg("infer").arg("--flow=nonexistent.yaml"));
}

#[test]
fn test_infer_strict_mode() {
    // Test with --strict flag
    assert_cmd_snapshot!(
        stepflow()
            .arg("infer")
            .arg("--flow=tests/basic.yaml")
            .arg("--strict")
    );
}

#[test]
fn test_infer_secret_propagation() {
    // Test that is_secret annotation propagates from variables to step inputs.
    // The api_key variable is marked as is_secret: true, and when referenced
    // via {$variable: "api_key"}, the synthesized schema should preserve this.
    assert_cmd_snapshot!(
        stepflow()
            .arg("infer")
            .arg("--flow=stepflow-rs/crates/stepflow-cli/tests/commands/secret_propagation.yaml")
            .arg("--show-step-inputs")
    );
}
