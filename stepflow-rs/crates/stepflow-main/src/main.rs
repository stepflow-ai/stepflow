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

use clap::Parser as _;
use stepflow_main::{Cli, Result, args::init_tracing};

async fn run(cli: Cli) -> Result<()> {
    // Initialize tracing with the specified configuration
    init_tracing(
        &cli.log_level,
        &cli.other_log_level,
        cli.log_file.as_deref(),
    )?;

    cli.execute().await
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let omit_stack_trace = cli.omit_stack_trace;

    if let Err(e) = run(cli).await {
        #[allow(clippy::print_stderr)]
        if !omit_stack_trace {
            eprintln!("{e:?}");
        } else {
            eprintln!("{e}");
        }
        std::process::exit(1);
    }
}
