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

use super::stepflow;
use insta_cmd::assert_cmd_snapshot;

/// Macro to apply common filters for visualize command tests
macro_rules! apply_visualize_filters {
    ($temp_dir:expr) => {
        let mut settings = insta::Settings::clone_current();
        // Replace the specific temp directory path with placeholder
        let temp_path = $temp_dir.to_string_lossy().to_string();
        let escaped_path = regex::escape(&temp_path);
        settings.add_filter(&escaped_path, "[TEMP_DIR]/");
        // Convert windows paths to Unix paths
        settings.add_filter(r"\\", "/");
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
            .arg("--flow=tests/mock/basic.yaml")
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
            .arg("--flow=tests/mock/basic.yaml")
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
            .arg("--flow=tests/mock/basic.yaml")
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
            .arg("--flow=tests/mock/basic.yaml")
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
            .arg("--flow=tests/mock/basic.yaml")
            .arg(format!("--output={}", output_file.display()))
            .arg("--format=dot")
            .arg("--no-details")
    );

    // Clean up the temporary file
    let _ = std::fs::remove_file(&output_file);
}

#[test]
fn test_visualize_conditional_skip_workflow() {
    // Test visualization with conditional skip workflow which has optional dependencies
    let temp_dir = std::env::temp_dir();
    apply_visualize_filters!(temp_dir);

    let output_file = temp_dir.join("stepflow_test_conditional_skip.dot");

    assert_cmd_snapshot!(
        stepflow()
            .arg("visualize")
            .arg("--flow=tests/mock/conditional_skip.yaml")
            .arg(format!("--output={}", output_file.display()))
            .arg("--format=dot")
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
            .arg("--flow=tests/builtins/blob_test.yaml")
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
    assert_cmd_snapshot!(
        stepflow()
            .arg("visualize")
            .arg("--flow=nonexistent.yaml")
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
            .arg("--flow=tests/mock/basic.yaml")
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
            .arg("--flow=tests/mock/basic.yaml")
            .arg(format!("--output={}", output_file.display()))
            .arg("--format=xyz")
    );

    // Note: No cleanup needed since invalid format means no file is created
}

#[test]
fn test_visualize_stdout_non_dot_format() {
    // Test error handling for non-DOT format with stdout
    assert_cmd_snapshot!(
        stepflow()
            .arg("visualize")
            .arg("--flow=tests/mock/basic.yaml")
            .arg("--format=svg")
    );
}

#[test]
fn test_visualize_conditional_skip_workflow_stdout() {
    // Test visualization with conditional skip workflow output to stdout
    assert_cmd_snapshot!(
        stepflow()
            .arg("visualize")
            .arg("--flow=tests/mock/conditional_skip.yaml")
            .arg("--format=dot")
    );
}

#[test]
fn test_visualize_stdout_with_options() {
    // Test stdout with various options
    assert_cmd_snapshot!(
        stepflow()
            .arg("visualize")
            .arg("--flow=tests/mock/basic.yaml")
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
            .arg("--flow=tests/mock/basic.yaml")
            .arg(format!("--output={}", output_file.display()))
            .arg("--format=dot")
            .arg("--no-servers")
            .arg("--no-details")
    );

    // Clean up the temporary file
    let _ = std::fs::remove_file(&output_file);
}
