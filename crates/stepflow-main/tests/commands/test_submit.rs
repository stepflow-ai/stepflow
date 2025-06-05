use super::stepflow;
use insta_cmd::assert_cmd_snapshot;

#[test]
fn test_submit_help() {
    assert_cmd_snapshot!(stepflow().arg("submit").arg("--help"));
}
