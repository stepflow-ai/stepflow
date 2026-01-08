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

#![allow(clippy::print_stdout)]

use crate::args::{ConfigArgs, OutputArgs, WorkflowLoader, load};
use crate::test_server::TestServerManager;
use crate::{MainError, Result};
use clap::Args;
use error_stack::ResultExt as _;
use similar::{ChangeTag, TextDiff};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use stepflow_config::StepflowConfig;
use stepflow_core::workflow::Flow;
use stepflow_core::{BlobId, FlowError, FlowResult};
use walkdir::WalkDir;

/// Normalize run_id fields in FlowResult for consistent testing
fn normalize_flow_result(result: FlowResult) -> FlowResult {
    match result {
        FlowResult::Success(result) => {
            let normalized_value = normalize_json_value(result.as_ref().clone());
            FlowResult::Success(normalized_value.into())
        }
        FlowResult::Failed(error) => FlowResult::Failed(FlowError {
            code: error.code,
            message: error.message,
            data: error
                .data
                .map(|data| normalize_json_value(data.clone_value()).into()),
        }),
    }
}

/// Recursively normalize run_id fields and strip backtraces in JSON values
fn normalize_json_value(mut value: serde_json::Value) -> serde_json::Value {
    use serde_json::{Map, Value};

    match &mut value {
        Value::Object(map) => {
            let mut sorted = Map::new();
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();

            for key in keys {
                let mut val = map.remove(&key).unwrap();

                if key == "run_id" && val.is_string() {
                    // Normalize run_id fields to a fixed value for testing
                    val = Value::String("00000000-0000-0000-0000-000000000000".to_string());
                } else if key == "backtrace" {
                    // Remove backtrace field entirely to avoid CI environment differences
                    //
                    // We could make this a bit stricter by only doing it to the top level
                    // `error.data.stack.*.backtrace` fields.
                    continue;
                } else {
                    // Recursively normalize field values.
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

/// Check if actual FlowResult matches expected FlowResult using partial matching.
/// Only fields present in `expected` are checked against `actual`.
fn partial_match(expected: &FlowResult, actual: &FlowResult) -> bool {
    match (expected, actual) {
        (FlowResult::Success(exp), FlowResult::Success(act)) => {
            partial_match_json(exp.as_ref(), act.as_ref())
        }
        (FlowResult::Failed(exp), FlowResult::Failed(act)) => {
            // For errors, check code, message, and data if specified
            if exp.code != act.code || exp.message != act.message {
                return false;
            }
            // Only check data if expected has data specified
            if let Some(exp_data) = &exp.data {
                if let Some(act_data) = &act.data {
                    if !partial_match_json(exp_data.as_ref(), act_data.as_ref()) {
                        return false;
                    }
                } else {
                    return false; // Expected data but actual has none
                }
            }
            true
        }
        _ => false, // Different variants don't match
    }
}

/// Check if actual JSON value contains all fields from expected JSON value.
/// Extra fields in actual are allowed.
fn partial_match_json(expected: &serde_json::Value, actual: &serde_json::Value) -> bool {
    use serde_json::Value;

    match (expected, actual) {
        (Value::Object(exp_map), Value::Object(act_map)) => {
            // All keys in expected must exist in actual with matching values
            for (key, exp_val) in exp_map {
                match act_map.get(key) {
                    Some(act_val) => {
                        if !partial_match_json(exp_val, act_val) {
                            return false;
                        }
                    }
                    None => return false, // Expected key not found in actual
                }
            }
            true
        }
        (Value::Array(exp_arr), Value::Array(act_arr)) => {
            // Arrays must have same length and matching elements
            if exp_arr.len() != act_arr.len() {
                return false;
            }
            for (exp_val, act_val) in exp_arr.iter().zip(act_arr.iter()) {
                if !partial_match_json(exp_val, act_val) {
                    return false;
                }
            }
            true
        }
        // For primitives, do exact match
        (exp, act) => exp == act,
    }
}

/// Show a diff between expected and actual outputs using the similar crate
fn show_diff(expected: &FlowResult, actual: &FlowResult, test_name: &str) {
    let expected_yaml = serde_yaml_ng::to_string(expected)
        .unwrap_or_else(|_| "Error serializing expected".to_string());
    let actual_yaml =
        serde_yaml_ng::to_string(actual).unwrap_or_else(|_| "Error serializing actual".to_string());

    let diff = TextDiff::from_lines(&expected_yaml, &actual_yaml);

    println!("Test Case {test_name} failed:");
    println!("--- Expected");
    println!("+++ Actual");

    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => "-",
            ChangeTag::Insert => "+",
            ChangeTag::Equal => " ",
        };
        print!("{sign}{change}");
    }
    println!();
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
    server_manager: Option<&TestServerManager>,
) -> Result<StepflowConfig> {
    // If explicit config provided, use that
    if let Some(config_path) = config_path {
        let config_args = ConfigArgs::with_path(Some(config_path));
        return config_args.load_config(None);
    }

    // 1. Check config in test section of this workflow (with server substitution)
    if let Some(test_config) = workflow.test()
        && let Some(config_value) = &test_config.config
    {
        return parse_stepflow_config_from_value(config_value, flow_path, server_manager);
    }

    // TODO: 2. Check stepflow_config in test section of enclosing workflow (if any)
    // This would require parsing parent flows, which is complex

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
            let config_args = ConfigArgs::with_path(Some(test_config_path));
            return config_args.load_config(None);
        }
    }

    // 4. Look for stepflow-config.yml in workflow directory
    // 5. Look for stepflow-config.yml in current directory
    // Reuse existing config resolution logic
    let config_args = ConfigArgs::with_path(None);
    let flow_dir = flow_path.parent();
    config_args.load_config(flow_dir)
}

fn parse_stepflow_config_from_value(
    value: &serde_json::Value,
    flow_path: &Path,
    server_manager: Option<&TestServerManager>,
) -> Result<StepflowConfig> {
    // Apply server URL substitution if server manager is available
    let config_str = if let Some(manager) = server_manager {
        manager.create_substituted_config(value)?
    } else {
        serde_json::to_string(value)
            .change_context(MainError::Configuration)
            .attach_printable("Failed to serialize config")?
    };

    let mut config: StepflowConfig = serde_json::from_str(&config_str)
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
    for test_case in flow.test_mut().unwrap().cases.iter_mut() {
        if let Some(new_output) = updates.remove(&test_case.name) {
            test_case.output = Some(new_output);
        }
    }
    let output_args = OutputArgs::with_path(Some(flow_path.to_owned()));
    output_args.write_output(flow)
}

/// Discover yaml files recursively in a directory.
fn discover_yaml_files(workflow_files: &mut HashSet<PathBuf>, dir: &Path) -> Result<()> {
    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file()
            && let Some(filename) = path.file_name().and_then(|n| n.to_str())
        {
            // Check if it's a YAML file
            if filename.ends_with(".yaml") || filename.ends_with(".yml") {
                workflow_files.insert(path.to_owned());
            }
        }
    }

    Ok(())
}

#[derive(Debug)]
pub enum FileStatus {
    NoTests,
    Errored(String),
    Run,
}

/// Result of running tests on a single workflow.
#[derive(Debug)]
struct WorkflowTestResult {
    file_status: FileStatus,
    workflow_path: PathBuf,
    total_cases: usize,
    passed_cases: usize,
    failed_cases: usize,
    execution_errors: usize,
    updates: HashMap<String, FlowResult>,
}

/// Main entry point for running tests on files or directories.
///
/// Return number of failures.
pub async fn run_tests(
    paths: &[PathBuf],
    config_path: Option<PathBuf>,
    options: TestOptions,
) -> Result<usize> {
    let mut workflow_files = HashSet::new();

    for path in paths {
        if path.is_file() {
            workflow_files.insert(path.to_owned());
        } else if path.is_dir() {
            discover_yaml_files(&mut workflow_files, path)?;
        } else {
            return Err(MainError::MissingFile(path.to_owned()).into());
        }
    }

    if workflow_files.is_empty() {
        println!("No workflow files found");
        return Ok(0);
    }

    let mut flow_files: Vec<_> = workflow_files.into_iter().collect();
    flow_files.sort_unstable();

    // Run tests on all flows and collect results
    let mut results: Vec<WorkflowTestResult> = Vec::new();

    for flow_path in flow_files {
        let result = run_single_flow_test(&flow_path, config_path.clone(), &options).await;

        match result {
            Ok(Some(result)) => {
                // Print immediate feedback
                println!(
                    "{}: {}/{} passed",
                    flow_path.display(),
                    result.passed_cases,
                    result.total_cases
                );
                results.push(result);
            }
            Ok(None) => {
                // Skipped file (not a flow file)
                println!("{}: Skipped (invalid or non-flow file)", flow_path.display());
            }
            Err(e) => {
                // Error loading or running tests
                println!("{}: Error - {e:#}", flow_path.display());
                results.push(WorkflowTestResult {
                    file_status: FileStatus::Errored(format!("{e:#}")),
                    workflow_path: flow_path.clone(),
                    total_cases: 0,
                    passed_cases: 0,
                    failed_cases: 0,
                    execution_errors: 0,
                    updates: HashMap::new(),
                });
            }
        }
    }

    // Apply all updates at the end if requested
    if options.update {
        apply_all_updates(&results).await?;
    }

    // Print summary return true if there were failures.
    print_summary(&results)
}

fn load_flow(path: &Path) -> Result<Option<Arc<Flow>>> {
    let rdr = File::open(path).change_context_lazy(|| MainError::MissingFile(path.to_owned()))?;

    // Try to parse as YAML, but if it fails, treat it as a non-flow file
    let value: serde_yaml_ng::Value = match serde_yaml_ng::from_reader(rdr) {
        Ok(v) => v,
        Err(_) => {
            // Invalid YAML or parsing error - skip this file
            return Ok(None);
        }
    };

    let Some(object) = value.as_mapping() else {
        return Ok(None);
    };

    match object.get("schema").and_then(|s| s.as_str()) {
        Some(schema) if Flow::supported_schema(schema) => {
            // Try to deserialize as Flow, but if it fails with the right schema, that's an error
            let flow: Flow = serde_yaml_ng::from_value(value)
                .change_context_lazy(|| MainError::InvalidFile(path.to_owned()))?;
            Ok(Some(Arc::new(flow)))
        }
        _ => {
            // No schema field or unsupported schema - skip this file
            Ok(None)
        }
    }
}

/// Run tests on a single workflow file.
async fn run_single_flow_test(
    flow_path: &Path,
    config_path: Option<PathBuf>,
    options: &TestOptions,
) -> Result<Option<WorkflowTestResult>> {
    // Try to load workflow
    let Some(flow) = load_flow(flow_path)? else {
        return Ok(None);
    };

    // Check if it has test cases and extract them
    let test_cases = if let Some(config) = flow.test() {
        config.cases.as_slice()
    } else {
        // No test cases, skip.
        return Ok(Some(WorkflowTestResult {
            file_status: FileStatus::NoTests,
            workflow_path: flow_path.to_owned(),
            total_cases: 0,
            passed_cases: 0,
            failed_cases: 0,
            execution_errors: 0,
            updates: HashMap::new(),
        }));
    };

    // Initialize server manager and start any test servers
    let mut server_manager = TestServerManager::new();
    if let Some(test_config) = flow.test()
        && !test_config.servers.is_empty()
    {
        println!("Starting {} test servers...", test_config.servers.len());
        let working_dir = flow_path.parent().unwrap_or_else(|| Path::new("."));
        server_manager
            .start_servers(&test_config.servers, working_dir)
            .await?;
    }

    // Set up executor with server-aware config
    let config = load_test_config(flow_path, config_path, &flow, Some(&server_manager))?;
    let executor = WorkflowLoader::create_executor_from_config(config).await?;

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
        // No matching cases
        return Ok(Some(WorkflowTestResult {
            file_status: FileStatus::NoTests,
            workflow_path: flow_path.to_owned(),
            total_cases: 0,
            passed_cases: 0,
            failed_cases: 0,
            execution_errors: 0,
            updates: HashMap::new(),
        }));
    }

    // Run the tests
    let mut updates = HashMap::new();
    let mut execution_errors = 0;

    let flow_id = BlobId::from_flow(&flow).change_context(MainError::Configuration)?;
    for test_case in &cases_to_run {
        println!("----------\nRunning Test Case {}", test_case.name);
        let result = crate::run::run(
            executor.clone(),
            flow.clone(),
            flow_id.clone(),
            test_case.input.clone(),
            None, // No overrides in test execution
        )
        .await;

        match result {
            Ok((_run_id, actual_output)) => {
                let normalized_output = normalize_flow_result(actual_output);
                match &test_case.output {
                    Some(expected_output) => {
                        let normalized_expected = normalize_flow_result(expected_output.clone());
                        // Use partial matching - expected fields must match, extra fields in actual are OK
                        if !partial_match(&normalized_expected, &normalized_output) {
                            if options.diff {
                                show_diff(
                                    &normalized_expected,
                                    &normalized_output,
                                    &test_case.name,
                                );
                            } else {
                                println!(
                                    "Test Case {} failed. New Output:\n{}\n",
                                    test_case.name,
                                    serde_yaml_ng::to_string(&normalized_output).unwrap()
                                );
                            }
                            updates.insert(test_case.name.clone(), normalized_output);
                        }
                    }
                    None => {
                        println!(
                            "Test Case {} output:\n{}\n",
                            test_case.name,
                            serde_yaml_ng::to_string(&normalized_output).unwrap()
                        );
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

    // Clean up test servers
    server_manager.stop_all_servers().await?;

    Ok(Some(WorkflowTestResult {
        file_status: FileStatus::Run,
        workflow_path: flow_path.to_owned(),
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
///
/// Return true if there were any errors.
fn print_summary(results: &[WorkflowTestResult]) -> Result<usize> {
    let mut files_error = 0;
    let mut files_skip = 0;

    let mut cases_total = 0;
    let mut cases_pass = 0;
    let mut cases_fail = 0;
    let mut cases_error = 0;

    for result in results {
        match &result.file_status {
            FileStatus::Errored(e) => {
                println!("{}: Error {}", result.workflow_path.display(), e);
                files_error += 1;
            }
            FileStatus::NoTests => {
                println!(
                    "{}: Skipped (no test cases)",
                    result.workflow_path.display()
                );
                files_skip += 1;
            }
            FileStatus::Run => {
                if result.execution_errors > 0 {
                    println!(
                        "{}: {}/{} passed ({} errors)",
                        result.workflow_path.display(),
                        result.passed_cases,
                        result.total_cases,
                        result.execution_errors
                    );
                } else {
                    println!(
                        "{}: {}/{} passed",
                        result.workflow_path.display(),
                        result.passed_cases,
                        result.total_cases
                    );
                }
                cases_total += result.total_cases;
                cases_pass += result.passed_cases;
                cases_fail += result.failed_cases;
                cases_error += result.execution_errors;
            }
        }
    }

    println!("\n=== Test Summary ===");
    println!("Files tested: {}", results.len());
    if files_skip > 0 {
        println!("Files skipped: {files_skip}");
    }
    if files_error > 0 {
        println!("Files errored: {files_error}");
    }
    println!("Total test cases: {cases_total}");
    println!("Passed: {cases_pass}");
    println!("Failed: {cases_fail}");
    if cases_error > 0 {
        println!("Execution errors: {cases_error}");
    }

    Ok(cases_fail + cases_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_normalize_strips_backtrace_fields() {
        // Create a JSON structure with backtrace fields
        let input = json!({
            "outcome": "failed",
            "error": {
                "code": 500,
                "message": "Test error",
                "data": {
                    "stack": [
                        {
                            "error": "Test error",
                            "attachments": [],
                            "backtrace": "   0: rust_begin_unwind\n           at /rustc/.../std/panicking.rs:584:5\n   1: core::panic::panic_fmt\n           at /rustc/.../core/panic.rs:142:14"
                        }
                    ]
                }
            }
        });

        let normalized = normalize_json_value(input);

        // Verify that backtrace field is stripped
        let error = &normalized["error"];
        let stack = &error["data"]["stack"][0];

        assert!(
            stack.get("backtrace").is_none(),
            "Backtrace field should be stripped"
        );
        assert_eq!(stack["error"], "Test error");
        assert_eq!(stack["attachments"], json!([]));
    }

    #[test]
    fn test_normalize_preserves_non_backtrace_fields() {
        let input = json!({
            "outcome": "success",
            "result": {
                "value": 42,
                "run_id": "actual-run-id-123",
                "metadata": {
                    "timestamp": "2023-01-01T00:00:00Z",
                    "some_backtrace_info": "should be preserved"
                }
            }
        });

        let normalized = normalize_json_value(input);

        // Verify run_id is normalized
        assert_eq!(
            normalized["result"]["run_id"],
            "00000000-0000-0000-0000-000000000000"
        );

        // Verify other fields are preserved
        assert_eq!(normalized["result"]["value"], 42);
        assert_eq!(
            normalized["result"]["metadata"]["timestamp"],
            "2023-01-01T00:00:00Z"
        );
        assert_eq!(
            normalized["result"]["metadata"]["some_backtrace_info"],
            "should be preserved"
        );
    }
}
