use super::stepflow;
use insta_cmd::assert_cmd_snapshot;

#[test]
fn test_submit_help() {
    assert_cmd_snapshot!(stepflow().arg("submit").arg("--help"));
}

#[test]
fn test_submit_connection_error() {
    // Test that submit fails gracefully when server is not running
    assert_cmd_snapshot!(
        stepflow()
            .arg("submit")
            .arg("--url=http://localhost:9999")
            .arg("--flow=tests/mock/basic.yaml")
            .arg(r#"--input-json={"m": 1, "n": 2}"#)
    );
}

#[test]
fn test_submit_with_file_input() {
    // Test submit command parsing with file input
    assert_cmd_snapshot!(
        stepflow()
            .arg("submit")
            .arg("--url=http://localhost:9999")
            .arg("--flow=tests/mock/basic.yaml")
            .arg("--input=../../../../examples/eval/input.json")
    );
}

#[test]
fn test_submit_nonexistent_workflow() {
    assert_cmd_snapshot!(
        stepflow()
            .arg("submit")
            .arg("--url=http://localhost:9999")
            .arg("--flow=nonexistent.yaml")
            .arg(r#"--input-json={}"#)
    );
}
