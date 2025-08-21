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
fn test_submit_help() {
    assert_cmd_snapshot!(stepflow().arg("submit").arg("--help"));
}

#[test]
fn test_submit_connection_error() {
    // Test that submit fails gracefully when server is not running
    assert_cmd_snapshot!(
        stepflow()
            .arg("submit")
            .arg("--url=http://localhost:9999")
            .arg("--flow=tests/mock/basic.yaml")
            .arg(r#"--input-json={"m": 1, "n": 2}"#)
    );
}

#[test]
fn test_submit_with_file_input() {
    // Test submit command parsing with file input
    assert_cmd_snapshot!(
        stepflow()
            .arg("submit")
            .arg("--url=http://localhost:9999")
            .arg("--flow=tests/mock/basic.yaml")
            .arg("--input=examples/eval/input.json")
    );
}

#[test]
fn test_submit_nonexistent_workflow() {
    assert_cmd_snapshot!(
        stepflow()
            .arg("submit")
            .arg("--url=http://localhost:9999")
            .arg("--flow=nonexistent.yaml")
            .arg(r#"--input-json={}"#)
    );
}
