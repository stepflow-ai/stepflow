#[test]
fn test_help() {
    assert_cmd_snapshot!(stepflow().arg("--help"));
}
