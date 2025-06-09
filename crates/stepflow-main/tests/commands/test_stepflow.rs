use super::stepflow;
use insta_cmd::assert_cmd_snapshot;

#[test]
fn test_help() {
    assert_cmd_snapshot!(stepflow().arg("--help"));
}

#[test]
fn test_version() {
    assert_cmd_snapshot!(stepflow().arg("--version"));
}

#[test]
fn test_no_command() {
    assert_cmd_snapshot!(stepflow());
}

#[test]
fn test_invalid_command() {
    assert_cmd_snapshot!(stepflow().arg("invalid-command"));
}

#[test]
fn test_with_log_level() {
    assert_cmd_snapshot!(stepflow()
        .arg("--log-level=debug")
        .arg("--help"));
}

#[test]
fn test_with_different_log_levels() {
    assert_cmd_snapshot!(stepflow()
        .arg("--log-level=trace")
        .arg("--other-log-level=debug")
        .arg("--help"));
}
