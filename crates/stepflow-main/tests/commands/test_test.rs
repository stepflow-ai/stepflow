use super::stepflow;
use insta_cmd::assert_cmd_snapshot;

#[test]
fn test_test_help() {
    assert_cmd_snapshot!(stepflow().arg("test").arg("--help"));
}

#[test]
fn test_test_basic_workflow() {
    assert_cmd_snapshot!(stepflow()
        .arg("test")
        .arg("tests/basic.yaml"));
}

#[test]
fn test_test_mock_directory() {
    assert_cmd_snapshot!(stepflow()
        .arg("test")
        .arg("tests/mock"));
}

#[test]
fn test_test_with_custom_config() {
    assert_cmd_snapshot!(stepflow()
        .arg("test")
        .arg("--config=tests/builtins/stepflow-config.yml")
        .arg("tests/builtins"));
}

#[test]
fn test_test_nonexistent_path() {
    assert_cmd_snapshot!(stepflow()
        .arg("test")
        .arg("nonexistent_path"));
}
