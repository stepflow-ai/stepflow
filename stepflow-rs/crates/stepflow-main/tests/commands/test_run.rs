use super::stepflow;
use insta_cmd::assert_cmd_snapshot;

#[test]
fn test_run_help() {
    assert_cmd_snapshot!(stepflow().arg("run").arg("--help"));
}

#[test]
fn test_run_basic_workflow_with_file_input() {
    assert_cmd_snapshot!(
        stepflow()
            .arg("run")
            .arg("--flow=tests/mock/basic.yaml")
            .arg("--input=../../../../examples/eval/input.json")
    );
}

#[test]
fn test_run_basic_workflow_with_json_input() {
    assert_cmd_snapshot!(
        stepflow()
            .arg("run")
            .arg("--flow=tests/mock/basic.yaml")
            .arg(r#"--input-json={"m": 3, "n": 4}"#)
    );
}

#[test]
fn test_run_basic_workflow_with_yaml_input() {
    assert_cmd_snapshot!(
        stepflow()
            .arg("run")
            .arg("--flow=tests/mock/basic.yaml")
            .arg("--input-yaml=m: 2\nn: 7")
    );
}

#[test]
fn test_run_with_custom_config() {
    assert_cmd_snapshot!(
        stepflow()
            .arg("run")
            .arg("--flow=tests/builtins/nested_eval.yaml")
            .arg("--config=tests/builtins/stepflow-config.yml")
            .arg(r#"--input-json={"input": 42}"#)
    );
}

#[test]
fn test_run_nonexistent_workflow() {
    assert_cmd_snapshot!(
        stepflow()
            .arg("run")
            .arg("--flow=nonexistent.yaml")
            .arg(r#"--input-json={}"#)
    );
}

#[test]
fn test_run_invalid_input_json() {
    assert_cmd_snapshot!(
        stepflow()
            .arg("run")
            .arg("--flow=tests/mock/basic.yaml")
            .arg("--input-json=invalid json")
    );
}
