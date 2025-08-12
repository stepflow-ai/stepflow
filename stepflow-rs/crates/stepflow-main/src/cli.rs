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

use error_stack::ResultExt as _;
use std::{path::PathBuf, sync::Arc};
use stepflow_core::{BlobId, workflow::Flow};
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
    ///
    /// Execute a workflow directly and return the result.
    /// 
    /// # Examples
    /// 
    /// ```bash
    ///
    /// # Run with input file
    ///
    /// stepflow run --flow=examples/basic/workflow.yaml --input=examples/basic/input1.json
    ///
    /// # Run with inline JSON input
    ///
    /// stepflow run --flow=workflow.yaml --input-json='{"m": 3, "n": 4}'
    ///
    /// # Run with inline YAML input
    ///
    /// stepflow run --flow=workflow.yaml --input-yaml='m: 2\nn: 7'
    ///
    /// # Run with stdin input
    ///
    /// echo '{"m": 1, "n": 2}' | stepflow run --flow=workflow.yaml --stdin-format=json
    ///
    /// # Run with custom config and output to file
    ///
    /// stepflow run --flow=workflow.yaml --input=input.json --config=my-config.yml --output=result.json
    ///
    /// ```
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
    ///
    /// Start a StepFlow service that can accept workflow submissions via HTTP API.
    /// 
    /// # Examples
    /// 
    /// ```bash
    ///
    /// # Start server on default port (7837)
    ///
    /// stepflow serve
    ///
    /// # Start server on custom port with config
    ///
    /// stepflow serve --port=8080 --config=production-config.yml
    ///
    /// ```
    Serve {
        /// Port to run the service on.
        #[arg(long, value_name = "PORT", default_value = "7837")]
        port: u16,

        #[command(flatten)]
        config_args: ConfigArgs,
    },
    /// Submit a workflow to a StepFlow service.
    ///
    /// Submit a workflow to a running StepFlow service.
    /// 
    /// # Examples
    /// 
    /// ```bash
    ///
    /// # Submit to local server
    ///
    /// stepflow submit --flow=workflow.yaml --input=input.json
    ///
    /// # Submit to remote server
    ///
    /// stepflow submit --url=http://production-server:7837 --flow=workflow.yaml --input-json='{"key": "value"}'
    ///
    /// # Submit with inline YAML input
    ///
    /// stepflow submit --flow=workflow.yaml --input-yaml='param: value'
    ///
    /// ```
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
    ///
    /// Run test cases defined in workflow files.
    /// 
    /// # Examples
    /// 
    /// ```bash
    ///
    /// # Run all tests in a workflow file
    ///
    /// stepflow test examples/basic/workflow.yaml
    ///
    /// # Run tests in a directory
    ///
    /// stepflow test examples/
    ///
    /// # Run specific test case
    ///
    /// stepflow test workflow.yaml --case=calculate_with_8_and_5
    ///
    /// # Run multiple specific test cases
    ///
    /// stepflow test workflow.yaml --case=test1 --case=test2
    ///
    /// # Update expected outputs (snapshot testing)
    ///
    /// stepflow test workflow.yaml --update
    ///
    /// # Show detailed diff on test failures
    ///
    /// stepflow test workflow.yaml --diff
    ///
    /// ```
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
    ///
    /// List all available components from the configured plugins.
    /// 
    /// # Examples
    /// 
    /// ```bash
    ///
    /// # List components in pretty format
    ///
    /// stepflow list-components
    ///
    /// # List components with JSON output including schemas
    ///
    /// stepflow list-components --format=json
    ///
    /// # List components from specific config without schemas
    ///
    /// stepflow list-components --config=my-config.yml --format=yaml --schemas=false
    ///
    /// # Show all components including unreachable ones
    ///
    /// stepflow list-components --hide-unreachable=false
    ///
    /// ```
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
    ///
    /// Start an interactive REPL (Read-Eval-Print Loop) for workflow development and debugging.
    /// The REPL provides an interactive environment for testing workflow fragments, debugging
    /// step execution, exploring component capabilities, and iterative workflow development.
    /// 
    /// # Examples
    /// 
    /// ```bash
    ///
    /// # Start REPL with default config
    ///
    /// stepflow repl
    ///
    /// # Start REPL with custom config
    ///
    /// stepflow repl --config=development-config.yml
    ///
    /// ```
    Repl {
        #[command(flatten)]
        config_args: ConfigArgs,
    },
    /// Validate workflow files and configuration.
    ///
    /// Validate workflow files and configuration without executing them. This performs workflow
    /// validation (structure, step dependencies, value references), configuration validation
    /// (plugin definitions, routing rules), component routing validation, and schema validation.
    /// Returns 0 for success, 1+ for validation failures (suitable for CI/CD pipelines).
    /// 
    /// # Examples
    /// 
    /// ```bash
    ///
    /// # Validate workflow with auto-detected config
    /// stepflow validate --flow=examples/basic/workflow.yaml
    ///
    /// # Validate with specific config
    /// stepflow validate --flow=workflow.yaml --config=my-config.yml
    ///
    /// ```
    Validate {
        /// Path to the workflow file to validate.
        #[arg(
            long = "flow",
            value_name = "FILE",
            value_hint = clap::ValueHint::FilePath
        )]
        flow_path: PathBuf,

        #[command(flatten)]
        config_args: ConfigArgs,
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

                let flow_id =
                    BlobId::from_flow(&flow).change_context(crate::MainError::Configuration)?;
                let output = run(executor, flow, flow_id, input).await?;
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
            } => {
                let failures =
                    validate::validate(&flow_path, config_args.config_path.as_deref()).await?;
                if failures > 0 {
                    std::process::exit(1);
                }
            }
        };

        Ok(())
    }
}
