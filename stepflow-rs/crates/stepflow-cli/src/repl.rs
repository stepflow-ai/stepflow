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

#![allow(clippy::print_stdout, clippy::print_stderr)]

use clap::Parser as _;
use error_stack::ResultExt as _;
use rustyline::{DefaultEditor, error::ReadlineError};
use std::{path::PathBuf, sync::Arc};
use stepflow_core::{
    BlobId,
    workflow::{Flow, ValueRef},
};
use stepflow_plugin::StepflowEnvironment;

use crate::{
    MainError, Result,
    args::InputArgs,
    args::{ConfigArgs, WorkflowLoader, load},
};

/// Information about the last workflow run
pub struct LastRun {
    pub flow: Arc<Flow>,
    pub flow_id: BlobId,
    pub input: ValueRef,
}

impl LastRun {
    pub fn new(flow: Arc<Flow>, flow_id: BlobId, input: ValueRef) -> Self {
        Self {
            flow,
            flow_id,
            input,
        }
    }

    /// Execute this workflow
    pub async fn execute(&self, env: &Arc<StepflowEnvironment>) -> Result<()> {
        let run_status = stepflow_execution::submit_run(
            env,
            self.flow.clone(),
            self.flow_id.clone(),
            vec![self.input.clone()],
            stepflow_core::SubmitRunParams::default(),
        )
        .await
        .change_context(MainError::FlowExecution)?;

        // Wait for completion and fetch results
        stepflow_execution::wait_for_completion(env, run_status.run_id)
            .await
            .change_context(MainError::FlowExecution)?;
        let run_status = stepflow_execution::get_run(
            env,
            run_status.run_id,
            stepflow_core::GetRunParams {
                include_results: true,
                ..Default::default()
            },
        )
        .await
        .change_context(MainError::FlowExecution)?;

        // Extract result from run status
        let result = run_status
            .results
            .and_then(|r| r.into_iter().next())
            .and_then(|item| item.result);

        // Display result
        if let Some(result) = result {
            let result_json =
                serde_json::to_string_pretty(&result).change_context(MainError::FlowExecution)?;
            println!("Result:\n{result_json}");
        } else {
            println!("No result available");
        }

        Ok(())
    }

    /// Update input
    pub fn update_input(&mut self, input: ValueRef) {
        self.input = input;
    }
}

/// State maintained by the REPL
pub struct ReplState {
    env: Arc<StepflowEnvironment>,
    last_run: Option<LastRun>,
    config_path: Option<PathBuf>,
}

impl ReplState {
    async fn new(config_path: Option<PathBuf>) -> Result<Self> {
        // Initialize executor with default config
        let config_args = ConfigArgs::with_path(config_path.clone());
        let config = config_args.load_config(None)?;
        let executor = WorkflowLoader::create_executor_from_config(config).await?;

        Ok(Self {
            env: executor,
            last_run: None,
            config_path,
        })
    }
}

/// Commands available in the REPL
#[derive(clap::Parser, Debug)]
#[command(no_binary_name = true)]
pub enum ReplCommand {
    /// Load and execute a workflow with input
    #[command(name = "run")]
    Run {
        /// Path to the workflow file to execute
        #[arg(long = "flow", value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
        flow: PathBuf,

        #[command(flatten)]
        input_args: InputArgs,
    },
    /// Re-execute the current workflow, optionally with new input.
    #[command(name = "rerun")]
    Rerun {
        #[command(flatten)]
        input_args: InputArgs,
    },
    /// Show current REPL state
    #[command(name = "status")]
    Status,
    /// Display the currently loaded workflow
    #[command(name = "workflow")]
    Workflow,
    /// Display the current input
    #[command(name = "input")]
    Input,
    /// Exit the REPL
    #[command(name = "quit")]
    Quit,
    /// Exit the REPL
    #[command(name = "exit")]
    Exit,
}

/// Parse a command line into a ReplCommand using clap
fn parse_command(input: &str) -> Result<ReplCommand> {
    let args: Vec<&str> = input.split_whitespace().collect();

    if args.is_empty() {
        return Err(MainError::ReplCommand("Empty command".to_string()).into());
    }

    match ReplCommand::try_parse_from(args) {
        Ok(command) => Ok(command),
        Err(err) => {
            // Convert clap errors to our error type
            Err(MainError::ReplCommand(err.to_string()).into())
        }
    }
}

/// Handle a parsed command
async fn handle_command(command: ReplCommand, state: &mut ReplState) -> Result<()> {
    match command {
        ReplCommand::Run { flow, input_args } => handle_run_command(flow, input_args, state).await,
        ReplCommand::Rerun { input_args } => handle_rerun_command(input_args, state).await,
        ReplCommand::Status => handle_status_command(state).await,
        ReplCommand::Workflow => handle_workflow_command(state).await,
        ReplCommand::Input => handle_input_command(state).await,
        ReplCommand::Quit | ReplCommand::Exit => {
            println!("Goodbye!");
            Ok(())
        }
    }
}

/// Handle the run command
async fn handle_run_command(
    flow_path: PathBuf,
    input_args: InputArgs,
    state: &mut ReplState,
) -> Result<()> {
    // Load flow
    let flow: Arc<Flow> = load(&flow_path)?;
    let flow_id = BlobId::from_flow(flow.as_ref()).change_context(MainError::Configuration)?;
    println!("Loaded flow: {}", flow_path.display());

    // Parse input with flow context
    let input_value = input_args.parse_input(true)?;

    // Create LastRun structure and execute
    let last_run = LastRun::new(flow, flow_id, input_value);
    last_run.execute(&state.env).await?;

    // Store the last run
    state.last_run = Some(last_run);
    Ok(())
}

/// Handle the rerun command
async fn handle_rerun_command(input_args: InputArgs, state: &mut ReplState) -> Result<()> {
    let last_run = state.last_run.as_mut().ok_or_else(|| {
        MainError::ReplCommand("No workflow loaded. Use 'run' first.".to_string())
    })?;

    // Use new input if provided, otherwise use stored input
    if input_args.has_input() {
        let input_value = input_args.parse_input(false)?;
        last_run.update_input(input_value);
    }

    // Execute workflow
    last_run.execute(&state.env).await
}

/// Handle the status command
async fn handle_status_command(state: &ReplState) -> Result<()> {
    println!("REPL Status:");
    println!("  Config: {:?}", state.config_path);

    let plugins: Vec<_> = state.env.plugins().collect();
    println!("  Executor: {} plugins loaded", plugins.len());

    if let Some(last_run) = &state.last_run {
        println!("  Workflow: {} steps", last_run.flow.steps().len());
        if let Some(name) = last_run.flow.name() {
            println!("    Name: {name}");
        }
        println!("  Input: Loaded");
    } else {
        println!("  Workflow: Not loaded");
        println!("  Input: Not loaded");
    }

    Ok(())
}

/// Handle the workflow command
async fn handle_workflow_command(state: &ReplState) -> Result<()> {
    if let Some(last_run) = &state.last_run {
        let workflow_json = serde_json::to_string_pretty(last_run.flow.as_ref())
            .change_context(MainError::FlowExecution)?;
        println!("Current workflow:\n{workflow_json}");
    } else {
        println!("No workflow loaded. Use 'run --workflow=<file>' to load a workflow.");
    }
    Ok(())
}

/// Handle the input command
async fn handle_input_command(state: &ReplState) -> Result<()> {
    if let Some(last_run) = &state.last_run {
        let input_json = serde_json::to_string_pretty(last_run.input.as_ref())
            .change_context(MainError::FlowExecution)?;
        println!("Current input:\n{input_json}");
    } else {
        println!("No input loaded. Use 'run' or 'rerun' with input to load input.");
    }
    Ok(())
}

/// Print help information using clap's automatic help generation
fn print_help() {
    use clap::CommandFactory as _;

    // Create a clap command for ReplCommand and print its help
    let mut cmd = ReplCommand::command();
    cmd.set_bin_name(""); // Remove binary name from help output

    // Print a custom header
    println!("Stepflow REPL Commands:");
    println!();

    // Use clap's help generation
    let help = cmd.render_help();
    println!("{help}");
}

/// Main REPL function
pub async fn run_repl(config_path: Option<PathBuf>) -> Result<()> {
    let mut rl = DefaultEditor::new().change_context(MainError::ReplInit)?;

    let mut state = ReplState::new(config_path).await?;

    println!("Stepflow REPL v{}", env!("CARGO_PKG_VERSION"));
    println!("Type 'help' for available commands, 'quit' to exit");

    loop {
        let readline = rl.readline("stepflow> ");
        match readline {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                let _ = rl.add_history_entry(line);

                match parse_command(line) {
                    Ok(ReplCommand::Quit) | Ok(ReplCommand::Exit) => {
                        println!("Goodbye!");
                        break;
                    }
                    Ok(command) => {
                        if let Err(e) = handle_command(command, &mut state).await {
                            eprintln!("Error: {e}");
                        }
                    }
                    Err(e) => {
                        let error_msg = e.to_string();
                        // Check if this is a help request (clap handles help automatically)
                        if error_msg.contains("help") || line.trim() == "help" {
                            print_help();
                        } else {
                            eprintln!("Error: {error_msg}");
                        }
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("^D");
                break;
            }
            Err(err) => {
                eprintln!("Error reading line: {err:?}");
                break;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_parsing() {
        // Test basic commands that don't require files
        let commands = vec![("status", "Status"), ("quit", "Quit"), ("exit", "Exit")];

        for (input, expected_variant) in commands {
            let result = parse_command(input);
            match result {
                Ok(command) => {
                    let debug_str = format!("{command:?}");
                    assert!(
                        debug_str.starts_with(expected_variant),
                        "Expected {input} to parse as {expected_variant} variant, got: {command:?}"
                    );
                }
                Err(e) => panic!("Failed to parse '{input}': {e}"),
            }
        }
    }

    #[test]
    fn test_rerun_with_input() {
        // Test with simple JSON (without spaces that would cause parsing issues)
        let result = parse_command("rerun --input-json '{\"test\":123}'");
        match result {
            Ok(ReplCommand::Rerun { input_args }) => {
                assert_eq!(input_args.input_json, Some("'{\"test\":123}'".to_string()));
            }
            _ => panic!("Expected rerun command, got: {result:?}"),
        }
    }

    #[test]
    fn test_run_command_parsing() {
        let result = parse_command("run --flow test.yaml");
        match result {
            Ok(ReplCommand::Run {
                flow: workflow,
                input_args,
            }) => {
                assert_eq!(workflow.to_string_lossy(), "test.yaml");
                assert!(!input_args.has_input());
            }
            _ => panic!("Expected run command, got: {result:?}"),
        }
    }
}
