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

mod test_list_components;
mod test_repl;
mod test_run;
mod test_serve;
mod test_stepflow;
mod test_submit;
mod test_test;
mod test_visualize;

use insta_cmd::Command;
use std::path::Path;

fn stepflow() -> Command {
    let mut command = Command::new(insta_cmd::get_cargo_bin("stepflow"));

    // Locate the cargo workspace
    let path = Path::new(std::env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("Failed to locate workspace root");
    command.current_dir(path);
    command.arg("--log-file=/dev/null");
    command.arg("--omit-stack-trace");
    command
}
