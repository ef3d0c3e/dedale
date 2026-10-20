//! What a competitor is and how to start it.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use dedale_core::protocol::{About, FromBot, decode_bot_line};

use crate::interrupt;
use crate::process::{BotProcess, Output};
use crate::sandbox::Sandbox;

/// A competitor program: a Python bot, or any executable speaking the raw
/// protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BotSpec {
    Python(PathBuf),
    Executable(PathBuf),
}

impl BotSpec {
    /// `.py` files are Python bots; anything else is run directly.
    #[must_use]
    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        if path.extension().is_some_and(|ext| ext == "py") {
            BotSpec::Python(path)
        } else {
            BotSpec::Executable(path)
        }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            BotSpec::Python(path) | BotSpec::Executable(path) => path,
        }
    }

    /// The file name without its extension, used as the player's name.
    #[must_use]
    pub fn name(&self) -> String {
        self.path().file_stem().map_or_else(
            || self.path().display().to_string(),
            |stem| stem.to_string_lossy().into_owned(),
        )
    }

    /// What the bot says about itself in its `hello`: starts
    /// it, reads its first line and stops it. A Python bot's harness reads
    /// it from the source, so none of the bot's code runs. `None` when the
    /// bot does not start or says no `hello` within `timeout` (a game
    /// reports why).
    #[must_use]
    pub fn about(&self, launcher: &Launcher, timeout: Duration) -> Option<About> {
        let Ok(process) = BotProcess::spawn(self.command(launcher), None) else {
            return None;
        };
        let deadline = Instant::now() + timeout;
        // Dropping the process kills it.
        loop {
            if interrupt::requested() {
                return None;
            }
            let slice = deadline.min(Instant::now() + interrupt::POLL);
            match process.recv_until(slice) {
                Some(Output::Line { line, .. }) => {
                    return match decode_bot_line(&line) {
                        Ok(FromBot::Hello(hello)) => Some(hello.about),
                        _ => None,
                    };
                }
                Some(_) => return None,
                None if slice >= deadline => return None,
                None => {}
            }
        }
    }

    /// The command that starts the bot. Paths are made absolute so a bare
    /// file name is never looked up in `PATH`.
    #[must_use]
    pub fn command(&self, launcher: &Launcher) -> Command {
        self.command_on(launcher, None)
    }

    /// [`BotSpec::command`], pinned to `cpu` when the launcher sandboxes
    /// bots. A sandboxed bot sees its own directory and starts there.
    #[must_use]
    pub fn command_on(&self, launcher: &Launcher, cpu: Option<usize>) -> Command {
        let path = std::path::absolute(self.path()).unwrap_or_else(|_| self.path().to_owned());
        let dir = path.parent().unwrap_or(Path::new("/"));
        let env = [("PYTHONDONTWRITEBYTECODE", "1")];
        match (self, &launcher.sandbox) {
            (BotSpec::Python(_), None) => {
                let mut command = Command::new(&launcher.python);
                command
                    .arg("-u")
                    .arg(&launcher.harness)
                    .arg(&path)
                    .envs(env);
                command
            }
            (BotSpec::Executable(_), None) => Command::new(&path),
            (BotSpec::Python(_), Some(sandbox)) => {
                let python = sandbox.python().unwrap_or(Path::new(&launcher.python));
                let harness = launcher.harness.parent().unwrap_or(Path::new("/"));
                // The harness imports the `dedale` package next to it.
                let package = harness.parent().unwrap_or(harness);
                let args = [
                    OsStr::new("-u"),
                    launcher.harness.as_os_str(),
                    path.as_os_str(),
                ];
                sandbox.command(python, &args, &env, &[package, dir], dir, cpu)
            }
            (BotSpec::Executable(_), Some(sandbox)) => {
                sandbox.command(&path, &[], &[], &[dir], dir, cpu)
            }
        }
    }
}

/// [`BotSpec::about`] for every bot, all at once.
#[must_use]
pub fn abouts(specs: &[BotSpec], launcher: &Launcher, timeout: Duration) -> Vec<Option<About>> {
    thread::scope(|scope| {
        let probes: Vec<_> = specs
            .iter()
            .map(|spec| scope.spawn(move || spec.about(launcher, timeout)))
            .collect();
        probes
            .into_iter()
            .map(|probe| probe.join().unwrap_or_default())
            .collect()
    })
}

/// How bots are started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launcher {
    /// The interpreter, `python3` by default.
    pub python: OsString,
    /// The harness script that loads a bot module and speaks the protocol.
    pub harness: PathBuf,
    /// The sandbox every bot runs in, if any.
    pub sandbox: Option<Sandbox>,
}

impl Launcher {
    /// The same launcher, running bots in `sandbox` (none for `None`).
    #[must_use]
    pub fn sandboxed(self, sandbox: Option<Sandbox>) -> Self {
        Self { sandbox, ..self }
    }
}

#[cfg(test)]
mod tests {
    use super::{BotSpec, Launcher};
    use std::path::Path;

    #[test]
    fn specs_from_paths() {
        assert_eq!(
            BotSpec::from_path("bots/greedy.py"),
            BotSpec::Python("bots/greedy.py".into())
        );
        assert_eq!(
            BotSpec::from_path("bots/fast"),
            BotSpec::Executable("bots/fast".into())
        );
        assert_eq!(BotSpec::from_path("bots/greedy.py").name(), "greedy");
        assert_eq!(BotSpec::from_path("a.out").name(), "a");
    }

    #[test]
    fn commands() {
        let launcher = Launcher {
            python: "python3.12".into(),
            harness: "/tmp/h/_harness.py".into(),
            sandbox: None,
        };
        let command = BotSpec::from_path("greedy.py").command(&launcher);
        assert_eq!(command.get_program(), "python3.12");
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args[..2], ["-u", "/tmp/h/_harness.py"]);
        assert!(Path::new(args[2]).is_absolute());
        assert!(Path::new(args[2]).ends_with("greedy.py"));

        let command = BotSpec::from_path("fast").command(&launcher);
        assert!(Path::new(command.get_program()).is_absolute());
        assert_eq!(command.get_args().count(), 0);
    }
}
