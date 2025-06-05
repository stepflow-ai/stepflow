use super::stepflow;
use insta_cmd::assert_cmd_snapshot;

#[test]
fn test_run_help() {
    assert_cmd_snapshot!(stepflow().arg("run").arg("--help"));
}
