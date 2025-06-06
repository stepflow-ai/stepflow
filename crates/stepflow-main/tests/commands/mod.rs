#![allow(clippy::mod_module_files)]
mod test_list_components;
mod test_repl;
mod test_run;
mod test_serve;
mod test_stepflow;
mod test_submit;
mod test_test;

use insta_cmd::Command;

fn stepflow() -> Command {
    let mut command = Command::new(insta_cmd::get_cargo_bin("stepflow"));
    command.arg("--log-file=/dev/null");
    command.arg("--omit-stack-trace");
    command
}
