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

use clap::Parser as _;
use error_stack::ResultExt as _;
use stepflow_cli::{Cli, Result};
use stepflow_observability::init_observability;

async fn run(cli: Cli) -> Result<()> {
    // Initialize observability with the specified configuration
    // CLI runs workflows, so enable run diagnostic
    let binary_config = stepflow_observability::BinaryObservabilityConfig {
        service_name: "stepflow-cli",
        include_run_diagnostic: true,
    };
    let _guard = init_observability(&cli.observability, binary_config)
        .change_context(stepflow_cli::MainError::TracingInit)?;

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
