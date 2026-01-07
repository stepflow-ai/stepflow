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

use super::stepflow;
use insta_cmd::assert_cmd_snapshot;

/// Get the project root path (3 parents up from CARGO_MANIFEST_DIR)
fn project_root() -> std::path::PathBuf {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("Failed to find project root")
        .to_path_buf()
}

/// Macro to apply project root filter for error message tests
macro_rules! apply_project_root_filter {
    () => {
        let mut settings = insta::Settings::clone_current();
        let project_path = project_root().to_string_lossy().to_string();
        let escaped_path = regex::escape(&project_path);
        settings.add_filter(&escaped_path, "[PROJECT]");
        // Convert windows paths to Unix paths
        if std::path::MAIN_SEPARATOR == '\\' {
            settings.add_filter(r"\\", "/");
        }
        let _bound = settings.bind_to_scope();
    };
}

/// Macro to apply common filters for visualize command tests
macro_rules! apply_visualize_filters {
    ($temp_dir:expr) => {
        let mut settings = insta::Settings::clone_current();
        // Replace the project root path with placeholder
        let project_path = project_root().to_string_lossy().to_string();
        let escaped_project = regex::escape(&project_path);
        settings.add_filter(&escaped_project, "[PROJECT]");
        // Replace the specific temp directory path with placeholder
        let mut temp_path = $temp_dir.to_string_lossy().to_string();
        if temp_path.ends_with(std::path::MAIN_SEPARATOR) {
            temp_path.pop();
        }
        let escaped_path = regex::escape(&temp_path);
        settings.add_filter(&escaped_path, "[TEMP_DIR]");
        // Convert windows paths to Unix paths
        if std::path::MAIN_SEPARATOR == '\\' {
            settings.add_filter(r"\\", "/");
        }
        let _bound = settings.bind_to_scope();
    };
}

#[test]
fn test_visualize_help() {
    assert_cmd_snapshot!(stepflow().arg("visualize").arg("--help"));
}

#[test]
fn test_visualize_basic_workflow_dot() {
    // Test basic visualization with DOT output format to temporary file
    let temp_dir = std::env::temp_dir();
    apply_visualize_filters!(temp_dir);

    let output_file = temp_dir.join("stepflow_test_basic_output.dot");

    assert_cmd_snapshot!(
        stepflow()
            .arg("visualize")
            .arg("tests/mock/basic.yaml")
            .arg(format!("--output={}", output_file.display()))
            .arg("--format=dot")
    );

    // Clean up the temporary file
    let _ = std::fs::remove_file(&output_file);
}

#[test]
fn test_visualize_basic_workflow_dot_stdout() {
    // Test basic visualization with DOT output to stdout
    assert_cmd_snapshot!(
        stepflow()
            .arg("visualize")
            .arg("tests/mock/basic.yaml")
            .arg("--format=dot")
    );
}

// Note: Removed test_visualize_basic_workflow_dot_content as it's redundant -
// the stdout tests now provide much better DOT content validation in snapshots

#[test]
fn test_visualize_basic_workflow_svg() {
    // Test SVG output - this should work if graphviz is available
    // If graphviz is not available, it will show an appropriate error
    let temp_dir = std::env::temp_dir();
    apply_visualize_filters!(temp_dir);

    let output_file = temp_dir.join("stepflow_test_basic_output.svg");

    assert_cmd_snapshot!(
        stepflow()
            .arg("visualize")
            .arg("tests/mock/basic.yaml")
            .arg(format!("--output={}", output_file.display()))
            .arg("--format=svg")
    );

    // Clean up the temporary file
    let _ = std::fs::remove_file(&output_file);
}

#[test]
fn test_visualize_with_no_servers_option() {
    // Test the --no-servers option that hides component server information
    let temp_dir = std::env::temp_dir();
    apply_visualize_filters!(temp_dir);

    let output_file = temp_dir.join("stepflow_test_basic_no_servers.dot");

    assert_cmd_snapshot!(
        stepflow()
            .arg("visualize")
            .arg("tests/mock/basic.yaml")
            .arg(format!("--output={}", output_file.display()))
            .arg("--format=dot")
            .arg("--no-servers")
    );

    // Clean up the temporary file
    let _ = std::fs::remove_file(&output_file);
}

#[test]
fn test_visualize_with_no_details_option() {
    // Test the --no-details option that hides tooltips and metadata
    let temp_dir = std::env::temp_dir();
    apply_visualize_filters!(temp_dir);

    let output_file = temp_dir.join("stepflow_test_basic_no_details.dot");

    assert_cmd_snapshot!(
        stepflow()
            .arg("visualize")
            .arg("tests/mock/basic.yaml")
            .arg(format!("--output={}", output_file.display()))
            .arg("--format=dot")
            .arg("--no-details")
    );

    // Clean up the temporary file
    let _ = std::fs::remove_file(&output_file);
}

#[test]
fn test_visualize_with_custom_config() {
    // Test visualization with custom config that defines different plugins
    let temp_dir = std::env::temp_dir();
    apply_visualize_filters!(temp_dir);

    let output_file = temp_dir.join("stepflow_test_blob_test.dot");

    assert_cmd_snapshot!(
        stepflow()
            .arg("visualize")
            .arg("tests/builtins/blob_test.yaml")
            .arg("--config=tests/builtins/stepflow-config.yml")
            .arg(format!("--output={}", output_file.display()))
            .arg("--format=dot")
    );

    // Clean up the temporary file
    let _ = std::fs::remove_file(&output_file);
}

#[test]
fn test_visualize_nonexistent_workflow() {
    // Test error handling for nonexistent workflow files
    apply_project_root_filter!();
    assert_cmd_snapshot!(
        stepflow()
            .arg("visualize")
            .arg("nonexistent.yaml")
            .arg("--output=output.dot")
            .arg("--format=dot")
    );
}

#[test]
fn test_visualize_png_format() {
    // Test PNG output format - this tests the format parsing
    // Will fail gracefully if graphviz is not available
    let temp_dir = std::env::temp_dir();
    apply_visualize_filters!(temp_dir);

    let output_file = temp_dir.join("stepflow_test_basic_output.png");

    assert_cmd_snapshot!(
        stepflow()
            .arg("visualize")
            .arg("tests/mock/basic.yaml")
            .arg(format!("--output={}", output_file.display()))
            .arg("--format=png")
    );

    // Clean up the temporary file
    let _ = std::fs::remove_file(&output_file);
}

#[test]
fn test_visualize_invalid_format() {
    // Test error handling for invalid output formats
    let temp_dir = std::env::temp_dir();
    apply_visualize_filters!(temp_dir);

    let output_file = temp_dir.join("stepflow_test_basic_output.xyz");

    assert_cmd_snapshot!(
        stepflow()
            .arg("visualize")
            .arg("tests/mock/basic.yaml")
            .arg(format!("--output={}", output_file.display()))
            .arg("--format=xyz")
    );

    // Note: No cleanup needed since invalid format means no file is created
}

#[test]
fn test_visualize_stdout_non_dot_format() {
    // Test that SVG format without --output creates file next to workflow
    let output_path = test_workflow_path("tests/mock/basic.svg");

    // Clean up any existing output from previous test runs
    let _ = std::fs::remove_file(&output_path);

    assert_cmd_snapshot!(
        stepflow()
            .arg("visualize")
            .arg("tests/mock/basic.yaml")
            .arg("--format=svg")
    );

    // Clean up the generated file
    let _ = std::fs::remove_file(&output_path);
}

#[test]
fn test_visualize_stdout_with_options() {
    // Test stdout with various options
    assert_cmd_snapshot!(
        stepflow()
            .arg("visualize")
            .arg("tests/mock/basic.yaml")
            .arg("--format=dot")
            .arg("--no-servers")
            .arg("--no-details")
    );
}

#[test]
fn test_visualize_all_options_combined() {
    // Test combining multiple options
    let temp_dir = std::env::temp_dir();
    apply_visualize_filters!(temp_dir);

    let output_file = temp_dir.join("stepflow_test_basic_combined.dot");

    assert_cmd_snapshot!(
        stepflow()
            .arg("visualize")
            .arg("tests/mock/basic.yaml")
            .arg(format!("--output={}", output_file.display()))
            .arg("--format=dot")
            .arg("--no-servers")
            .arg("--no-details")
    );

    // Clean up the temporary file
    let _ = std::fs::remove_file(&output_file);
}

// ============================================================================
// Path handling tests - verify files are created in the correct locations
// ============================================================================

/// Get the path to test files relative to the project root
fn test_workflow_path(relative: &str) -> std::path::PathBuf {
    // Project root is 3 parents up from CARGO_MANIFEST_DIR
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("Failed to find project root");
    project_root.join(relative)
}

/// Test that DOT output with explicit path creates file in correct location
#[test]
fn test_visualize_path_explicit_output_creates_file() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let output_file = temp_dir.path().join("output.dot");
    let workflow_path = test_workflow_path("tests/mock/basic.yaml");

    let status = stepflow()
        .arg("visualize")
        .arg(&workflow_path)
        .arg(format!("--output={}", output_file.display()))
        .arg("--format=dot")
        .status()
        .expect("Failed to run command");

    assert!(status.success(), "Command should succeed");
    assert!(
        output_file.exists(),
        "Output file should exist at: {}",
        output_file.display()
    );

    // Verify it's a valid DOT file
    let content = std::fs::read_to_string(&output_file).expect("Failed to read output");
    assert!(
        content.contains("digraph workflow"),
        "Output should be a valid DOT file"
    );
}

/// Test that inferred SVG output creates file next to workflow
#[test]
fn test_visualize_path_inferred_svg_creates_file() {
    // Skip if graphviz not installed
    #[allow(clippy::print_stdout)]
    if which::which("dot").is_err() {
        println!("Skipping test: graphviz not installed");
        return;
    }

    // Copy workflow to temp dir so we can test inferred output
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let workflow_path = temp_dir.path().join("test_workflow.yaml");
    let expected_output = temp_dir.path().join("test_workflow.svg");
    let source_workflow = test_workflow_path("tests/mock/basic.yaml");

    // Copy the workflow file
    std::fs::copy(&source_workflow, &workflow_path).expect("Failed to copy workflow");

    // Clean up any existing output file
    let _ = std::fs::remove_file(&expected_output);

    let status = stepflow()
        .arg("visualize")
        .arg(&workflow_path)
        .arg("--format=svg")
        .status()
        .expect("Failed to run command");

    assert!(status.success(), "Command should succeed");
    assert!(
        expected_output.exists(),
        "Inferred SVG output should exist at: {}",
        expected_output.display()
    );

    // Verify it's a valid SVG file
    let content = std::fs::read_to_string(&expected_output).expect("Failed to read output");
    assert!(
        content.contains("<svg") || content.contains("<?xml"),
        "Output should be a valid SVG file"
    );
}

/// Test that running from subdirectory with relative path works
#[test]
fn test_visualize_path_relative_workflow_from_subdir() {
    // Skip if graphviz not installed
    #[allow(clippy::print_stdout)]
    if which::which("dot").is_err() {
        println!("Skipping test: graphviz not installed");
        return;
    }

    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let source_workflow = test_workflow_path("tests/mock/basic.yaml");

    // Create a subdirectory structure: temp/workflows/flow.yaml
    let workflows_dir = temp_dir.path().join("workflows");
    std::fs::create_dir_all(&workflows_dir).expect("Failed to create workflows dir");

    let workflow_path = workflows_dir.join("flow.yaml");
    std::fs::copy(&source_workflow, &workflow_path).expect("Failed to copy workflow");

    // Expected output next to workflow
    let expected_output = workflows_dir.join("flow.svg");
    let _ = std::fs::remove_file(&expected_output);

    // Run from temp_dir with relative path to workflow
    let status = stepflow()
        .current_dir(temp_dir.path())
        .arg("visualize")
        .arg("workflows/flow.yaml")
        .arg("--format=svg")
        .status()
        .expect("Failed to run command");

    assert!(status.success(), "Command should succeed");
    assert!(
        expected_output.exists(),
        "Output should be created next to workflow at: {}",
        expected_output.display()
    );
}

/// Test that explicit relative output path is relative to CWD, not workflow
#[test]
fn test_visualize_path_explicit_output_relative_to_cwd() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let source_workflow = test_workflow_path("tests/mock/basic.yaml");

    // Create subdirectory for workflow
    let workflows_dir = temp_dir.path().join("workflows");
    std::fs::create_dir_all(&workflows_dir).expect("Failed to create workflows dir");

    let workflow_path = workflows_dir.join("flow.yaml");
    std::fs::copy(&source_workflow, &workflow_path).expect("Failed to copy workflow");

    // Output should be in CWD (temp_dir), not next to workflow
    let expected_output = temp_dir.path().join("my_output.dot");
    let _ = std::fs::remove_file(&expected_output);

    // Run from temp_dir with explicit relative output
    let status = stepflow()
        .current_dir(temp_dir.path())
        .arg("visualize")
        .arg("workflows/flow.yaml")
        .arg("--output=my_output.dot")
        .arg("--format=dot")
        .status()
        .expect("Failed to run command");

    assert!(status.success(), "Command should succeed");
    assert!(
        expected_output.exists(),
        "Output should be in CWD at: {}",
        expected_output.display()
    );

    // Verify it should NOT be next to the workflow
    let wrong_location = workflows_dir.join("my_output.dot");
    assert!(
        !wrong_location.exists(),
        "Output should NOT be next to workflow"
    );
}

/// Test that existing file blocks inferred output
#[test]
fn test_visualize_path_inferred_output_blocked_by_existing() {
    // Skip if graphviz not installed
    #[allow(clippy::print_stdout)]
    if which::which("dot").is_err() {
        println!("Skipping test: graphviz not installed");
        return;
    }

    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let source_workflow = test_workflow_path("tests/mock/basic.yaml");
    let workflow_path = temp_dir.path().join("flow.yaml");
    let existing_output = temp_dir.path().join("flow.svg");

    std::fs::copy(&source_workflow, &workflow_path).expect("Failed to copy workflow");

    // Create an existing output file
    std::fs::write(&existing_output, "existing content").expect("Failed to create existing file");

    let output = stepflow()
        .current_dir(temp_dir.path())
        .arg("visualize")
        .arg("flow.yaml")
        .arg("--format=svg")
        .output()
        .expect("Failed to run command");

    assert!(
        !output.status.success(),
        "Command should fail when output exists"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("already exists"),
        "Error should mention file exists: {}",
        stderr
    );

    // Verify original file wasn't overwritten
    let content = std::fs::read_to_string(&existing_output).expect("Failed to read file");
    assert_eq!(
        content, "existing content",
        "Existing file should not be modified"
    );
}
