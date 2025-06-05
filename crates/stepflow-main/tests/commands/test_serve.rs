use super::stepflow;
use insta_cmd::assert_cmd_snapshot;

#[test]
fn test_serve_help() {
    assert_cmd_snapshot!(stepflow().arg("serve").arg("--help"));
}
