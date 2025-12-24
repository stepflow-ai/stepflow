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

use error_stack::ResultExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use stepflow_core::{
    BlobId,
    workflow::{Flow, WorkflowOverrides},
};
use url::Url;

use crate::{
    args::{ConfigArgs, ExecutionArgs, InputArgs, OutputArgs, WorkflowLoader, load},
    error::Result,
    infer,
    list_components::OutputFormat,
    repl::run_repl,
    run::run,
    submit::submit,
    test::TestOptions,
    validate,
    validation_display::display_diagnostics,
    visualize,
};

/// Stepflow command line application.
///
/// Allows running a flow directly (with run)
#[derive(clap::Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Observability configuration
    #[command(flatten)]
    pub observability: stepflow_observability::ObservabilityConfig,

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
    /// # Run batch with multiple inputs from JSONL file
    ///
    /// stepflow run --flow=workflow.yaml --inputs=inputs.jsonl --output=results.jsonl
    ///
    /// # Run batch with limited concurrency
    ///
    /// stepflow run --flow=workflow.yaml --inputs=inputs.jsonl --max-concurrent=5
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

        /// Path to JSONL file containing multiple inputs (one JSON object per line).
        ///
        /// When specified, the workflow is executed once per line in the file.
        /// Results are written in JSONL format (one result per line).
        #[arg(long="inputs", value_name = "FILE", value_hint = clap::ValueHint::FilePath,
              conflicts_with_all = ["input", "input_json", "input_yaml", "stdin_format"])]
        inputs_path: Option<PathBuf>,

        /// Maximum number of concurrent executions (only used with --inputs).
        ///
        /// Defaults to number of inputs if not specified.
        #[arg(long = "max-concurrent", value_name = "N")]
        max_concurrent: Option<usize>,

        #[command(flatten)]
        execution_args: ExecutionArgs,

        #[command(flatten)]
        output_args: OutputArgs,
    },
    /// Submit a workflow to a Stepflow server.
    ///
    /// Submit a workflow to a running Stepflow server.
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
    /// stepflow submit --url=http://production-server:7840 --flow=workflow.yaml --input-json='{"key": "value"}'
    ///
    /// # Submit with inline YAML input
    ///
    /// stepflow submit --flow=workflow.yaml --input-yaml='param: value'
    ///
    /// # Submit batch with multiple inputs from JSONL file
    ///
    /// stepflow submit --flow=workflow.yaml --inputs=inputs.jsonl --output=results.jsonl
    ///
    /// ```
    Submit {
        /// The URL of the Stepflow service to submit the workflow to.
        #[arg(long, value_name = "URL", default_value = "http://localhost:7837", value_hint = clap::ValueHint::Url)]
        url: Url,

        /// Path to the workflow file to submit.
        #[arg(long="flow", value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
        flow_path: PathBuf,

        #[command(flatten)]
        input_args: InputArgs,

        /// Path to JSONL file containing multiple inputs (one JSON object per line).
        ///
        /// When specified, the workflow is executed once per line in the file.
        /// Results are written in JSONL format (one result per line).
        #[arg(long="inputs", value_name = "FILE", value_hint = clap::ValueHint::FilePath,
              conflicts_with_all = ["input", "input_json", "input_yaml", "stdin_format"])]
        inputs_path: Option<PathBuf>,

        /// Maximum number of concurrent executions on the server (only used with --inputs).
        ///
        /// Defaults to number of inputs if not specified.
        #[arg(long = "max-concurrent", value_name = "N")]
        max_concurrent: Option<usize>,

        #[command(flatten)]
        execution_args: ExecutionArgs,

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
    ///
    /// stepflow validate --flow=examples/basic/workflow.yaml
    ///
    /// # Validate with specific config
    ///
    /// stepflow validate --flow=workflow.yaml --config=my-config.yml
    ///
    /// # Validate with type checking
    ///
    /// stepflow validate --flow=workflow.yaml --type-check
    ///
    /// # Validate with strict type checking
    ///
    /// stepflow validate --flow=workflow.yaml --type-check --strict
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

        /// Enable type checking in addition to validation.
        ///
        /// Performs static type analysis to catch type mismatches before execution.
        #[arg(long)]
        type_check: bool,

        /// Treat untyped component outputs as errors instead of warnings.
        ///
        /// Only applicable when --type-check is enabled.
        #[arg(long)]
        strict: bool,

        #[command(flatten)]
        config_args: ConfigArgs,
    },
    /// Infer types for a workflow and output an annotated flow.
    ///
    /// Perform static type checking on a workflow and optionally output an annotated
    /// version with inferred output schemas. This helps catch type mismatches before
    /// execution and documents the expected data flow through the workflow.
    ///
    /// # Examples
    ///
    /// ```bash
    /// # Output annotated flow to stdout
    /// stepflow infer --flow=workflow.yaml
    ///
    /// # Output to auto-named file (workflow.inferred.yaml)
    /// stepflow infer --flow=workflow.yaml --output
    ///
    /// # Output to specific file
    /// stepflow infer --flow=workflow.yaml --output=annotated.yaml
    ///
    /// # Strict mode (untyped outputs are errors)
    /// stepflow infer --flow=workflow.yaml --strict
    /// ```
    Infer {
        /// Path to the workflow file to type check.
        #[arg(
            long = "flow",
            value_name = "FILE",
            value_hint = clap::ValueHint::FilePath
        )]
        flow_path: PathBuf,

        /// Output path for the annotated workflow.
        ///
        /// If specified without a value, writes to `<flow>.inferred.yaml`.
        /// If specified with a value, writes to that path.
        /// If not specified, outputs to stdout.
        #[arg(
            long = "output",
            short = 'o',
            value_name = "FILE",
            num_args = 0..=1,
            default_missing_value = "",
            value_hint = clap::ValueHint::FilePath
        )]
        output: Option<String>,

        /// Show inferred input types for each step.
        ///
        /// This prints the expected input type for each step based on component schemas.
        /// Useful for understanding what data each step receives.
        #[arg(long = "show-step-inputs")]
        show_step_inputs: bool,

        /// Treat untyped component outputs as errors instead of warnings.
        #[arg(long = "strict")]
        strict: bool,

        #[command(flatten)]
        config_args: ConfigArgs,
    },
    /// Visualize workflow structure as a graph.
    ///
    /// Generate a visual representation of workflow structure showing steps, dependencies,
    /// and component routing. Supports multiple output formats (DOT, SVG, PNG) with
    /// optional features like component server coloring and detailed tooltips.
    ///
    /// For SVG and PNG formats, output defaults to a file next to the workflow with
    /// matching extension (e.g., workflow.yaml → workflow.svg). Use --output to override.
    /// DOT format outputs to stdout by default.
    ///
    /// # Examples
    ///
    /// ```bash
    /// # Generate SVG visualization (writes to workflow.svg)
    /// stepflow visualize workflow.yaml
    ///
    /// # Generate PNG (writes to workflow.png)
    /// stepflow visualize workflow.yaml --format=png
    ///
    /// # Specify custom output path
    /// stepflow visualize workflow.yaml --output=diagrams/flow.svg
    ///
    /// # Output DOT to stdout for piping
    /// stepflow visualize workflow.yaml --format=dot
    ///
    /// # Minimal visualization without server details
    /// stepflow visualize workflow.yaml --no-servers
    /// ```
    Visualize {
        /// Path to the workflow file to visualize.
        #[arg(
            value_name = "FLOW",
            value_hint = clap::ValueHint::FilePath
        )]
        flow_path: PathBuf,

        /// Path to write the visualization output.
        ///
        /// For SVG/PNG formats, defaults to a file next to the workflow (e.g., workflow.svg).
        /// For DOT format, defaults to stdout.
        #[arg(
            long = "output",
            short = 'o',
            value_name = "FILE",
            value_hint = clap::ValueHint::FilePath
        )]
        output_path: Option<PathBuf>,

        /// Output format for the visualization.
        #[arg(long = "format", value_name = "FORMAT", default_value = "svg")]
        format: visualize::OutputFormat,

        /// Hide component server information from nodes.
        #[arg(long = "no-servers")]
        no_servers: bool,

        /// Hide detailed tooltips and metadata.
        #[arg(long = "no-details")]
        no_details: bool,

        #[command(flatten)]
        config_args: ConfigArgs,
    },
}

impl Cli {
    #[allow(clippy::print_stderr)]
    pub async fn execute(self) -> Result<()> {
        log::debug!(
            "Executing CLI command: {}",
            match &self.command {
                Command::Run { .. } => "run",
                Command::Test { .. } => "test",
                Command::Submit { .. } => "submit",
                Command::ListComponents { .. } => "list-components",
                Command::Repl { .. } => "repl",
                Command::Validate { .. } => "validate",
                Command::Infer { .. } => "infer",
                Command::Visualize { .. } => "visualize",
            }
        );
        match self.command {
            #[allow(clippy::print_stdout)]
            Command::Run {
                flow_path,
                config_args,
                input_args,
                inputs_path,
                max_concurrent,
                execution_args,
                output_args,
            } => {
                use stepflow_plugin::Context as _;

                let flow: Flow = load(&flow_path)?;
                let flow_dir = flow_path.parent();
                let config = config_args.load_config(flow_dir)?;

                // Parse overrides without applying them (late binding)
                let overrides = execution_args.override_args.parse_overrides()?;

                let flow = Arc::new(flow);

                // Validate workflow and configuration before execution
                let diagnostics = stepflow_analysis::validate_with_config(
                    &flow,
                    &config.plugins,
                    &config.routing,
                )
                .change_context(crate::MainError::ValidationError(
                    "Validation failed".to_string(),
                ))?;

                let failure_count = display_diagnostics(&diagnostics);
                if failure_count > 0 {
                    std::process::exit(1);
                }

                let executor = WorkflowLoader::create_executor_from_config(config).await?;
                let flow_id =
                    BlobId::from_flow(&flow).change_context(crate::MainError::Configuration)?;

                // Check if we're in batch mode
                if let Some(inputs_path) = inputs_path {
                    // Batch mode: read inputs from JSONL file
                    let inputs_content = std::fs::read_to_string(&inputs_path)
                        .change_context(crate::MainError::Configuration)
                        .attach_printable_lazy(|| {
                            format!("Failed to read inputs file: {:?}", inputs_path)
                        })?;

                    let inputs: Vec<stepflow_core::workflow::ValueRef> = inputs_content
                        .lines()
                        .enumerate()
                        .map(|(idx, line)| {
                            serde_json::from_str(line)
                                .change_context(crate::MainError::Configuration)
                                .attach_printable_lazy(|| {
                                    format!("Failed to parse input at line {}", idx + 1)
                                })
                        })
                        .collect::<Result<Vec<_>>>()?;

                    let total_inputs = inputs.len();
                    eprintln!("Running batch with {} inputs", total_inputs);

                    // Submit batch execution using unified runs API
                    let mut params =
                        stepflow_core::SubmitRunParams::with_inputs(flow.clone(), flow_id, inputs);
                    params = params.with_wait(true);
                    if let Some(max_concurrent) = max_concurrent {
                        params = params.with_max_concurrency(max_concurrent);
                    }
                    if let Some(overrides) = overrides {
                        params = params.with_overrides(overrides);
                    }
                    let run_status = executor
                        .submit_run(params)
                        .await
                        .change_context(crate::MainError::Configuration)?;

                    eprintln!("run_id: {}", run_status.run_id.simple());

                    // Extract results from run status
                    let results: Vec<Option<stepflow_core::FlowResult>> = run_status
                        .results
                        .map(|items| items.into_iter().map(|item| item.result).collect())
                        .unwrap_or_default();

                    // Write results using output_args or stdout in JSONL format
                    if let Some(output_path) = &output_args.output_path {
                        let mut output_file = std::fs::File::create(output_path)
                            .change_context(crate::MainError::Configuration)
                            .attach_printable_lazy(|| {
                                format!("Failed to create output file: {:?}", output_path)
                            })?;

                        use std::io::Write as _;
                        for result in results.iter().flatten() {
                            let json = serde_json::to_string(result)
                                .change_context(crate::MainError::Configuration)?;
                            writeln!(output_file, "{}", json)
                                .change_context(crate::MainError::Configuration)?;
                        }
                        eprintln!(
                            "Batch execution completed. Results written to {:?}",
                            output_path
                        );
                    } else {
                        // Write to stdout in JSONL format
                        for result in results.iter().flatten() {
                            let json = serde_json::to_string(result)
                                .change_context(crate::MainError::Configuration)?;
                            println!("{}", json);
                        }
                        eprintln!("Batch execution completed with {} results", results.len());
                    }
                } else {
                    // Single input mode
                    let input = input_args.parse_input(true)?;
                    let (run_id, output) = run(executor, flow, flow_id, input, overrides).await?;
                    // Output run_id without hyphens for Jaeger trace ID compatibility
                    eprintln!("run_id: {}", run_id.simple());
                    output_args.write_output(output)?;
                }
            }
            #[allow(clippy::print_stdout)]
            Command::Submit {
                url,
                flow_path,
                input_args,
                inputs_path,
                max_concurrent,
                execution_args,
                output_args,
            } => {
                let flow: Flow = load(&flow_path)?;

                // Parse overrides for submission
                let overrides = execution_args.override_args.parse_overrides()?;

                // Parse inputs (single or batch mode)
                let inputs: Vec<stepflow_core::workflow::ValueRef> =
                    if let Some(inputs_path) = inputs_path {
                        // Batch mode: read inputs from JSONL file
                        let inputs_content = std::fs::read_to_string(&inputs_path)
                            .change_context(crate::MainError::Configuration)
                            .attach_printable_lazy(|| {
                                format!("Failed to read inputs file: {:?}", inputs_path)
                            })?;

                        inputs_content
                            .lines()
                            .enumerate()
                            .map(|(idx, line)| {
                                serde_json::from_str(line)
                                    .change_context(crate::MainError::Configuration)
                                    .attach_printable_lazy(|| {
                                        format!("Failed to parse input at line {}", idx + 1)
                                    })
                            })
                            .collect::<Result<Vec<_>>>()?
                    } else {
                        // Single input mode
                        vec![input_args.parse_input(true)?]
                    };

                // Handle empty inputs case (batch mode with 0 items)
                if inputs.is_empty() {
                    eprintln!("No inputs provided, nothing to submit");
                    if let Some(output_path) = &output_args.output_path {
                        // Create empty output file
                        std::fs::File::create(output_path)
                            .change_context(crate::MainError::Configuration)?;
                    }
                    return Ok(());
                }

                let is_batch = inputs.len() > 1;
                if is_batch {
                    eprintln!("Submitting batch with {} inputs", inputs.len());
                }

                // Submit using the unified API (works for both single and batch)
                let results = submit(
                    url,
                    flow,
                    inputs,
                    overrides.as_ref().unwrap_or(&WorkflowOverrides::new()),
                    max_concurrent,
                )
                .await?;

                // Write results
                if is_batch {
                    // Batch mode: write results in JSONL format
                    if let Some(output_path) = &output_args.output_path {
                        let mut output_file = std::fs::File::create(output_path)
                            .change_context(crate::MainError::Configuration)
                            .attach_printable_lazy(|| {
                                format!("Failed to create output file: {:?}", output_path)
                            })?;

                        use std::io::Write as _;
                        for result in &results {
                            let json = serde_json::to_string(result)
                                .change_context(crate::MainError::Configuration)?;
                            writeln!(output_file, "{}", json)
                                .change_context(crate::MainError::Configuration)?;
                        }
                        eprintln!(
                            "Batch submission completed. Results written to {:?}",
                            output_path
                        );
                    } else {
                        // Write to stdout in JSONL format
                        for result in &results {
                            let json = serde_json::to_string(result)
                                .change_context(crate::MainError::Configuration)?;
                            println!("{}", json);
                        }
                        eprintln!("Batch submission completed with {} results", results.len());
                    }
                } else {
                    // Single mode: write single result
                    let output = results.into_iter().next().expect("single result expected");
                    output_args.write_output(output.clone())?;

                    // Exit with non-zero status if workflow execution failed
                    if output.failed().is_some() {
                        std::process::exit(1);
                    }
                }
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
                type_check,
                strict,
                config_args,
            } => {
                let failures = validate::validate(
                    &flow_path,
                    config_args.config_path.as_deref(),
                    type_check,
                    strict,
                )
                .await?;
                if failures > 0 {
                    std::process::exit(1);
                }
            }
            Command::Infer {
                flow_path,
                output,
                show_step_inputs,
                strict,
                config_args,
            } => {
                // Determine output path
                let output_path = match output {
                    Some(path) if path.is_empty() => {
                        // --output without value: auto-generate filename
                        let stem = flow_path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("workflow");
                        let parent = flow_path.parent().unwrap_or(Path::new("."));
                        Some(parent.join(format!("{}.inferred.yaml", stem)))
                    }
                    Some(path) => Some(PathBuf::from(path)),
                    None => None,
                };

                let failures = infer::infer(
                    &flow_path,
                    config_args.config_path.as_deref(),
                    output_path.as_deref(),
                    strict,
                    show_step_inputs,
                )
                .await?;
                if failures > 0 {
                    std::process::exit(1);
                }
            }
            Command::Visualize {
                flow_path,
                output_path,
                format,
                no_servers,
                no_details,
                config_args,
            } => {
                use crate::MainError;

                // Capture current working directory FIRST, before any operations that might change it
                let cwd = std::env::current_dir().change_context(MainError::internal(
                    "Failed to get current working directory",
                ))?;

                // Make flow_path absolute so derived paths are also absolute
                let flow_path = if flow_path.is_absolute() {
                    flow_path
                } else {
                    cwd.join(&flow_path)
                };

                let flow: Arc<Flow> = load(&flow_path)?;
                let flow_dir = flow_path.parent();

                // Try to load config for server information (simplified approach for now)
                let router = match config_args.load_config(flow_dir) {
                    Ok(_config) => {
                        // For now, we'll pass None but could build a router here in the future
                        // This would require implementing PluginRouter construction from StepflowConfig
                        None
                    }
                    Err(_) => {
                        log::warn!(
                            "Could not load configuration, visualization will not show component server routing"
                        );
                        None
                    }
                };

                let vis_config = visualize::VisualizationConfig {
                    format,
                    show_component_servers: !no_servers,
                    show_details: !no_details,
                };

                // Determine output path: explicit, inferred, or stdout
                let resolved_output_path: Option<PathBuf> = match output_path {
                    Some(path) => {
                        // Resolve relative paths against captured current working directory
                        Some(if path.is_absolute() {
                            path
                        } else {
                            cwd.join(&path)
                        })
                    }
                    None if format != visualize::OutputFormat::Dot => {
                        // Infer output path from flow file (already absolute)
                        let inferred = flow_path.with_extension(format.extension());

                        // Check if file already exists
                        if inferred.exists() {
                            use std::io::{self, Write as _};
                            let display_path = inferred
                                .strip_prefix(&cwd)
                                .map(|p| p.to_path_buf())
                                .unwrap_or_else(|_| inferred.clone());
                            writeln!(
                                io::stderr(),
                                "Error: Output file already exists: {}\nUse --output to specify a different path.",
                                display_path.display()
                            )
                            .change_context(MainError::Configuration)?;
                            std::process::exit(1);
                        }

                        Some(inferred)
                    }
                    None => None, // DOT format to stdout
                };

                match resolved_output_path {
                    Some(output_path) => {
                        // Output to file
                        visualize::visualize_flow(flow, router, &output_path, vis_config).await?;

                        // Print success message with path relative to original CWD for cleaner output
                        use std::io::{self, Write as _};
                        let display_path = output_path
                            .strip_prefix(&cwd)
                            .map(|p| p.to_path_buf())
                            .unwrap_or(output_path);
                        writeln!(
                            io::stderr(),
                            "✅ Visualization generated: {}",
                            display_path.display()
                        )
                        .change_context(MainError::Configuration)?;
                    }
                    None => {
                        // Output to stdout (DOT format only)
                        visualize::visualize_flow_to_stdout(flow, router, vis_config).await?;
                    }
                }
            }
        };

        Ok(())
    }
}
