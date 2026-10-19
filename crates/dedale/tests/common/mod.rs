//! Helpers shared by the integration tests.

#![allow(dead_code, unreachable_pub)]

use std::process::Command;

/// A command running the scripted fake bot with `args`.
pub fn fake_bot(args: &[&str]) -> Command {
    let Some(path) = option_env!("CARGO_BIN_EXE_dedale-fake-bot") else {
        panic!("dedale-fake-bot was not built: run the tests with --all-features");
    };
    let mut command = Command::new(path);
    command.args(args);
    command
}
