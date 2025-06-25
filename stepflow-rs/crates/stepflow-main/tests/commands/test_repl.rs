use insta_cmd::assert_cmd_snapshot;

use super::stepflow;

#[test]
fn test_repl_help() {
    // Test REPL help by passing it as subcommand help
    assert_cmd_snapshot!(stepflow().args(["repl", "--help"]));
}

// Note: Interactive stdin testing with insta_cmd is complex, so we test the core functionality
// through integration tests that can properly handle the interactive nature of the REPL.
// For now, we'll focus on testing the command structure and help output.
