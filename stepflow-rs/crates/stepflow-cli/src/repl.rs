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
use stepflow_execution::{StepflowExecutor, WorkflowExecutor};
use stepflow_plugin::Context as _;

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
    pub last_execution: Option<WorkflowExecutor>,
}

impl LastRun {
    pub fn new(flow: Arc<Flow>, flow_id: BlobId, input: ValueRef) -> Self {
        Self {
            flow,
            flow_id,
            input,
            last_execution: None,
        }
    }

    /// Execute this workflow normally (non-debug mode)
    pub async fn execute_normal(&self, executor: &StepflowExecutor) -> Result<()> {
        let params = stepflow_core::SubmitFlowParams::new(
            self.flow.clone(),
            self.flow_id.clone(),
            self.input.clone(),
        );
        let run_id = executor
            .submit_flow(params)
            .await
            .change_context(MainError::FlowExecution)?;

        let result = executor
            .flow_result(run_id)
            .await
            .change_context(MainError::FlowExecution)?;

        // Display result
        let result_json =
            serde_json::to_string_pretty(&result).change_context(MainError::FlowExecution)?;
        println!("Result:\n{result_json}");

        Ok(())
    }

    /// Create a debug execution for this workflow
    pub async fn create_debug_execution(
        &mut self,
        executor: &Arc<StepflowExecutor>,
    ) -> Result<&mut WorkflowExecutor> {
        let state_store = executor.state_store();
        let run_id = uuid::Uuid::now_v7();
        let workflow_executor = WorkflowExecutor::new(
            executor.clone(),
            self.flow.clone(),
            self.flow_id.clone(),
            run_id,
            self.input.clone(),
            state_store.clone(),
            None, // TODO: Add variables support to REPL
        )
        .change_context(MainError::FlowExecution)?;

        self.last_execution = Some(workflow_executor);
        Ok(self.last_execution.as_mut().unwrap())
    }

    /// Get the current debug execution, if any
    pub fn debug_execution(&mut self) -> Option<&mut WorkflowExecutor> {
        self.last_execution.as_mut()
    }

    /// Update input and clear any existing execution
    pub fn update_input(&mut self, input: ValueRef) {
        self.input = input;
        self.last_execution = None; // Clear execution since input changed
    }
}

/// State maintained by the REPL
pub struct ReplState {
    executor: Arc<StepflowExecutor>,
    last_run: Option<LastRun>,
    config_path: Option<PathBuf>,
    debug_mode: bool,
}

impl ReplState {
    async fn new(config_path: Option<PathBuf>) -> Result<Self> {
        // Initialize executor with default config
        let config_args = ConfigArgs::with_path(config_path.clone());
        let config = config_args.load_config(None)?;
        let executor = WorkflowLoader::create_executor_from_config(config).await?;

        Ok(Self {
            executor,
            last_run: None,
            config_path,
            debug_mode: false,
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

        /// Enable debug mode for step-by-step execution
        #[arg(long = "debug")]
        debug: bool,
    },
    /// Re-execute the current workflow, optionally with new input.
    #[command(name = "rerun")]
    Rerun {
        #[command(flatten)]
        input_args: InputArgs,

        /// Enable debug mode for step-by-step execution
        #[arg(long = "debug")]
        debug: bool,
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
    /// Show all steps in the workflow (debug mode only)
    #[command(name = "steps")]
    Steps,
    /// Evaluate a step: queue it and run all dependencies, return result (debug mode only)
    #[command(name = "eval")]
    Eval {
        /// Step ID to evaluate
        step_id: String,
    },
    /// Queue a step and its dependencies for execution (debug mode only)
    #[command(name = "queue")]
    Queue {
        /// Step ID to queue
        step_id: String,
    },
    /// Run the next ready step from the queue (debug mode only)
    #[command(name = "next")]
    Next,
    /// Run all steps in the queue until empty (debug mode only)
    #[command(name = "run-queue")]
    RunQueue,
    /// Show the result of a step, or indicate it hasn't been run yet (debug mode only)
    #[command(name = "show")]
    Show {
        /// Step ID to show result for
        step_id: String,
    },
    /// Inspect details of a specific step (debug mode only)
    #[command(name = "inspect")]
    Inspect {
        /// Step ID to inspect
        step_id: String,
    },
    /// Show completed steps and their results (debug mode only)
    #[command(name = "completed")]
    Completed,
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
        ReplCommand::Run {
            flow,
            input_args,
            debug,
        } => handle_run_command(flow, input_args, debug, state).await,
        ReplCommand::Rerun { input_args, debug } => {
            handle_rerun_command(input_args, debug, state).await
        }
        ReplCommand::Status => handle_status_command(state).await,
        ReplCommand::Workflow => handle_workflow_command(state).await,
        ReplCommand::Input => handle_input_command(state).await,
        ReplCommand::Quit | ReplCommand::Exit => {
            println!("Goodbye!");
            Ok(())
        }
        ReplCommand::Steps => handle_steps_command(state).await,
        ReplCommand::Eval { step_id } => handle_eval_command(step_id, state).await,
        ReplCommand::Queue { step_id } => handle_queue_command(step_id, state).await,
        ReplCommand::Next => handle_next_command(state).await,
        ReplCommand::RunQueue => handle_run_queue_command(state).await,
        ReplCommand::Show { step_id } => handle_show_command(step_id, state).await,
        ReplCommand::Inspect { step_id } => handle_inspect_command(step_id, state).await,
        ReplCommand::Completed => handle_completed_command(state).await,
    }
}

/// Handle the run command
async fn handle_run_command(
    flow_path: PathBuf,
    input_args: InputArgs,
    debug: bool,
    state: &mut ReplState,
) -> Result<()> {
    // Load flow
    let flow: Arc<Flow> = load(&flow_path)?;
    let flow_id = BlobId::from_flow(flow.as_ref()).change_context(MainError::Configuration)?;
    println!("Loaded flow: {}", flow_path.display());

    // Parse input with flow context
    let input_value = input_args.parse_input(true)?;

    // Create LastRun structure
    let mut last_run = LastRun::new(flow, flow_id, input_value);
    state.debug_mode = debug;

    if debug {
        // Create debug execution
        last_run.create_debug_execution(&state.executor).await?;

        println!("Debug mode enabled. Queue is empty.");
        println!("Use 'eval <step_id>' to evaluate a step (runs dependencies automatically).");
        println!("Or use 'queue <step_id>' + 'next' for step-by-step control.");
        println!("Use 'steps' to see all steps, 'show <step_id>' for results.");
    } else {
        // Execute flow normally
        last_run.execute_normal(&state.executor).await?;
    }

    // Store the last run
    state.last_run = Some(last_run);
    Ok(())
}

/// Handle the rerun command
async fn handle_rerun_command(
    input_args: InputArgs,
    debug: bool,
    state: &mut ReplState,
) -> Result<()> {
    let last_run = state.last_run.as_mut().ok_or_else(|| {
        MainError::ReplCommand("No workflow loaded. Use 'run' first.".to_string())
    })?;

    // Use new input if provided, otherwise use stored input
    if input_args.has_input() {
        let input_value = input_args.parse_input(false)?;
        last_run.update_input(input_value);
    }

    state.debug_mode = debug;

    if debug {
        // Create debug execution
        last_run.create_debug_execution(&state.executor).await?;

        println!("Debug mode enabled. Queue is empty.");
        println!("Use 'eval <step_id>' to evaluate a step (runs dependencies automatically).");
        println!("Or use 'queue <step_id>' + 'next' for step-by-step control.");
        Ok(())
    } else {
        // Execute workflow normally
        last_run.execute_normal(&state.executor).await
    }
}

/// Handle the status command
async fn handle_status_command(state: &ReplState) -> Result<()> {
    println!("REPL Status:");
    println!("  Config: {:?}", state.config_path);
    println!("  Debug mode: {}", state.debug_mode);

    let plugins = state.executor.list_plugins().await;
    println!("  Executor: {} plugins loaded", plugins.len());

    if let Some(last_run) = &state.last_run {
        println!("  Workflow: {} steps", last_run.flow.steps().len());
        if let Some(name) = last_run.flow.name() {
            println!("    Name: {name}");
        }
        println!("  Input: Loaded");
        if last_run.last_execution.is_some() {
            println!("  Debug execution: Active");
        }
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

/// Handle the steps command - show all steps in the workflow
async fn handle_steps_command(state: &ReplState) -> Result<()> {
    if !state.debug_mode {
        println!(
            "Steps command is only available in debug mode. Use 'run --debug' to enable debug mode."
        );
        return Ok(());
    }

    if let Some(last_run) = &state.last_run {
        if let Some(debug_session) = &last_run.last_execution {
            let all_steps = debug_session.list_all_steps().await;
            println!("Workflow steps ({} total):", all_steps.len());
            for step_status in &all_steps {
                println!(
                    "  [{}] {} ({}): {}",
                    step_status.step_index,
                    step_status.step_id,
                    step_status.status,
                    step_status.component
                );
            }
        } else {
            println!(
                "No debug session active. Use 'run --workflow=<file> --debug' to start debugging."
            );
        }
    } else {
        println!(
            "No debug session active. Use 'run --workflow=<file> --debug' to start debugging."
        );
    }
    Ok(())
}

/// Handle the eval command - evaluate a step (runs dependencies automatically)
async fn handle_eval_command(step_id: String, state: &mut ReplState) -> Result<()> {
    if !state.debug_mode {
        println!(
            "Eval command is only available in debug mode. Use 'run --debug' to enable debug mode."
        );
        return Ok(());
    }

    if let Some(last_run) = &mut state.last_run {
        if let Some(debug_session) = last_run.debug_execution() {
            match debug_session.eval_step(&step_id).await {
                Ok(result) => {
                    print_flow_result(&result)?;
                }
                Err(e) => {
                    println!("Failed to evaluate step '{step_id}': {e}");
                }
            }
        } else {
            println!(
                "No debug session active. Use 'run --flow=<file> --debug' to start debugging."
            );
        }
    } else {
        println!("No debug session active. Use 'run --flow=<file> --debug' to start debugging.");
    }
    Ok(())
}

/// Handle the queue command - queue a step and its dependencies
async fn handle_queue_command(step_id: String, state: &mut ReplState) -> Result<()> {
    if !state.debug_mode {
        println!(
            "Queue command is only available in debug mode. Use 'run --debug' to enable debug mode."
        );
        return Ok(());
    }

    if let Some(last_run) = &mut state.last_run {
        if let Some(debug_session) = last_run.debug_execution() {
            match debug_session.queue_step(&step_id) {
                Ok(newly_queued) => {
                    if newly_queued.is_empty() {
                        println!("Step '{step_id}' already queued.");
                    } else {
                        println!("Queued: [{}]", newly_queued.join(", "));
                    }
                    // Show what's ready to run
                    let ready_steps = debug_session.get_queued_steps();
                    let ready: Vec<_> = ready_steps
                        .iter()
                        .filter(|s| s.status == stepflow_core::status::StepStatus::Runnable)
                        .map(|s| s.step_id.as_str())
                        .collect();
                    if !ready.is_empty() {
                        println!("Ready to run: [{}]", ready.join(", "));
                    }
                }
                Err(e) => {
                    println!("Failed to queue step '{step_id}': {e}");
                }
            }
        } else {
            println!(
                "No debug session active. Use 'run --flow=<file> --debug' to start debugging."
            );
        }
    } else {
        println!("No debug session active. Use 'run --flow=<file> --debug' to start debugging.");
    }
    Ok(())
}

/// Handle the next command - run the next ready step
async fn handle_next_command(state: &mut ReplState) -> Result<()> {
    if !state.debug_mode {
        println!(
            "Next command is only available in debug mode. Use 'run --debug' to enable debug mode."
        );
        return Ok(());
    }

    if let Some(last_run) = &mut state.last_run {
        if let Some(debug_session) = last_run.debug_execution() {
            match debug_session.run_next_step().await {
                Ok(Some(result)) => {
                    println!("Running: {}", result.metadata.step_id);
                    print_flow_result(&result.result)?;
                    // Show what's ready now
                    let ready_steps = debug_session.get_queued_steps();
                    let ready: Vec<_> = ready_steps
                        .iter()
                        .filter(|s| s.status == stepflow_core::status::StepStatus::Runnable)
                        .map(|s| s.step_id.as_str())
                        .collect();
                    if ready.is_empty() {
                        println!("Queue empty.");
                    } else {
                        println!("Ready to run: [{}]", ready.join(", "));
                    }
                }
                Ok(None) => {
                    println!("No steps ready to run. Use 'queue <step_id>' to add steps.");
                }
                Err(e) => {
                    println!("Failed to run next step: {e}");
                }
            }
        } else {
            println!(
                "No debug session active. Use 'run --flow=<file> --debug' to start debugging."
            );
        }
    } else {
        println!("No debug session active. Use 'run --flow=<file> --debug' to start debugging.");
    }
    Ok(())
}

/// Handle the run-queue command - run all queued steps
async fn handle_run_queue_command(state: &mut ReplState) -> Result<()> {
    if !state.debug_mode {
        println!(
            "Run-queue command is only available in debug mode. Use 'run --debug' to enable debug mode."
        );
        return Ok(());
    }

    if let Some(last_run) = &mut state.last_run {
        if let Some(debug_session) = last_run.debug_execution() {
            match debug_session.run_queue().await {
                Ok(results) => {
                    if results.is_empty() {
                        println!("No steps to run. Use 'queue <step_id>' to add steps.");
                    } else {
                        println!("Ran {} steps:", results.len());
                        for result in &results {
                            println!(
                                "  {} - {:?}",
                                result.metadata.step_id,
                                match &result.result {
                                    stepflow_core::FlowResult::Success(_) => "SUCCESS",
                                    stepflow_core::FlowResult::Failed(_) => "FAILED",
                                }
                            );
                        }
                    }
                }
                Err(e) => {
                    println!("Failed to run queue: {e}");
                }
            }
        } else {
            println!(
                "No debug session active. Use 'run --flow=<file> --debug' to start debugging."
            );
        }
    } else {
        println!("No debug session active. Use 'run --flow=<file> --debug' to start debugging.");
    }
    Ok(())
}

/// Handle the show command - show the result of a step
async fn handle_show_command(step_id: String, state: &ReplState) -> Result<()> {
    if !state.debug_mode {
        println!(
            "Show command is only available in debug mode. Use 'run --debug' to enable debug mode."
        );
        return Ok(());
    }

    if let Some(last_run) = &state.last_run {
        if let Some(debug_session) = &last_run.last_execution {
            match debug_session.get_step_result(&step_id) {
                Some(result) => {
                    println!("Step '{step_id}':");
                    print_flow_result(&result)?;
                }
                None => {
                    println!("Step '{step_id}' has not been run yet.");
                }
            }
        } else {
            println!(
                "No debug session active. Use 'run --flow=<file> --debug' to start debugging."
            );
        }
    } else {
        println!("No debug session active. Use 'run --flow=<file> --debug' to start debugging.");
    }
    Ok(())
}

/// Handle the inspect command - inspect a specific step
async fn handle_inspect_command(step_id: String, state: &ReplState) -> Result<()> {
    if !state.debug_mode {
        println!(
            "Inspect command is only available in debug mode. Use 'run --debug' to enable debug mode."
        );
        return Ok(());
    }

    if let Some(last_run) = &state.last_run {
        if let Some(debug_session) = &last_run.last_execution {
            match debug_session.inspect_step(&step_id).await {
                Ok(inspection) => {
                    println!("Step '{step_id}' inspection:");
                    println!("  Index: {}", inspection.metadata.step_index);
                    println!("  Component: {}", inspection.metadata.component);
                    println!("  State: {:?}", inspection.state);

                    let input_json = serde_json::to_string_pretty(&inspection.input)
                        .change_context(MainError::FlowExecution)?;
                    println!("  Input: {input_json}");

                    println!("  Error handling: {:?}", inspection.on_error);
                }
                Err(e) => {
                    println!("Failed to inspect step '{step_id}': {e}");
                }
            }
        } else {
            println!(
                "No debug session active. Use 'run --workflow=<file> --debug' to start debugging."
            );
        }
    } else {
        println!(
            "No debug session active. Use 'run --workflow=<file> --debug' to start debugging."
        );
    }
    Ok(())
}

/// Handle the completed command - show completed steps
async fn handle_completed_command(state: &ReplState) -> Result<()> {
    if !state.debug_mode {
        println!(
            "Completed command is only available in debug mode. Use 'run --debug' to enable debug mode."
        );
        return Ok(());
    }

    if let Some(last_run) = &state.last_run {
        if let Some(debug_session) = &last_run.last_execution {
            match debug_session.get_completed_steps().await {
                Ok(completed_steps) => {
                    if completed_steps.is_empty() {
                        println!("No steps have been completed yet.");
                    } else {
                        println!("Completed steps ({} total):", completed_steps.len());
                        for step in &completed_steps {
                            let status = match &step.result {
                                stepflow_core::FlowResult::Success(_) => "SUCCESS",
                                stepflow_core::FlowResult::Failed(_) => "FAILED",
                            };
                            println!(
                                "  [{}] {} ({}): {}",
                                step.metadata.step_index,
                                step.metadata.step_id,
                                status,
                                step.metadata.component
                            );
                        }
                    }
                }
                Err(e) => {
                    println!("Failed to get completed steps: {e}");
                }
            }
        } else {
            println!(
                "No debug session active. Use 'run --workflow=<file> --debug' to start debugging."
            );
        }
    } else {
        println!(
            "No debug session active. Use 'run --workflow=<file> --debug' to start debugging."
        );
    }
    Ok(())
}

/// Print a FlowResult in a formatted way
fn print_flow_result(result: &stepflow_core::FlowResult) -> Result<()> {
    match result {
        stepflow_core::FlowResult::Success(result) => {
            let result_json = serde_json::to_string_pretty(result.as_ref())
                .change_context(MainError::FlowExecution)?;
            println!("Result: {result_json}");
        }
        stepflow_core::FlowResult::Failed(error) => {
            println!("Result: FAILED - {error}");
        }
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

    // Add some additional context
    println!();
    println!("Note: Debug commands (eval, queue, next, etc.) are only available when");
    println!("a workflow is loaded in debug mode using 'run --flow=<file> --debug'.");
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
        let commands = vec![
            ("status", "Status"),
            ("quit", "Quit"),
            ("exit", "Exit"),
            ("steps", "Steps"),
            ("next", "Next"),
            ("run-queue", "RunQueue"),
        ];

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
    fn test_new_debug_commands_parsing() {
        // Test eval command
        let result = parse_command("eval step1");
        match result {
            Ok(ReplCommand::Eval { step_id }) => {
                assert_eq!(step_id, "step1");
            }
            _ => panic!("Expected eval command, got: {result:?}"),
        }

        // Test queue command
        let result = parse_command("queue step2");
        match result {
            Ok(ReplCommand::Queue { step_id }) => {
                assert_eq!(step_id, "step2");
            }
            _ => panic!("Expected queue command, got: {result:?}"),
        }

        // Test show command
        let result = parse_command("show step3");
        match result {
            Ok(ReplCommand::Show { step_id }) => {
                assert_eq!(step_id, "step3");
            }
            _ => panic!("Expected show command, got: {result:?}"),
        }
    }

    #[test]
    fn test_rerun_with_input() {
        // Test with simple JSON (without spaces that would cause parsing issues)
        let result = parse_command("rerun --input-json '{\"test\":123}'");
        match result {
            Ok(ReplCommand::Rerun { input_args, debug }) => {
                assert_eq!(input_args.input_json, Some("'{\"test\":123}'".to_string()));
                assert!(!debug);
            }
            _ => panic!("Expected rerun command, got: {result:?}"),
        }
    }

    #[test]
    fn test_run_command_parsing() {
        let result = parse_command("run --flow test.yaml --debug");
        match result {
            Ok(ReplCommand::Run {
                flow: workflow,
                input_args,
                debug,
            }) => {
                assert_eq!(workflow.to_string_lossy(), "test.yaml");
                assert!(debug);
                assert!(!input_args.has_input());
            }
            _ => panic!("Expected run command, got: {result:?}"),
        }
    }
}
