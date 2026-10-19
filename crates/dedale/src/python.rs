//! The embedded Python package and harness.
//!
//! The sources in `bindings/python/dedale/` are compiled into the binary and
//! written to a temporary directory for each run, so `dedale run bot.py`
//! needs nothing installed besides Python.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::PathBuf;

use tempfile::TempDir;

use crate::bot::Launcher;

/// The package files, by path relative to the extraction directory.
pub const FILES: [(&str, &str); 3] = [
    (
        "dedale/__init__.py",
        include_str!("../../../bindings/python/dedale/__init__.py"),
    ),
    (
        "dedale/_harness.py",
        include_str!("../../../bindings/python/dedale/_harness.py"),
    ),
    (
        "dedale/py.typed",
        include_str!("../../../bindings/python/dedale/py.typed"),
    ),
];

/// The extracted package; the directory is removed on drop.
#[derive(Debug)]
pub struct PythonRuntime {
    dir: TempDir,
}

impl PythonRuntime {
    /// Writes the package to a new temporary directory.
    ///
    /// # Errors
    ///
    /// When the directory or a file cannot be written.
    pub fn extract() -> io::Result<Self> {
        let dir = tempfile::Builder::new()
            .prefix("dedale-python-")
            .tempdir()?;
        for (path, contents) in FILES {
            let path = dir.path().join(path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, contents)?;
        }
        Ok(Self { dir })
    }

    #[must_use]
    pub fn harness(&self) -> PathBuf {
        self.dir.path().join("dedale/_harness.py")
    }

    /// A launcher running Python bots with `python` and this harness.
    #[must_use]
    pub fn launcher(&self, python: impl Into<OsString>) -> Launcher {
        Launcher {
            python: python.into(),
            harness: self.harness(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FILES, PythonRuntime};
    use std::fs;

    #[test]
    fn extracts_the_embedded_files() {
        let runtime = PythonRuntime::extract().unwrap();
        let root = runtime
            .harness()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_owned();
        for (path, contents) in FILES {
            assert_eq!(fs::read_to_string(root.join(path)).unwrap(), contents);
        }
        drop(runtime);
        assert!(!root.exists());
    }
}
