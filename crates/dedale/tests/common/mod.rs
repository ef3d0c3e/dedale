//! Helpers shared by the integration tests.

#![allow(dead_code, unreachable_pub)]

pub mod http;

use std::path::{Path, PathBuf};
use std::process::Command;

use dedale::bot::BotSpec;
use dedale::python::PythonRuntime;
use dedale::referee::Entrant;
use dedale_core::protocol::About;

/// The scripted fake bot's executable.
pub fn fake_bot_path() -> &'static str {
    let Some(path) = option_env!("CARGO_BIN_EXE_dedale-fake-bot") else {
        panic!("dedale-fake-bot was not built: run the tests with --all-features");
    };
    path
}

/// A command running the scripted fake bot with `args`.
pub fn fake_bot(args: &[&str]) -> Command {
    let mut command = Command::new(fake_bot_path());
    command.args(args);
    command
}

/// The Python interpreter: `DEDALE_PYTHON` or `python3`.
pub fn python() -> String {
    std::env::var("DEDALE_PYTHON").unwrap_or_else(|_| "python3".to_owned())
}

/// The repository root.
pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A Python bot from the misbehaving test bots in `tests/bots/`.
pub fn test_bot(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/bots")
        .join(format!("{name}.py"))
}

/// An example bot from `examples/python/`.
pub fn example_bot(name: &str) -> PathBuf {
    repo().join("examples/python").join(format!("{name}.py"))
}

/// An entrant running the Python bot at `path` through `runtime`.
pub fn python_entrant(runtime: &PythonRuntime, path: &Path) -> Entrant {
    let spec = BotSpec::from_path(path);
    Entrant {
        name: spec.name(),
        path: path.display().to_string(),
        about: About::default(),
        command: spec.command(&runtime.launcher(python())),
        stderr_log: None,
    }
}
