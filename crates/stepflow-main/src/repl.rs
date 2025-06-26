#![allow(clippy::print_stdout, clippy::print_stderr)]
use clap::Parser as _;
use error_stack::ResultExt as _;
use rustyline::{DefaultEditor, error::ReadlineError};
use std::{path::PathBuf, sync::Arc};
use stepflow_core::workflow::{Flow, FlowHash, ValueRef};
use stepflow_execution::{StepExecutionResult, StepFlowExecutor, WorkflowExecutor};
use stepflow_plugin::Context as _;

use crate::{
    MainError, Result,
    args::InputArgs,
    args::{ConfigArgs, WorkflowLoader, load},
};

/// Information about the last workflow run
pub struct LastRun {
    pub workflow: Arc<Flow>,
    pub workflow_hash: FlowHash,
    pub workflow_path: PathBuf,
    pub input: ValueRef,
    pub last_execution: Option<Arc<tokio::sync::Mutex<WorkflowExecutor>>>,
    pub execution_id: Option<uuid::Uuid>,
}

impl LastRun {
    pub fn new(
        workflow: Arc<Flow>,
        workflow_hash: FlowHash,
        workflow_path: PathBuf,
        input: ValueRef,
    ) -> Self {
        Self {
            workflow,
            workflow_hash,
            workflow_path,
            input,
            last_execution: None,
            execution_id: None,
        }
    }

    /// Execute this workflow normally (non-debug mode)
    pub async fn execute_normal(&self, executor: &StepFlowExecutor) -> Result<()> {
        let execution_id = executor
            .submit_flow(
                self.workflow.clone(),
                self.workflow_hash.clone(),
                self.input.clone(),
            )
            .await
            .change_context(MainError::FlowExecution)?;

        let result = executor
            .flow_result(execution_id)
            .await
            .change_context(MainError::FlowExecution)?;

        // Display result
        let result_json =
            serde_json::to_string_pretty(&result).change_context(MainError::FlowExecution)?;
        println!("Result:\n{}", result_json);

        Ok(())
    }

    /// Create a debug execution for this workflow
    pub async fn create_debug_execution(
        &mut self,
        executor: &Arc<StepFlowExecutor>,
    ) -> Result<()> {
        let execution_id = uuid::Uuid::new_v4();
        
        // Submit the workflow to get it registered
        executor.submit_flow(
            self.workflow.clone(),
            self.workflow_hash.clone(),
            self.input.clone(),
        ).await.change_context(MainError::FlowExecution)?;
        
        // Get the debug session executor
        let debug_executor = executor.debug_session(execution_id)
            .await
            .change_context(MainError::FlowExecution)?;
        
        // Store the execution ID for later reference
        self.execution_id = Some(execution_id);
        
        // Store the executor
        self.last_execution = Some(debug_executor);
        
        Ok(())
    }

    /// Get the current debug execution, if any
    pub async fn execute_step(&mut self, step_id: &str) -> Result<StepExecutionResult> {
        if let Some(executor) = &self.last_execution {
            let mut workflow_executor = executor.lock().await;
            workflow_executor.execute_step_by_id(step_id).await
                .change_context(MainError::FlowExecution)
        } else {
            Err(error_stack::report!(MainError::NoDebugSession))
        }
    }

    /// Update input and clear any existing execution
    pub fn update_input(&mut self, input: ValueRef) {
        self.input = input;
        self.last_execution = None;
        self.execution_id = None;
    }
}

/// State maintained by the REPL
pub struct ReplState {
    executor: Arc<StepFlowExecutor>,
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
    /// Show currently runnable steps (debug mode only)
    #[command(name = "runnable")]
    Runnable,
    /// Execute a specific step (debug mode only)
    #[command(name = "run-step")]
    RunStep {
        /// Step ID to execute
        step_id: String,
    },
    /// Execute multiple steps (debug mode only)
    #[command(name = "run-steps")]
    RunSteps {
        /// Step IDs to execute (space-separated)
        step_ids: Vec<String>,
    },
    /// Execute all currently runnable steps (debug mode only)
    #[command(name = "run-all")]
    RunAll,
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
    /// Show completed steps and their results (debug mode only)
    #[command(name = "completed")]
    Completed,
    /// Get the output of a specific step (debug mode only)
    #[command(name = "output")]
    Output {
        /// Step ID to get output for
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
        ReplCommand::Runnable => handle_runnable_command(state).await,
        ReplCommand::RunStep { step_id } => handle_run_step_command(step_id, state).await,
        ReplCommand::RunSteps { step_ids } => handle_run_steps_command(step_ids, state).await,
        ReplCommand::RunAll => handle_run_all_command(state).await,
        ReplCommand::Continue => handle_continue_command(state).await,
        ReplCommand::Pause => handle_pause_command(state).await,
        ReplCommand::Inspect { step_id } => handle_inspect_command(step_id, state).await,
        ReplCommand::Completed => handle_completed_command(state).await,
        ReplCommand::Output { step_id } => handle_output_command(step_id, state).await,
    }
}

/// Handle the run command
async fn handle_run_command(
    workflow_path: PathBuf,
    input_args: InputArgs,
    debug: bool,
    state: &mut ReplState,
) -> Result<()> {
    // Load workflow
    let workflow: Arc<Flow> = load(&workflow_path)?;
    let workflow_hash = Flow::hash(workflow.as_ref());
    println!("Loaded workflow: {}", workflow_path.display());

    // Parse input with flow context
    let flow_dir = workflow_path.parent();
    let input_value = input_args.parse_input(flow_dir, true)?;

    // Create LastRun structure
    let mut last_run = LastRun::new(workflow, workflow_hash, workflow_path, input_value);
    state.debug_mode = debug;

    if debug {
        // Create debug execution
        last_run.create_debug_execution(&state.executor).await?;

        println!("Debug mode enabled. Workflow loaded but not started.");
        println!("Use 'steps' to see all steps, 'runnable' to see runnable steps.");
        println!("Use 'run-step <step_id>' to execute a specific step, or 'continue' to run all.");
    } else {
        // Execute workflow normally
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
        // Parse input with flow context from stored workflow path
        let flow_dir = last_run.workflow_path.parent();
        let input_value = input_args.parse_input(flow_dir, false)?;
        last_run.update_input(input_value);
    }

    state.debug_mode = debug;

    if debug {
        // Create debug execution
        last_run.create_debug_execution(&state.executor).await?;

        println!("Debug mode enabled. Workflow loaded but not started.");
        println!("Use 'steps' to see all steps, 'runnable' to see runnable steps.");
        println!("Use 'run-step <step_id>' to execute a specific step, or 'continue' to run all.");
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
        println!("  Workflow: {} steps", last_run.workflow.steps.len());
        if let Some(name) = &last_run.workflow.name {
            println!("    Name: {}", name);
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
        let workflow_json = serde_json::to_string_pretty(last_run.workflow.as_ref())
            .change_context(MainError::FlowExecution)?;
        println!("Current workflow:\n{}", workflow_json);
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
        println!("Current input:\n{}", input_json);
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
            // Lock the mutex to access the WorkflowExecutor
            let workflow_executor = debug_session.lock().await;
            
            let all_steps = workflow_executor.list_all_steps().await;
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

/// Handle the runnable command - show currently runnable steps
async fn handle_runnable_command(state: &ReplState) -> Result<()> {
    if !state.debug_mode {
        println!(
            "Runnable command is only available in debug mode. Use 'run --debug' to enable debug mode."
        );
        return Ok(());
    }

    if let Some(last_run) = &state.last_run {
        if let Some(debug_session) = &last_run.last_execution {
            // Lock the mutex to access the WorkflowExecutor
            let workflow_executor = debug_session.lock().await;
            
            let runnable_steps = workflow_executor.get_runnable_steps().await;
            if runnable_steps.is_empty() {
                println!(
                    "No steps are currently runnable. All dependencies may be satisfied or workflow is complete."
                );
            } else {
                println!("Currently runnable steps ({} total):", runnable_steps.len());
                for step_status in &runnable_steps {
                    println!(
                        "  [{}] {}: {}",
                        step_status.step_index, step_status.step_id, step_status.component
                    );
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

/// Handle the run-step command - execute a specific step
async fn handle_run_step_command(step_id: String, state: &mut ReplState) -> Result<()> {
    if !state.debug_mode {
        println!(
            "Run-step command is only available in debug mode. Use 'run --debug' to enable debug mode."
        );
        return Ok(());
    }

    if let Some(last_run) = &mut state.last_run {
        if let Some(debug_session) = &last_run.last_execution {
            // Lock the mutex to access the WorkflowExecutor
            let mut workflow_executor = debug_session.lock().await;
            
            match workflow_executor.execute_step_by_id(&step_id).await {
                Ok(result) => {
                    print_step_result(&step_id, &result.result)?;
                }
                Err(e) => {
                    println!("Failed to execute step '{}': {}", step_id, e);
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

/// Handle the run-steps command - execute multiple specific steps
async fn handle_run_steps_command(step_ids: Vec<String>, state: &mut ReplState) -> Result<()> {
    if !state.debug_mode {
        println!(
            "Run-steps command is only available in debug mode. Use 'run --debug' to enable debug mode."
        );
        return Ok(());
    }

    if let Some(last_run) = &mut state.last_run {
        if let Some(debug_session) = &last_run.last_execution {
            // Lock the mutex to access the WorkflowExecutor
            let mut workflow_executor = debug_session.lock().await;
            
            match workflow_executor.execute_steps(&step_ids).await {
                Ok(results) => {
                    println!("Executed {} steps:", results.len());
                    for result in results {
                        print_step_result(&result.metadata.step_id, &result.result)?;
                    }
                }
                Err(e) => {
                    println!("Failed to execute steps: {}", e);
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

/// Handle the run-all command - execute all currently runnable steps
async fn handle_run_all_command(state: &mut ReplState) -> Result<()> {
    if !state.debug_mode {
        println!(
            "Run-all command is only available in debug mode. Use 'run --debug' to enable debug mode."
        );
        return Ok(());
    }

    if let Some(last_run) = &mut state.last_run {
        if let Some(debug_session) = &last_run.last_execution {
            // Lock the mutex to access the WorkflowExecutor
            let mut workflow_executor = debug_session.lock().await;
            
            match workflow_executor.execute_all_runnable().await {
                Ok(results) => {
                    if results.is_empty() {
                        println!("No runnable steps to execute.");
                    } else {
                        println!("Executed {} runnable steps:", results.len());
                        for result in results {
                            print_step_result(&result.metadata.step_id, &result.result)?;
                        }
                    }
                }
                Err(e) => {
                    println!("Failed to execute runnable steps: {}", e);
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

/// Handle the continue command - continue workflow execution
async fn handle_continue_command(state: &mut ReplState) -> Result<()> {
    if !state.debug_mode {
        println!(
            "Continue command is only available in debug mode. Use 'run --debug' to enable debug mode."
        );
        return Ok(());
    }

    if let Some(last_run) = &mut state.last_run {
        if let Some(debug_session) = &last_run.last_execution {
            // Lock the mutex to access the WorkflowExecutor
            let mut workflow_executor = debug_session.lock().await;
            
            println!("Continuing workflow execution to completion...");
            match workflow_executor.execute_to_completion().await {
                Ok(final_result) => {
                    // Print final result
                    let result_json = serde_json::to_string_pretty(&final_result)
                        .change_context(MainError::FlowExecution)?;
                    println!("Workflow completed successfully.");
                    println!("Final result: {}", result_json);
                }
                Err(e) => {
                    println!("Workflow execution failed: {}", e);
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

/// Handle the pause command - pause workflow execution
async fn handle_pause_command(state: &ReplState) -> Result<()> {
    if !state.debug_mode {
        println!(
            "Pause command is only available in debug mode. Use 'run --debug' to enable debug mode."
        );
        return Ok(());
    }

    // TODO: Implement pause functionality
    println!("Pausing workflow execution (not yet implemented)");
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
            // Lock the mutex to access the WorkflowExecutor
            let workflow_executor = debug_session.lock().await;
            
            match workflow_executor.inspect_step(&step_id).await {
                Ok(inspection) => {
                    println!("Step '{}' inspection:", step_id);
                    println!("  Index: {}", inspection.metadata.step_index);
                    println!("  Component: {}", inspection.metadata.component);
                    println!("  State: {:?}", inspection.state);

                    let input_json = serde_json::to_string_pretty(&inspection.input)
                        .change_context(MainError::FlowExecution)?;
                    println!("  Input: {}", input_json);

                    if let Some(skip_if) = &inspection.skip_if {
                        let skip_json = serde_json::to_string_pretty(skip_if)
                            .change_context(MainError::FlowExecution)?;
                        println!("  Skip condition: {}", skip_json);
                    }

                    println!("  Error handling: {:?}", inspection.on_error);
                }
                Err(e) => {
                    println!("Failed to inspect step '{}': {}", step_id, e);
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
            // Lock the mutex to access the WorkflowExecutor
            let workflow_executor = debug_session.lock().await;
            
            match workflow_executor.get_completed_steps().await {
                Ok(completed_steps) => {
                    if completed_steps.is_empty() {
                        println!("No steps have been completed yet.");
                    } else {
                        println!("Completed steps ({} total):", completed_steps.len());
                        for step in &completed_steps {
                            let status = match &step.result {
                                stepflow_core::FlowResult::Success { .. } => "SUCCESS",
                                stepflow_core::FlowResult::Skipped => "SKIPPED",
                                stepflow_core::FlowResult::Failed { .. } => "FAILED",
                                stepflow_core::FlowResult::Streaming { .. } => "RUNNING",
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
                    println!("Failed to get completed steps: {}", e);
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

/// Handle the output command - get output of a specific step
async fn handle_output_command(step_id: String, state: &ReplState) -> Result<()> {
    if !state.debug_mode {
        println!(
            "Output command is only available in debug mode. Use 'run --debug' to enable debug mode."
        );
        return Ok(());
    }

    if let Some(last_run) = &state.last_run {
        if let Some(debug_session) = &last_run.last_execution {
            // Lock the mutex to access the WorkflowExecutor
            let workflow_executor = debug_session.lock().await;
            
            match workflow_executor.get_step_output(&step_id).await {
                Ok(result) => {
                    println!("Output of step '{}':", step_id);
                    print_flow_result(&result)?;
                }
                Err(e) => {
                    println!("Failed to get output for step '{}': {}", step_id, e);
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

/// Print the result of a step execution
fn print_step_result(step_id: &str, result: &stepflow_core::FlowResult) -> Result<()> {
    println!("Step '{}' executed successfully.", step_id);
    print_flow_result(result)
}

/// Print a FlowResult in a formatted way
fn print_flow_result(result: &stepflow_core::FlowResult) -> Result<()> {
    match result {
        stepflow_core::FlowResult::Success { result } => {
            let result_json = serde_json::to_string_pretty(result.as_ref())
                .change_context(MainError::FlowExecution)?;
            println!("Result: {}", result_json);
        }
        stepflow_core::FlowResult::Skipped => {
            println!("Result: SKIPPED");
        }
        stepflow_core::FlowResult::Failed { error } => {
            println!("Result: FAILED - {}", error);
        }
        stepflow_core::FlowResult::Streaming { stream_id, metadata, chunk, chunk_index, is_final } => {
            println!("Result: STREAMING");
            println!("  Stream ID: {}", stream_id);
            println!("  Metadata: {}", serde_json::to_string_pretty(metadata).unwrap_or_else(|_| "<unserializable>".to_string()));
            println!("  Chunk: {}... ({} bytes base64)", &chunk[..chunk.len().min(32)], chunk.len());
            println!("  Chunk Index: {}", chunk_index);
            println!("  Final: {}", is_final);
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
    println!("StepFlow REPL Commands:");
    println!();

    // Use clap's help generation
    let help = cmd.render_help();
    println!("{}", help);

    // Add some additional context
    println!();
    println!("Note: Debug commands (steps, runnable, run-step, etc.) are only available when");
    println!("a workflow is loaded in debug mode using 'run --workflow=<file> --debug'.");
}

/// Main REPL function
pub async fn run_repl(config_path: Option<PathBuf>) -> Result<()> {
    let mut rl = DefaultEditor::new().change_context(MainError::ReplInit)?;

    let mut state = ReplState::new(config_path).await?;

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
                    assert!(
                        debug_str.starts_with(expected_variant),
                        "Expected {} to parse as {} variant, got: {:?}",
                        input,
                        expected_variant,
                        command
                    );
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
            Ok(ReplCommand::Rerun { input_args, debug }) => {
                assert_eq!(input_args.input_json, Some("'{\"test\":123}'".to_string()));
                assert!(!debug);
            }
            _ => panic!("Expected rerun command, got: {:?}", result),
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
            _ => panic!("Expected run command, got: {:?}", result),
        }
    }
}
