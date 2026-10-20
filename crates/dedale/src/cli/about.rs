//! `dedale about`: what bots say about themselves, as JSON lines, without
//! playing a game. Submission checks read it; judging is their job, so a bot
//! that says nothing is reported, not an error.

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use clap::Args;
use dedale_core::protocol::About;
use serde::Serialize;

use super::CliResult;
use super::run::SandboxArgs;
use crate::bot::{BotSpec, abouts};
use crate::python::PythonRuntime;

#[derive(Debug, Args)]
pub(crate) struct AboutArgs {
    /// Bots: Python files (`.py`).
    #[arg(required = true, value_name = "BOT")]
    bots: Vec<PathBuf>,
    /// How long a bot may take to say `hello`, in milliseconds.
    #[arg(long, value_name = "MS", default_value_t = 10_000)]
    hello_ms: u64,
    /// Python interpreter for `.py` bots.
    #[arg(
        long,
        value_name = "PATH",
        env = "DEDALE_PYTHON",
        default_value = "python3"
    )]
    python: OsString,
    #[command(flatten)]
    sandbox: SandboxArgs,
}

/// One line of output.
#[derive(Debug, Serialize)]
struct Report {
    path: String,
    /// False when the bot did not start or said no `hello` in time.
    said_hello: bool,
    #[serde(flatten)]
    about: About,
}

pub(crate) fn run(args: &AboutArgs) -> CliResult {
    let runtime = PythonRuntime::extract()?;
    let launcher = runtime
        .launcher(&args.python)
        .sandboxed(args.sandbox.sandbox(&args.python)?);
    let specs: Vec<BotSpec> = args.bots.iter().map(BotSpec::from_path).collect();
    let found = abouts(&specs, &launcher, Duration::from_millis(args.hello_ms));
    let mut stdout = std::io::stdout().lock();
    for (path, about) in args.bots.iter().zip(found) {
        let report = Report {
            path: path.display().to_string(),
            said_hello: about.is_some(),
            about: about.unwrap_or_default(),
        };
        writeln!(stdout, "{}", serde_json::to_string(&report)?)?;
    }
    Ok(())
}
