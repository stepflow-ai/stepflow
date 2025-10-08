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

use insta_cmd::assert_cmd_snapshot;

use super::stepflow;

#[test]
fn test_repl_help() {
    // Test REPL help by passing it as subcommand help
    assert_cmd_snapshot!(stepflow().args(["repl", "--help"]));
}

// Note: Interactive stdin testing with insta_cmd is complex, so we test the core functionality
// through integration tests that can properly handle the interactive nature of the REPL.
// For now, we'll focus on testing the command structure and help output.
