use super::stepflow;
use insta_cmd::assert_cmd_snapshot;

#[test]
fn test_test_help() {
    assert_cmd_snapshot!(stepflow().arg("test").arg("--help"));
}
