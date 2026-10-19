//! What a competitor is and how to start it.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

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

    /// The command that starts the bot. Paths are made absolute so a bare
    /// file name is never looked up in `PATH`.
    #[must_use]
    pub fn command(&self, launcher: &Launcher) -> Command {
        let absolute = |path: &Path| std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
        match self {
            BotSpec::Python(script) => {
                let mut command = Command::new(&launcher.python);
                command
                    .arg("-u")
                    .arg(&launcher.harness)
                    .arg(absolute(script))
                    .env("PYTHONDONTWRITEBYTECODE", "1");
                command
            }
            BotSpec::Executable(path) => Command::new(absolute(path)),
        }
    }
}

/// How Python bots are started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launcher {
    /// The interpreter, `python3` by default.
    pub python: OsString,
    /// The harness script that loads a bot module and speaks the protocol.
    pub harness: PathBuf,
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
