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

use std::{path::PathBuf, sync::Arc};
use stepflow_core::workflow::Flow;
use url::Url;

use crate::{
    args::{ConfigArgs, InputArgs, LogLevel, OutputArgs, WorkflowLoader, load},
    error::Result,
    list_components::OutputFormat,
    repl::run_repl,
    run::run,
    serve::serve,
    submit::submit,
    test::TestOptions,
    validate,
};

/// StepFlow command line application.
///
/// Allows running a flow directly (with run)
#[derive(clap::Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Set the log level for StepFlow.
    #[arg(
        long = "log-level",
        value_name = "LEVEL",
        default_value = "info",
        global = true
    )]
    pub log_level: LogLevel,

    /// Set the log level for other parts of StepFlow.
    #[arg(
        long = "other-log-level",
        value_name = "LEVEL",
        default_value = "warn",
        global = true
    )]
    pub other_log_level: LogLevel,

    /// Write logs to a file instead of stderr.
    #[arg(long = "log-file", value_name = "FILE", value_hint = clap::ValueHint::FilePath, global = true)]
    pub log_file: Option<PathBuf>,

    /// Omit stack traces (line numbers of errors).
    #[arg(long = "omit-stack-trace", global = true)]
    pub omit_stack_trace: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Run a workflow directly.
    Run {
        /// Path to the workflow file to execute.
        #[arg(long="flow", value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
        flow_path: PathBuf,

        #[command(flatten)]
        config_args: ConfigArgs,

        #[command(flatten)]
        input_args: InputArgs,

        #[command(flatten)]
        output_args: OutputArgs,
    },
    /// Start a StepFlow service.
    Serve {
        /// Port to run the service on.
        #[arg(long, value_name = "PORT", default_value = "7837")]
        port: u16,

        #[command(flatten)]
        config_args: ConfigArgs,
    },
    /// Submit a workflow to a StepFlow service.
    Submit {
        /// The URL of the StepFlow service to submit the workflow to.
        #[arg(long, value_name = "URL", default_value = "http://localhost:7837", value_hint = clap::ValueHint::Url)]
        url: Url,

        /// Path to the workflow file to submit.
        #[arg(long="flow", value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
        flow_path: PathBuf,

        #[command(flatten)]
        input_args: InputArgs,

        #[command(flatten)]
        output_args: OutputArgs,
    },
    /// Run tests defined in workflow files or directories.
    Test {
        /// Paths to workflow files or directories containing tests.
        #[arg(value_name = "PATH", value_hint = clap::ValueHint::AnyPath)]
        paths: Vec<PathBuf>,

        #[command(flatten)]
        config_args: ConfigArgs,

        #[command(flatten)]
        test_options: TestOptions,
    },
    /// List all available components from a stepflow config.
    ListComponents {
        #[command(flatten)]
        config_args: ConfigArgs,

        /// Output format for the component list.
        #[arg(long = "format", value_name = "FORMAT", default_value = "pretty")]
        format: OutputFormat,

        /// Include component schemas in output.
        ///
        /// Defaults to false for pretty format, true for json/yaml formats.
        #[arg(long = "schemas")]
        schemas: Option<bool>,

        /// Hide components that are not reachable through any routing rule.
        ///
        /// Use --no-hide-unreachable to show all components regardless of routing.
        #[arg(long = "hide-unreachable", default_value = "true", action = clap::ArgAction::Set)]
        hide_unreachable: bool,
    },
    /// Start an interactive REPL for workflow development and debugging.
    Repl {
        #[command(flatten)]
        config_args: ConfigArgs,
    },
    /// Validate workflow files and configuration.
    Validate {
        /// Path to the workflow file to validate.
        #[arg(
            long = "flow",
            value_name = "FILE",
            value_hint = clap::ValueHint::FilePath
        )]
        flow_path: Option<PathBuf>,

        #[command(flatten)]
        config_args: ConfigArgs,

        /// Only validate the configuration file, not the workflow.
        #[arg(long)]
        config_only: bool,

        /// Only validate the workflow file, not the configuration.
        #[arg(long)]
        flow_only: bool,
    },
}

impl Cli {
    pub async fn execute(self) -> Result<()> {
        tracing::debug!(
            "Executing CLI command: {}",
            match &self.command {
                Command::Run { .. } => "run",
                Command::Test { .. } => "test",
                Command::Serve { .. } => "serve",
                Command::Submit { .. } => "submit",
                Command::ListComponents { .. } => "list-components",
                Command::Repl { .. } => "repl",
                Command::Validate { .. } => "validate",
            }
        );
        match self.command {
            Command::Run {
                flow_path,
                config_args,
                input_args,
                output_args,
            } => {
                let flow: Arc<Flow> = load(&flow_path)?;
                let flow_dir = flow_path.parent();
                let config = config_args.load_config(flow_dir)?;
                let executor = WorkflowLoader::create_executor_from_config(config).await?;

                let input = input_args.parse_input(true)?;

                let workflow_hash = Flow::hash(&flow);
                let output = run(executor, flow, workflow_hash, input).await?;
                output_args.write_output(output)?;
            }
            Command::Serve { port, config_args } => {
                let config = config_args.load_config(None)?;
                let executor = WorkflowLoader::create_executor_from_config(config).await?;

                serve(executor, port).await?;
            }
            Command::Submit {
                url,
                flow_path,
                input_args,
                output_args,
            } => {
                let flow: Flow = load(&flow_path)?;
                let input = input_args.parse_input(true)?;

                let output = submit(url, flow, input).await?;
                output_args.write_output(output)?;
            }
            Command::Test {
                paths,
                config_args,
                test_options,
            } => {
                let failures =
                    crate::test::run_tests(&paths, config_args.config_path, test_options).await?;
                if failures > 0 {
                    std::process::exit(1);
                }
            }
            Command::ListComponents {
                config_args,
                format,
                schemas,
                hide_unreachable,
            } => {
                crate::list_components::list_components(
                    config_args.config_path,
                    format,
                    schemas,
                    hide_unreachable,
                )
                .await?;
            }
            Command::Repl { config_args } => {
                run_repl(config_args.config_path).await?;
            }
            Command::Validate {
                flow_path,
                config_args,
                config_only,
                flow_only,
            } => {
                let failures = validate::validate(
                    flow_path.as_deref(),
                    config_args.config_path.as_deref(),
                    config_only,
                    flow_only,
                )
                .await?;
                if failures > 0 {
                    std::process::exit(1);
                }
            }
        };

        Ok(())
    }
}
