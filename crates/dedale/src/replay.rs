//! Replay files.

use std::fs;
use std::io;
use std::path::Path;

use dedale_core::record::{GameRecord, ReplayError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReadError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("{path}: {source}")]
    Replay {
        path: String,
        #[source]
        source: ReplayError,
    },
}

/// Writes `record` as compact JSON.
///
/// # Errors
///
/// When the file cannot be written.
pub fn write(path: &Path, record: &GameRecord) -> io::Result<()> {
    fs::write(path, record.to_json())
}

/// # Errors
///
/// When the file cannot be read or is not a replay this Dedale supports.
pub fn read(path: &Path) -> Result<GameRecord, ReadError> {
    let display = path.display().to_string();
    let text = fs::read_to_string(path).map_err(|source| ReadError::Io {
        path: display.clone(),
        source,
    })?;
    GameRecord::from_json(&text).map_err(|source| ReadError::Replay {
        path: display,
        source,
    })
}
