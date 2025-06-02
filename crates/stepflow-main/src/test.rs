#![allow(clippy::print_stdout)]
use crate::cli::{load, load_config, write_output};
use crate::{MainError, Result, stepflow_config::StepflowConfig};
use clap::Args;
use error_stack::ResultExt as _;
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use stepflow_core::FlowResult;
use stepflow_core::workflow::Flow;
use stepflow_execution::StepFlowExecutor;
use walkdir::WalkDir;

/// Normalize execution_id fields in FlowResult for consistent testing
fn normalize_flow_result(result: FlowResult) -> FlowResult {
    match result {
        FlowResult::Success { result } => {
            let normalized_value = normalize_json_value(result.as_ref().clone());
            FlowResult::Success {
                result: normalized_value.into(),
            }
        }
        other => other,
    }
}

/// Recursively normalize execution_id fields in JSON values
fn normalize_json_value(mut value: serde_json::Value) -> serde_json::Value {
    use serde_json::{Map, Value};

    match &mut value {
        Value::Object(map) => {
            let mut sorted = Map::new();
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();

            for key in keys {
                let mut val = map.remove(&key).unwrap();

                // Normalize execution_id fields to a fixed value for testing
                if key == "execution_id" && val.is_string() {
                    val = Value::String("00000000-0000-0000-0000-000000000000".to_string());
                } else {
                    val = normalize_json_value(val);
                }

                sorted.insert(key, val);
            }

            Value::Object(sorted)
        }
        Value::Array(arr) => {
            // Recursively normalize array elements
            Value::Array(arr.drain(..).map(normalize_json_value).collect())
        }
        _ => value,
    }
}

/// Test-specific options for running workflow tests.
#[derive(Debug, Clone, Args)]
pub struct TestOptions {
    /// Run only specific test case(s) by name. Can be repeated.
    #[arg(long = "case", value_name = "NAME")]
    pub cases: Vec<String>,

    /// Update expected outputs with actual outputs from test runs.
    #[arg(long)]
    pub update: bool,

    /// Show diff when tests fail.
    #[arg(long)]
    pub diff: bool,
}

/// Load stepflow config for tests using hierarchical resolution.
pub fn load_test_config(
    flow_path: &Path,
    config_path: Option<PathBuf>,
    workflow: &Flow,
) -> Result<StepflowConfig> {
    // If explicit config provided, use that
    if let Some(config_path) = config_path {
        return load_config(None, Some(config_path));
    }

    // 1. Check stepflow_config in test section of this workflow
    if let Some(test_config) = &workflow.test {
        if let Some(stepflow_config) = &test_config.stepflow_config {
            return parse_stepflow_config_from_value(stepflow_config, flow_path);
        }
    }

    // TODO: 2. Check stepflow_config in test section of enclosing workflow (if any)
    // This would require parsing parent workflows, which is complex

    // 3. Look for stepflow-config.test.yml in workflow directory
    let flow_dir = flow_path.parent().expect("flow_path should have a parent");
    let test_config_candidates = [
        "stepflow-config.test.yml",
        "stepflow-config.test.yaml",
        "stepflow_config.test.yml",
        "stepflow_config.test.yaml",
    ];

    for candidate in &test_config_candidates {
        let test_config_path = flow_dir.join(candidate);
        if test_config_path.is_file() {
            return load_config(None, Some(test_config_path));
        }
    }

    // 4. Look for stepflow-config.yml in workflow directory
    // 5. Look for stepflow-config.yml in current directory
    // Reuse existing config resolution logic
    crate::cli::load_config(Some(flow_path), None)
}

fn parse_stepflow_config_from_value(
    value: &serde_json::Value,
    flow_path: &Path,
) -> Result<StepflowConfig> {
    let mut config: StepflowConfig = serde_json::from_value(value.clone())
        .change_context_lazy(|| MainError::InvalidFile(flow_path.to_owned()))?;

    if config.working_directory.is_none() {
        let flow_dir = flow_path
            .parent()
            .expect("flow_path should have a parent directory");
        config.working_directory = Some(flow_dir.to_owned());
    }
    Ok(config)
}

/// Apply updates to the workflow file by updating test case outputs.
async fn apply_updates(flow_path: &Path, mut updates: HashMap<String, FlowResult>) -> Result<()> {
    // Read the original workflow file (easiest way to get an owned copy).
    let mut flow: Flow = load(flow_path)?;

    // Apply updates to the test cases using HashMap lookup
    for test_case in flow.test.as_mut().unwrap().cases.iter_mut() {
        if let Some(new_output) = updates.remove(&test_case.name) {
            test_case.output = Some(new_output);
        }
    }
    write_output(Some(flow_path.to_owned()), flow)
}

/// Discover workflow files recursively in a directory.
fn discover_workflow_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut workflows = Vec::new();
    let pattern = Regex::new(r"^stepflow[-_]config\.ya?ml$").unwrap();

    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() {
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                // Check if it's a YAML file but not a config file
                if (filename.ends_with(".yaml") || filename.ends_with(".yml"))
                    && !pattern.is_match(filename)
                {
                    workflows.push(path.to_owned());
                }
            }
        }
    }

    workflows.sort(); // For consistent ordering
    Ok(workflows)
}

/// Result of running tests on a single workflow.
#[derive(Debug)]
struct WorkflowTestResult {
    workflow_path: PathBuf,
    total_cases: usize,
    passed_cases: usize,
    failed_cases: usize,
    execution_errors: usize,
    updates: HashMap<String, FlowResult>,
}

/// Main entry point for running tests on a file or directory.
pub async fn run_tests(
    path: &Path,
    config_path: Option<PathBuf>,
    options: TestOptions,
) -> Result<()> {
    let workflow_files = if path.is_file() {
        vec![path.to_owned()]
    } else if path.is_dir() {
        discover_workflow_files(path)?
    } else {
        return Err(MainError::MissingFile(path.to_owned()).into());
    };

    if workflow_files.is_empty() {
        println!("No workflow files found");
        return Ok(());
    }

    // Run tests on all workflows and collect results
    let mut results = Vec::new();
    let mut total_files_skipped = 0;

    for workflow_path in workflow_files {
        match run_single_workflow_test(&workflow_path, config_path.clone(), &options).await {
            Ok(Some(result)) => {
                // Print immediate feedback
                if path.is_dir() {
                    println!(
                        "{}: {}/{} passed",
                        workflow_path.display(),
                        result.passed_cases,
                        result.total_cases
                    );
                }
                results.push(result);
            }
            Ok(None) => {
                // Skipped workflow
                if path.is_dir() {
                    println!("{}: Skipped (no test cases)", workflow_path.display());
                }
                total_files_skipped += 1;
            }
            Err(e) => {
                if path.is_dir() {
                    println!("{}: Skipped (error: {})", workflow_path.display(), e);
                } else {
                    return Err(e);
                }
                total_files_skipped += 1;
            }
        }
    }

    // Apply all updates at the end if requested
    if options.update {
        apply_all_updates(&results).await?;
    }

    // Print summary and exit with appropriate code
    print_summary_and_exit(&results, total_files_skipped, path.is_dir())?;

    Ok(())
}

/// Run tests on a single workflow file.
async fn run_single_workflow_test(
    workflow_path: &Path,
    config_path: Option<PathBuf>,
    options: &TestOptions,
) -> Result<Option<WorkflowTestResult>> {
    // Try to load workflow
    let flow: Arc<Flow> = load(workflow_path)?;

    // Check if it has test cases and extract them
    let test_cases = if let Some(config) = flow.test.as_ref() {
        config.cases.as_slice()
    } else {
        return Ok(None); // No test cases, skip.
    };

    // Set up executor
    let config = load_test_config(workflow_path, config_path, &flow)?;
    let plugins = config.create_plugins().await?;
    let executor = StepFlowExecutor::new(plugins);

    // Filter test cases if specific cases requested
    let cases_to_run: Vec<_> = if options.cases.is_empty() {
        test_cases.iter().collect()
    } else {
        test_cases
            .iter()
            .filter(|case| options.cases.contains(&case.name))
            .collect()
    };

    if cases_to_run.is_empty() {
        return Ok(None); // No matching cases
    }

    // Run the tests
    let mut updates = HashMap::new();
    let mut execution_errors = 0;

    for test_case in &cases_to_run {
        let result = crate::run::run(executor.clone(), flow.clone(), test_case.input.clone()).await;

        match result {
            Ok(actual_output) => {
                let normalized_output = normalize_flow_result(actual_output);
                match &test_case.output {
                    Some(expected_output) => {
                        let normalized_expected = normalize_flow_result(expected_output.clone());
                        if normalized_output != normalized_expected {
                            updates.insert(test_case.name.clone(), normalized_output);
                        }
                    }
                    None => {
                        updates.insert(test_case.name.clone(), normalized_output);
                    }
                }
            }
            Err(_) => {
                execution_errors += 1;
            }
        }
    }

    let total_cases = cases_to_run.len();
    let failed_cases = updates.len() + execution_errors;
    let passed_cases = total_cases - failed_cases;

    Ok(Some(WorkflowTestResult {
        workflow_path: workflow_path.to_owned(),
        total_cases,
        passed_cases,
        failed_cases,
        execution_errors,
        updates,
    }))
}

/// Apply updates to all workflow files that have them.
async fn apply_all_updates(results: &[WorkflowTestResult]) -> Result<()> {
    let mut total_updates = 0;

    for result in results {
        if !result.updates.is_empty() {
            apply_updates(&result.workflow_path, result.updates.clone()).await?;
            total_updates += result.updates.len();
        }
    }

    if total_updates > 0 {
        println!(
            "\nApplied {} output update(s) across {} file(s)",
            total_updates,
            results.iter().filter(|r| !r.updates.is_empty()).count()
        );
    }

    Ok(())
}

/// Print summary and exit with appropriate code.
fn print_summary_and_exit(
    results: &[WorkflowTestResult],
    files_skipped: usize,
    is_directory_mode: bool,
) -> Result<()> {
    if is_directory_mode && results.len() > 1 {
        // Print summary for directory mode
        let total_cases: usize = results.iter().map(|r| r.total_cases).sum();
        let total_passed: usize = results.iter().map(|r| r.passed_cases).sum();
        let total_failed: usize = results.iter().map(|r| r.failed_cases).sum();
        let total_exec_errors: usize = results.iter().map(|r| r.execution_errors).sum();

        println!("\n=== Test Summary ===");
        println!("Files tested: {}", results.len());
        if files_skipped > 0 {
            println!("Files skipped: {}", files_skipped);
        }
        println!("Total test cases: {}", total_cases);
        println!("Passed: {}", total_passed);
        println!("Failed: {}", total_failed);
        if total_exec_errors > 0 {
            println!("Execution errors: {}", total_exec_errors);
        }
    } else if let Some(result) = results.first() {
        // Single file mode - print detailed results
        println!("\nTest Results:");
        println!("  Passed: {}", result.passed_cases);
        println!("  Failed: {}", result.failed_cases);
        if result.execution_errors > 0 {
            println!("  Execution errors: {}", result.execution_errors);
        }
        println!("  Total: {}", result.total_cases);
    }

    // Exit with error code if any tests failed
    let any_failures = results
        .iter()
        .any(|r| r.failed_cases > 0 || r.execution_errors > 0);
    if any_failures {
        std::process::exit(1);
    }

    Ok(())
}
