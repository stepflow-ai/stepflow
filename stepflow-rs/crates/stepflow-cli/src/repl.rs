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

//! Interactive REPL for debugging workflows.
//!
//! Provides GDB-like commands for step-by-step workflow debugging:
//! - `info` - List all steps with status
//! - `info <step_id>` - Show detailed step information
//! - `status` - Show current debug session status
//! - `eval <step_id>` - Evaluate a step (run with dependencies)
//! - `next` - Execute next ready step
//! - `step` - Step into (same as next for now; for sub-flows later)
//! - `continue` - Run to completion
//! - `history` - Show debug event history

#![allow(clippy::print_stdout, clippy::print_stderr)]

use clap::Parser as _;
use error_stack::ResultExt as _;
use rustyline::{DefaultEditor, error::ReadlineError};
use std::{path::PathBuf, sync::Arc};
use stepflow_core::{
    BlobId,
    workflow::{Flow, ValueRef},
};
use stepflow_dtos::PendingAction;
use stepflow_execution::{DebugExecutor, StepflowExecutor};
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
    pub last_execution: Option<DebugExecutor>,
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
        let mut submit_params = stepflow_core::SubmitRunParams::new(
            self.flow.clone(),
            self.flow_id.clone(),
            vec![self.input.clone()],
        );
        submit_params = submit_params.with_wait(true);

        let run_status = executor
            .submit_run(submit_params)
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

    /// Create a debug execution for this workflow
    pub async fn create_debug_execution(
        &mut self,
        executor: &Arc<StepflowExecutor>,
    ) -> Result<&mut DebugExecutor> {
        let state_store = executor.state_store();
        let run_id = uuid::Uuid::now_v7();
        let debug_executor = DebugExecutor::new(
            executor.clone(),
            self.flow.clone(),
            self.flow_id.clone(),
            run_id,
            self.input.clone(),
            state_store.clone(),
            None, // TODO: Add variables support to REPL
        )
        .await
        .change_context(MainError::FlowExecution)?;

        self.last_execution = Some(debug_executor);
        Ok(self.last_execution.as_mut().unwrap())
    }

    /// Get the current debug execution, if any
    pub fn debug_execution(&mut self) -> Option<&mut DebugExecutor> {
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

    /// Re-execute the current workflow, optionally with new input
    #[command(name = "rerun")]
    Rerun {
        #[command(flatten)]
        input_args: InputArgs,

        /// Enable debug mode for step-by-step execution
        #[arg(long = "debug")]
        debug: bool,
    },

    /// Display workflow or step information (debug mode)
    #[command(name = "info")]
    Info {
        /// Step ID to show details for (optional)
        step_id: Option<String>,

        /// Filter to show only completed steps
        #[arg(long = "completed")]
        completed: bool,

        /// Filter to show only pending steps
        #[arg(long = "pending")]
        pending: bool,

        /// Show steps for a different run ID
        #[arg(long = "run")]
        run_id: Option<uuid::Uuid>,
    },

    /// Show current debug session status (debug mode)
    #[command(name = "status")]
    Status,

    /// Evaluate a step: queue it and run with dependencies (debug mode)
    #[command(name = "eval")]
    Eval {
        /// Step ID to evaluate
        step_id: String,
    },

    /// Execute the next ready step (debug mode)
    #[command(name = "next", visible_alias = "n")]
    Next,

    /// Step into (same as next; for sub-flows in the future) (debug mode)
    #[command(name = "step", visible_alias = "s")]
    Step,

    /// Run to completion (debug mode)
    #[command(name = "continue", visible_alias = "c")]
    Continue,

    /// Show debug event history (debug mode)
    #[command(name = "history")]
    History {
        /// Maximum number of events to show (default: 10)
        #[arg(short = 'n', default_value = "10")]
        limit: usize,

        /// Show all events
        #[arg(long = "all")]
        all: bool,
    },

    /// Display the currently loaded workflow
    #[command(name = "workflow")]
    Workflow,

    /// Display the current input
    #[command(name = "input")]
    Input,

    /// Exit the REPL
    #[command(name = "quit", visible_alias = "q")]
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
        ReplCommand::Run {
            flow,
            input_args,
            debug,
        } => handle_run_command(flow, input_args, debug, state).await,
        ReplCommand::Rerun { input_args, debug } => {
            handle_rerun_command(input_args, debug, state).await
        }
        ReplCommand::Info {
            step_id,
            completed,
            pending,
            run_id: _,
        } => handle_info_command(step_id, completed, pending, state).await,
        ReplCommand::Status => handle_status_command(state).await,
        ReplCommand::Eval { step_id } => handle_eval_command(step_id, state).await,
        ReplCommand::Next => handle_next_command(state).await,
        ReplCommand::Step => handle_step_command(state).await,
        ReplCommand::Continue => handle_continue_command(state).await,
        ReplCommand::History { limit, all } => {
            let limit = if all { usize::MAX } else { limit };
            handle_history_command(limit, state).await
        }
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
        let debug_session = last_run.create_debug_execution(&state.executor).await?;
        let status = debug_session.get_status();

        println!("Debug mode enabled. Run ID: {}", status.run_id);
        println!();
        print_status(&status);
        println!();
        println!("Commands: info, eval <step>, next, continue, history");
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
        let debug_session = last_run.create_debug_execution(&state.executor).await?;
        let status = debug_session.get_status();

        println!("Debug mode enabled. Run ID: {}", status.run_id);
        println!();
        print_status(&status);
        Ok(())
    } else {
        // Execute workflow normally
        last_run.execute_normal(&state.executor).await
    }
}

/// Handle the info command - show step information
async fn handle_info_command(
    step_id: Option<String>,
    completed: bool,
    pending: bool,
    state: &ReplState,
) -> Result<()> {
    if !state.debug_mode {
        println!("Info command requires debug mode. Use 'run --debug' to enable.");
        return Ok(());
    }

    let last_run = state
        .last_run
        .as_ref()
        .ok_or_else(|| MainError::ReplCommand("No workflow loaded.".to_string()))?;

    let debug_session = last_run
        .last_execution
        .as_ref()
        .ok_or_else(|| MainError::ReplCommand("No debug session active.".to_string()))?;

    match step_id {
        Some(step_id) => {
            // Show detailed info for a specific step
            match debug_session.get_step_detail(&step_id).await {
                Ok(detail) => {
                    println!("Step: {}", detail.info.step_id);
                    println!("  Index: {}", detail.info.step_index);
                    println!("  Component: {}", detail.info.component);
                    println!("  Status: {:?}", detail.info.status);

                    if !detail.dependencies.is_empty() {
                        println!("  Dependencies: [{}]", detail.dependencies.join(", "));
                    }

                    let input_json = serde_json::to_string_pretty(&detail.input)
                        .change_context(MainError::FlowExecution)?;
                    println!("  Input: {input_json}");

                    println!("  On Error: {:?}", detail.on_error);

                    if let Some(result) = &detail.result {
                        print_flow_result(result)?;
                    }
                }
                Err(e) => {
                    println!("Step '{}' not found: {}", step_id, e);
                }
            }
        }
        None => {
            // List all steps with optional filtering
            let steps = debug_session.get_steps_info().await;

            let filtered: Vec<_> = steps
                .iter()
                .filter(|s| {
                    if completed {
                        s.status == stepflow_core::status::StepStatus::Completed
                    } else if pending {
                        s.status == stepflow_core::status::StepStatus::Runnable
                            || s.status == stepflow_core::status::StepStatus::Blocked
                    } else {
                        true
                    }
                })
                .collect();

            if filtered.is_empty() {
                if completed {
                    println!("No completed steps.");
                } else if pending {
                    println!("No pending steps.");
                } else {
                    println!("No steps in workflow.");
                }
            } else {
                let filter_label = if completed {
                    "Completed steps"
                } else if pending {
                    "Pending steps"
                } else {
                    "All steps"
                };
                println!("{} ({}):", filter_label, filtered.len());
                for step in filtered {
                    let status_str = format!("{:?}", step.status);
                    println!(
                        "  [{:2}] {:20} {:12} {}",
                        step.step_index, step.step_id, status_str, step.component
                    );
                }
            }
        }
    }

    Ok(())
}

/// Handle the status command
async fn handle_status_command(state: &ReplState) -> Result<()> {
    if !state.debug_mode {
        // Show basic REPL status
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
        } else {
            println!("  Workflow: Not loaded");
        }
        return Ok(());
    }

    let last_run = state
        .last_run
        .as_ref()
        .ok_or_else(|| MainError::ReplCommand("No workflow loaded.".to_string()))?;

    let debug_session = last_run
        .last_execution
        .as_ref()
        .ok_or_else(|| MainError::ReplCommand("No debug session active.".to_string()))?;

    let status = debug_session.get_status();
    print_status(&status);

    Ok(())
}

/// Print debug status in a formatted way
fn print_status(status: &stepflow_dtos::DebugStatus) {
    println!("Run: {}", status.run_id);
    println!(
        "Progress: {}/{} steps completed",
        status.completed_count, status.total_steps
    );

    match &status.pending {
        PendingAction::ExecuteStep { step_id, .. } => {
            println!("Pending: ExecuteStep {{ step: \"{}\" }}", step_id);
        }
        PendingAction::AwaitingInput => {
            println!("Pending: AwaitingInput");
            println!("  Use 'eval <step_id>' to queue and run a step");
        }
        PendingAction::Complete => {
            println!("Pending: Complete");
        }
    }

    if !status.ready_steps.is_empty() {
        println!("Ready: [{}]", status.ready_steps.join(", "));
    }
}

/// Handle the eval command
async fn handle_eval_command(step_id: String, state: &mut ReplState) -> Result<()> {
    if !state.debug_mode {
        println!("Eval command requires debug mode. Use 'run --debug' to enable.");
        return Ok(());
    }

    let last_run = state
        .last_run
        .as_mut()
        .ok_or_else(|| MainError::ReplCommand("No workflow loaded.".to_string()))?;

    let debug_session = last_run
        .debug_execution()
        .ok_or_else(|| MainError::ReplCommand("No debug session active.".to_string()))?;

    match debug_session.eval_step(&step_id).await {
        Ok(result) => {
            println!("Evaluated: {}", step_id);
            print_flow_result(&result)?;
        }
        Err(e) => {
            println!("Failed to evaluate '{}': {}", step_id, e);
        }
    }

    Ok(())
}

/// Handle the next command
async fn handle_next_command(state: &mut ReplState) -> Result<()> {
    if !state.debug_mode {
        println!("Next command requires debug mode. Use 'run --debug' to enable.");
        return Ok(());
    }

    let last_run = state
        .last_run
        .as_mut()
        .ok_or_else(|| MainError::ReplCommand("No workflow loaded.".to_string()))?;

    let debug_session = last_run
        .debug_execution()
        .ok_or_else(|| MainError::ReplCommand("No debug session active.".to_string()))?;

    match debug_session.run_next_step().await {
        Ok(Some(result)) => {
            println!("Executed: {}", result.metadata.step_id);
            print_flow_result(&result.result)?;

            // Show updated status
            let status = debug_session.get_status();
            println!();
            print_status(&status);
        }
        Ok(None) => {
            println!("No steps ready to execute.");
            println!("Use 'eval <step_id>' to queue a step and its dependencies.");
        }
        Err(e) => {
            println!("Failed to execute next step: {}", e);
        }
    }

    Ok(())
}

/// Handle the step command (same as next for now)
async fn handle_step_command(state: &mut ReplState) -> Result<()> {
    // For now, step behaves the same as next
    // When sub-flows are implemented, step will enter sub-flows
    handle_next_command(state).await
}

/// Handle the continue command
async fn handle_continue_command(state: &mut ReplState) -> Result<()> {
    if !state.debug_mode {
        println!("Continue command requires debug mode. Use 'run --debug' to enable.");
        return Ok(());
    }

    let last_run = state
        .last_run
        .as_mut()
        .ok_or_else(|| MainError::ReplCommand("No workflow loaded.".to_string()))?;

    let debug_session = last_run
        .debug_execution()
        .ok_or_else(|| MainError::ReplCommand("No debug session active.".to_string()))?;

    match debug_session.continue_to_completion().await {
        Ok((result, steps_executed)) => {
            println!("Completed: {} steps executed", steps_executed);
            print_flow_result(&result)?;

            let status = debug_session.get_status();
            println!();
            print_status(&status);
        }
        Err(e) => {
            println!("Failed to continue: {}", e);
        }
    }

    Ok(())
}

/// Handle the history command
async fn handle_history_command(limit: usize, state: &ReplState) -> Result<()> {
    if !state.debug_mode {
        println!("History command requires debug mode. Use 'run --debug' to enable.");
        return Ok(());
    }

    let last_run = state
        .last_run
        .as_ref()
        .ok_or_else(|| MainError::ReplCommand("No workflow loaded.".to_string()))?;

    let debug_session = last_run
        .last_execution
        .as_ref()
        .ok_or_else(|| MainError::ReplCommand("No debug session active.".to_string()))?;

    match debug_session.get_debug_events(limit, 0).await {
        Ok(events) => {
            if events.is_empty() {
                println!("No debug events recorded yet.");
            } else {
                println!("Debug events ({}, newest first):", events.len());
                for (i, event) in events.iter().enumerate() {
                    let event_str = format_debug_event(event);
                    println!("  [{i}] {event_str}");
                }
            }
        }
        Err(e) => {
            println!("Failed to get events: {}", e);
        }
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
        println!("No workflow loaded. Use 'run --flow=<file>' to load a workflow.");
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

/// Format a debug event for display
fn format_debug_event(event: &stepflow_dtos::DebugEvent) -> String {
    use stepflow_dtos::DebugEvent;
    match event {
        DebugEvent::StepQueued {
            step_index,
            step_id,
        } => {
            format!("QUEUED: [{step_index}] {step_id}")
        }
        DebugEvent::StepStarted {
            step_index,
            step_id,
            ..
        } => {
            format!("STARTED: [{step_index}] {step_id}")
        }
        DebugEvent::StepCompleted {
            step_index,
            step_id,
            result,
        } => {
            let status = match result {
                stepflow_core::FlowResult::Success(_) => "SUCCESS",
                stepflow_core::FlowResult::Failed(_) => "FAILED",
            };
            format!("COMPLETED: [{step_index}] {step_id} - {status}")
        }
        DebugEvent::StepFailed {
            step_index,
            step_id,
            error,
        } => {
            format!("FAILED: [{step_index}] {step_id} - {error}")
        }
        DebugEvent::RunCompleted { run_id, success } => {
            let status = if *success { "SUCCESS" } else { "FAILED" };
            format!("RUN COMPLETED: {run_id} - {status}")
        }
    }
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
    println!("Debug commands (info, eval, next, step, continue, history) require");
    println!("a workflow loaded in debug mode using 'run --flow=<file> --debug'.");
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
            ("next", "Next"),
            ("step", "Step"),
            ("continue", "Continue"),
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
    fn test_eval_command_parsing() {
        let result = parse_command("eval step1");
        match result {
            Ok(ReplCommand::Eval { step_id }) => {
                assert_eq!(step_id, "step1");
            }
            _ => panic!("Expected eval command, got: {result:?}"),
        }
    }

    #[test]
    fn test_info_command_parsing() {
        // Info without step_id
        let result = parse_command("info");
        match result {
            Ok(ReplCommand::Info {
                step_id,
                completed,
                pending,
                ..
            }) => {
                assert!(step_id.is_none());
                assert!(!completed);
                assert!(!pending);
            }
            _ => panic!("Expected info command, got: {result:?}"),
        }

        // Info with step_id
        let result = parse_command("info step1");
        match result {
            Ok(ReplCommand::Info { step_id, .. }) => {
                assert_eq!(step_id, Some("step1".to_string()));
            }
            _ => panic!("Expected info command with step_id, got: {result:?}"),
        }

        // Info with --completed
        let result = parse_command("info --completed");
        match result {
            Ok(ReplCommand::Info { completed, .. }) => {
                assert!(completed);
            }
            _ => panic!("Expected info command with --completed, got: {result:?}"),
        }
    }

    #[test]
    fn test_history_command_parsing() {
        // Default limit
        let result = parse_command("history");
        match result {
            Ok(ReplCommand::History { limit, all }) => {
                assert_eq!(limit, 10);
                assert!(!all);
            }
            _ => panic!("Expected history command, got: {result:?}"),
        }

        // Custom limit
        let result = parse_command("history -n 50");
        match result {
            Ok(ReplCommand::History { limit, all }) => {
                assert_eq!(limit, 50);
                assert!(!all);
            }
            _ => panic!("Expected history command with limit, got: {result:?}"),
        }

        // All events
        let result = parse_command("history --all");
        match result {
            Ok(ReplCommand::History { all, .. }) => {
                assert!(all);
            }
            _ => panic!("Expected history command with --all, got: {result:?}"),
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

    #[test]
    fn test_command_aliases() {
        // Test 'n' as alias for 'next'
        let result = parse_command("n");
        assert!(matches!(result, Ok(ReplCommand::Next)));

        // Test 's' as alias for 'step'
        let result = parse_command("s");
        assert!(matches!(result, Ok(ReplCommand::Step)));

        // Test 'c' as alias for 'continue'
        let result = parse_command("c");
        assert!(matches!(result, Ok(ReplCommand::Continue)));

        // Test 'q' as alias for 'quit'
        let result = parse_command("q");
        assert!(matches!(result, Ok(ReplCommand::Quit)));
    }
}
