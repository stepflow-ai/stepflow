#![allow(clippy::print_stdout, clippy::print_stderr)]
use clap::Parser as _;
use error_stack::ResultExt as _;
use rustyline::{error::ReadlineError, DefaultEditor};
use std::{path::PathBuf, sync::Arc};
use stepflow_core::workflow::{Flow, ValueRef};
use stepflow_execution::StepFlowExecutor;
use stepflow_plugin::Context as _;

use crate::{
    args::{ConfigArgs, WorkflowLoader, load},
    args::InputArgs,
    MainError, Result,
};

/// State maintained by the REPL
pub struct ReplState {
    executor: Option<Arc<StepFlowExecutor>>,
    workflow: Option<Arc<Flow>>,
    input: Option<ValueRef>,
    config_path: Option<PathBuf>,
    debug_mode: bool,
    // TODO: Add debug execution state
}

impl ReplState {
    fn new(config_path: Option<PathBuf>) -> Self {
        Self {
            executor: None,
            workflow: None,
            input: None,
            config_path,
            debug_mode: false,
        }
    }
}

/// REPL-specific input arguments that extend the shared InputArgs
#[derive(clap::Args, Debug, Clone, Default)]
pub struct ReplInputArgs {
    #[command(flatten)]
    input_args: InputArgs,

    /// Enable debug mode for step-by-step execution
    #[arg(long = "debug")]
    pub debug: bool,
}

impl ReplInputArgs {
    /// Parse input from the various input sources into a ValueRef
    pub fn parse_input(&self) -> Result<ValueRef> {
        self.input_args.parse_input_with_stdin(false)
    }

    /// Check if any input is provided
    pub fn has_input(&self) -> bool {
        self.input_args.has_input()
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
        #[arg(long = "workflow", value_name = "FILE", value_hint = clap::ValueHint::FilePath)]
        workflow: PathBuf,
        
        #[command(flatten)]
        input_args: ReplInputArgs,
    },
    /// Re-execute the current workflow, optionally with new input
    #[command(name = "rerun")]
    Rerun {
        #[command(flatten)]
        input_args: ReplInputArgs,
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
    /// Show currently runnable steps (debug mode only)
    #[command(name = "runnable")]
    Runnable,
    /// Execute a specific step (debug mode only)
    #[command(name = "run-step")]
    RunStep {
        /// Step ID to execute
        step_id: String,
    },
    /// Continue workflow execution from current state (debug mode only)
    #[command(name = "continue")]
    Continue,
    /// Pause workflow execution (debug mode only)
    #[command(name = "pause")]
    Pause,
    /// Inspect details of a specific step (debug mode only)
    #[command(name = "inspect")]
    Inspect {
        /// Step ID to inspect
        step_id: String,
    },
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

/// Execute a workflow with the given input, handling both debug and normal modes
async fn execute_workflow(
    executor: &StepFlowExecutor,
    workflow: Arc<Flow>, 
    input_value: ValueRef,
    debug: bool,
) -> Result<()> {
    if debug {
        println!("Debug mode enabled. Workflow loaded but not started.");
        println!("Use 'steps' to see all steps, 'runnable' to see runnable steps.");
        println!("Use 'run-step <step_id>' to execute a specific step, or 'continue' to run all.");
        return Ok(());
    }
    
    // Execute workflow normally
    let execution_id = executor
        .submit_flow(workflow, input_value)
        .await
        .change_context(MainError::FlowExecution)?;
    
    let result = executor
        .flow_result(execution_id)
        .await
        .change_context(MainError::FlowExecution)?;
    
    // Display result
    let result_json = serde_json::to_string_pretty(&result)
        .change_context(MainError::FlowExecution)?;
    println!("Result:\n{}", result_json);
    
    Ok(())
}

/// Handle a parsed command
async fn handle_command(command: ReplCommand, state: &mut ReplState) -> Result<()> {
    match command {
        ReplCommand::Run { workflow, input_args } => {
            handle_run_command(workflow, input_args, state).await
        }
        ReplCommand::Rerun { input_args } => {
            handle_rerun_command(input_args, state).await
        }
        ReplCommand::Status => {
            handle_status_command(state).await
        }
        ReplCommand::Workflow => {
            handle_workflow_command(state).await
        }
        ReplCommand::Input => {
            handle_input_command(state).await
        }
        ReplCommand::Quit | ReplCommand::Exit => {
            println!("Goodbye!");
            Ok(())
        }
        ReplCommand::Steps => {
            handle_steps_command(state).await
        }
        ReplCommand::Runnable => {
            handle_runnable_command(state).await
        }
        ReplCommand::RunStep { step_id } => {
            handle_run_step_command(step_id, state).await
        }
        ReplCommand::Continue => {
            handle_continue_command(state).await
        }
        ReplCommand::Pause => {
            handle_pause_command(state).await
        }
        ReplCommand::Inspect { step_id } => {
            handle_inspect_command(step_id, state).await
        }
    }
}

/// Handle the run command
async fn handle_run_command(
    workflow_path: PathBuf,
    input_args: ReplInputArgs,
    state: &mut ReplState,
) -> Result<()> {
    // Load workflow
    let workflow: Arc<Flow> = load(&workflow_path)?;
    println!("Loaded workflow: {}", workflow_path.display());
    
    // Load config and create executor
    let config_args = ConfigArgs::with_path(state.config_path.clone());
    let config = config_args.load_config(Some(&workflow_path))?;
    let executor = WorkflowLoader::create_executor_from_config(config).await?;
    println!("Created executor with {} plugins", executor.list_plugins().await.len());
    
    // Parse input
    let input_value = input_args.parse_input()?;
    
    // Store state for potential rerun
    state.executor = Some(executor.clone());
    state.workflow = Some(workflow.clone());
    state.input = Some(input_value.clone());
    state.debug_mode = input_args.debug;
    
    // Execute workflow
    execute_workflow(&executor, workflow, input_value, input_args.debug).await
}

/// Handle the rerun command
async fn handle_rerun_command(
    input_args: ReplInputArgs,
    state: &mut ReplState,
) -> Result<()> {
    let executor = state.executor.as_ref()
        .ok_or_else(|| MainError::ReplCommand("No workflow loaded. Use 'run' first.".to_string()))?;
    let workflow = state.workflow.as_ref()
        .ok_or_else(|| MainError::ReplCommand("No workflow loaded. Use 'run' first.".to_string()))?;
    
    // Use new input if provided, otherwise use stored input
    let input_value = if input_args.has_input() {
        input_args.parse_input()?
    } else {
        state.input.as_ref()
            .ok_or_else(|| MainError::ReplCommand("No input available. Provide input or run workflow first.".to_string()))?
            .clone()
    };
    
    // Update state
    state.input = Some(input_value.clone());
    state.debug_mode = input_args.debug;
    
    // Execute workflow
    execute_workflow(executor, workflow.clone(), input_value, input_args.debug).await
}

/// Handle the status command
async fn handle_status_command(state: &ReplState) -> Result<()> {
    println!("REPL Status:");
    println!("  Config: {:?}", state.config_path);
    println!("  Debug mode: {}", state.debug_mode);
    
    if let Some(executor) = &state.executor {
        let plugins = executor.list_plugins().await;
        println!("  Executor: {} plugins loaded", plugins.len());
    } else {
        println!("  Executor: Not loaded");
    }
    
    if let Some(workflow) = &state.workflow {
        println!("  Workflow: {} steps", workflow.steps.len());
        if let Some(name) = &workflow.name {
            println!("    Name: {}", name);
        }
    } else {
        println!("  Workflow: Not loaded");
    }
    
    if state.input.is_some() {
        println!("  Input: Loaded");
    } else {
        println!("  Input: Not loaded");
    }
    
    Ok(())
}

/// Handle the workflow command
async fn handle_workflow_command(state: &ReplState) -> Result<()> {
    if let Some(workflow) = &state.workflow {
        let workflow_json = serde_json::to_string_pretty(workflow.as_ref())
            .change_context(MainError::FlowExecution)?;
        println!("Current workflow:\n{}", workflow_json);
    } else {
        println!("No workflow loaded. Use 'run --workflow=<file>' to load a workflow.");
    }
    Ok(())
}

/// Handle the input command
async fn handle_input_command(state: &ReplState) -> Result<()> {
    if let Some(input) = &state.input {
        let input_json = serde_json::to_string_pretty(input.as_ref())
            .change_context(MainError::FlowExecution)?;
        println!("Current input:\n{}", input_json);
    } else {
        println!("No input loaded. Use 'run' or 'rerun' with input to load input.");
    }
    Ok(())
}

/// Handle the steps command - show all steps in the workflow
async fn handle_steps_command(state: &ReplState) -> Result<()> {
    if !state.debug_mode {
        println!("Steps command is only available in debug mode. Use 'run --debug' to enable debug mode.");
        return Ok(());
    }
    
    if let Some(workflow) = &state.workflow {
        println!("Workflow steps ({} total):", workflow.steps.len());
        for (i, step) in workflow.steps.iter().enumerate() {
            println!("  [{}] {}: {}", i, step.id, step.component);
        }
    } else {
        println!("No workflow loaded. Use 'run --workflow=<file> --debug' to load a workflow in debug mode.");
    }
    Ok(())
}

/// Handle the runnable command - show currently runnable steps
async fn handle_runnable_command(state: &ReplState) -> Result<()> {
    if !state.debug_mode {
        println!("Runnable command is only available in debug mode. Use 'run --debug' to enable debug mode.");
        return Ok(());
    }
    
    // TODO: Implement actual dependency analysis to show runnable steps
    if let Some(workflow) = &state.workflow {
        println!("Currently runnable steps:");
        // For now, just show the first step as runnable
        if !workflow.steps.is_empty() {
            println!("  [0] {}: {}", workflow.steps[0].id, workflow.steps[0].component);
        }
        println!("(Note: Full dependency analysis not yet implemented)");
    } else {
        println!("No workflow loaded. Use 'run --workflow=<file> --debug' to load a workflow in debug mode.");
    }
    Ok(())
}

/// Handle the run-step command - execute a specific step
async fn handle_run_step_command(step_id: String, state: &ReplState) -> Result<()> {
    if !state.debug_mode {
        println!("Run-step command is only available in debug mode. Use 'run --debug' to enable debug mode.");
        return Ok(());
    }
    
    // TODO: Implement step-by-step execution
    println!("Executing step '{}' (not yet implemented)", step_id);
    Ok(())
}

/// Handle the continue command - continue workflow execution
async fn handle_continue_command(state: &ReplState) -> Result<()> {
    if !state.debug_mode {
        println!("Continue command is only available in debug mode. Use 'run --debug' to enable debug mode.");
        return Ok(());
    }
    
    // TODO: Implement continue execution from current state
    println!("Continuing workflow execution (not yet implemented)");
    Ok(())
}

/// Handle the pause command - pause workflow execution
async fn handle_pause_command(state: &ReplState) -> Result<()> {
    if !state.debug_mode {
        println!("Pause command is only available in debug mode. Use 'run --debug' to enable debug mode.");
        return Ok(());
    }
    
    // TODO: Implement pause functionality
    println!("Pausing workflow execution (not yet implemented)");
    Ok(())
}

/// Handle the inspect command - inspect a specific step
async fn handle_inspect_command(step_id: String, state: &ReplState) -> Result<()> {
    if !state.debug_mode {
        println!("Inspect command is only available in debug mode. Use 'run --debug' to enable debug mode.");
        return Ok(());
    }
    
    if let Some(workflow) = &state.workflow {
        if let Some(step) = workflow.steps.iter().find(|s| s.id == step_id) {
            let step_json = serde_json::to_string_pretty(step)
                .change_context(MainError::FlowExecution)?;
            println!("Step '{}' details:\n{}", step_id, step_json);
        } else {
            println!("Step '{}' not found in workflow", step_id);
        }
    } else {
        println!("No workflow loaded. Use 'run --workflow=<file> --debug' to load a workflow in debug mode.");
    }
    Ok(())
}


/// Print help information
fn print_help() {
    println!("StepFlow REPL Commands:");
    println!();
    println!("Basic Commands:");
    println!("  run --workflow=<file> [--input=<file>] [--input-json=<json>] [--input-yaml=<yaml>] [--debug]");
    println!("    Load and execute a workflow with input. Use --debug for step-by-step execution");
    println!();
    println!("  rerun [--input=<file>] [--input-json=<json>] [--input-yaml=<yaml>] [--debug]");
    println!("    Re-execute the current workflow, optionally with new input and debug mode");
    println!();
    println!("  status");
    println!("    Show current REPL state (loaded workflow, executor, debug mode, etc.)");
    println!();
    println!("  workflow");
    println!("    Display the currently loaded workflow");
    println!();
    println!("  input");
    println!("    Display the current input");
    println!();
    println!("  help");
    println!("    Show this help message");
    println!();
    println!("  quit, exit");
    println!("    Exit the REPL");
    println!();
    println!("Debug Commands (only available in debug mode):");
    println!("  steps");
    println!("    Show all steps in the workflow");
    println!();
    println!("  runnable");
    println!("    Show currently runnable steps");
    println!();
    println!("  run-step <step_id>");
    println!("    Execute a specific step");
    println!();
    println!("  continue");
    println!("    Continue workflow execution from current state");
    println!();
    println!("  pause");
    println!("    Pause workflow execution");
    println!();
    println!("  inspect <step_id>");
    println!("    Inspect details of a specific step");
    println!();
}

/// Main REPL function
pub async fn run_repl(config_path: Option<PathBuf>) -> Result<()> {
    let mut rl = DefaultEditor::new()
        .change_context(MainError::ReplInit)?;
    
    let mut state = ReplState::new(config_path);
    
    println!("StepFlow REPL v{}", env!("CARGO_PKG_VERSION"));
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
                            eprintln!("Error: {}", e);
                        }
                    }
                    Err(e) => {
                        let error_msg = e.to_string();
                        // Check if this is a help request (clap handles help automatically)
                        if error_msg.contains("help") || line.trim() == "help" {
                            print_help();
                        } else {
                            eprintln!("Error: {}", error_msg);
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
                eprintln!("Error reading line: {:?}", err);
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
    fn test_input_args_parsing() {
        // Test empty args
        let args = ReplInputArgs::default();
        assert!(!args.has_input());
        
        // Test debug flag
        let mut args = ReplInputArgs::default();
        args.debug = true;
        assert!(args.debug);
    }

    #[test]
    fn test_command_parsing() {
        // Test basic commands that don't require files
        let commands = vec![
            ("status", "Status"),
            ("quit", "Quit"),
            ("exit", "Exit"),
            ("steps", "Steps"),
            ("runnable", "Runnable"),
        ];

        for (input, expected_variant) in commands {
            let result = parse_command(input);
            match result {
                Ok(command) => {
                    let debug_str = format!("{:?}", command);
                    assert!(debug_str.starts_with(expected_variant), 
                            "Expected {} to parse as {} variant, got: {:?}", 
                            input, expected_variant, command);
                }
                Err(e) => panic!("Failed to parse '{}': {}", input, e),
            }
        }
    }

    #[test]
    fn test_rerun_with_input() {
        // Test with simple JSON (without spaces that would cause parsing issues)
        let result = parse_command("rerun --input-json '{\"test\":123}'");
        match result {
            Ok(ReplCommand::Rerun { input_args }) => {
                assert_eq!(input_args.input_args.input_json, Some("'{\"test\":123}'".to_string()));
                assert!(!input_args.debug);
            }
            _ => panic!("Expected rerun command, got: {:?}", result),
        }
    }

    #[test]
    fn test_run_command_parsing() {
        let result = parse_command("run --workflow test.yaml --debug");
        match result {
            Ok(ReplCommand::Run { workflow, input_args }) => {
                assert_eq!(workflow.to_string_lossy(), "test.yaml");
                assert!(input_args.debug);
                assert!(!input_args.has_input());
            }
            _ => panic!("Expected run command, got: {:?}", result),
        }
    }
}