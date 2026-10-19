//! `dedale view`: the runs and tournaments under a directory.
//!
//! A directory holding a `replay.json` is a game; one holding a
//! `tournament.json` is a tournament, whose games are listed under it (from
//! the file) rather than on their own. Symbolic links are never followed,
//! and only those two file names are ever served.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use dedale_core::code::MazeCode;
use dedale_core::game::{GameResult, Seating};
use dedale_core::record::REPLAY_FORMAT;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::http::{Method, Request, Response};
use super::server::Handler;
use crate::tournament::{Mode, TOURNAMENT_FORMAT, TournamentReport};

/// Directory levels looked into below the root.
const MAX_DEPTH: usize = 8;
/// Directories visited per scan, so a huge tree cannot hold the server.
pub const MAX_DIRS: usize = 10_000;

/// Serves the app over the results under a root directory.
#[derive(Debug)]
pub struct Browse {
    root: PathBuf,
    title: String,
}

impl Browse {
    /// # Errors
    ///
    /// When `root` is not a directory.
    pub fn new(root: &Path) -> io::Result<Self> {
        let canonical = root.canonicalize()?;
        if !canonical.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                format!("{} is not a directory", root.display()),
            ));
        }
        Ok(Self {
            root: canonical,
            title: root.display().to_string(),
        })
    }

    /// `replay.json` or `tournament.json` at `relative` under the root.
    fn file(&self, relative: &str) -> Response {
        let path = Path::new(relative);
        if relative.is_empty()
            || !path.components().all(|c| matches!(c, Component::Normal(_)))
            || !is_result_file(path)
        {
            return Response::error(400, "only replay.json and tournament.json under the root");
        }
        let Ok(real) = self.root.join(path).canonicalize() else {
            return Response::not_found();
        };
        // Through a symbolic link, possibly out of the root or to another file.
        if !real.starts_with(&self.root) || !is_result_file(&real) {
            return Response::error(403, "only replay.json and tournament.json under the root");
        }
        match fs::read(&real) {
            Ok(bytes) => Response::bytes(200, "application/json", bytes),
            Err(_) => Response::not_found(),
        }
    }
}

impl Handler for Browse {
    fn session(&self) -> serde_json::Value {
        json!({ "mode": "browse", "title": self.title })
    }

    fn api(&self, request: &Request) -> Response {
        match (request.path.as_str(), &request.method) {
            ("/api/runs", Method::Get) => Response::json(200, &scan(&self.root)),
            ("/api/file", Method::Get) => self.file(request.query("path").unwrap_or_default()),
            ("/api/runs" | "/api/file", _) => Response::method_not_allowed("GET"),
            _ => Response::not_found(),
        }
    }
}

fn is_result_file(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(OsStr::to_str),
        Some("replay.json" | "tournament.json")
    )
}

/// `GET /api/runs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Listing {
    /// Newest first.
    pub entries: Vec<Entry>,
    /// The scan stopped at [`MAX_DIRS`] directories.
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Entry {
    Game(GameSummary),
    Tournament(TournamentSummary),
}

impl Entry {
    fn modified(&self) -> u64 {
        match self {
            Entry::Game(g) => g.modified,
            Entry::Tournament(t) => t.modified,
        }
    }

    fn path(&self) -> &str {
        match self {
            Entry::Game(g) => &g.path,
            Entry::Tournament(t) => &t.path,
        }
    }
}

/// A game, from its replay (or from its tournament's file). Everything but
/// the path is missing when the file cannot be read, which `error` says.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct GameSummary {
    /// Of the `replay.json`, relative to the root, `/`-separated.
    pub path: String,
    /// Seconds since the Unix epoch.
    pub modified: u64,
    pub error: Option<String>,
    pub code: Option<MazeCode>,
    pub seating: Option<Seating>,
    pub bots: Vec<String>,
    pub result: Option<GameResult>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TournamentSummary {
    /// Of the `tournament.json`.
    pub path: String,
    pub modified: u64,
    pub error: Option<String>,
    pub mode: Option<Mode>,
    pub bots: Vec<String>,
    /// The bots ranked first.
    pub leaders: Vec<String>,
    pub games: Vec<GameSummary>,
}

/// Every game and tournament under `root`, newest first.
#[must_use]
pub fn scan(root: &Path) -> Listing {
    let mut scan = Scan {
        entries: Vec::new(),
        visited: 0,
    };
    scan.walk(root, "", 0);
    let mut entries = scan.entries;
    entries.sort_by(|a, b| {
        b.modified()
            .cmp(&a.modified())
            .then_with(|| a.path().cmp(b.path()))
    });
    Listing {
        entries,
        truncated: scan.visited > MAX_DIRS,
    }
}

struct Scan {
    entries: Vec<Entry>,
    visited: usize,
}

impl Scan {
    fn walk(&mut self, dir: &Path, relative: &str, depth: usize) {
        self.visited += 1;
        if self.visited > MAX_DIRS {
            return;
        }
        let join = |name: &str| {
            if relative.is_empty() {
                name.to_owned()
            } else {
                format!("{relative}/{name}")
            }
        };
        if is_regular(&dir.join("tournament.json")) {
            let summary = tournament(dir, &join("tournament.json"), relative);
            self.entries.push(Entry::Tournament(summary));
            // Its games are listed under it.
            return;
        }
        if is_regular(&dir.join("replay.json")) {
            let summary = game(&dir.join("replay.json"), join("replay.json"));
            self.entries.push(Entry::Game(summary));
        }
        if depth >= MAX_DEPTH {
            return;
        }
        let Ok(children) = fs::read_dir(dir) else {
            return;
        };
        let mut subdirs: Vec<(String, PathBuf)> = children
            .filter_map(Result::ok)
            // `file_type` does not follow symbolic links.
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter_map(|e| Some((e.file_name().into_string().ok()?, e.path())))
            .filter(|(name, _)| !name.starts_with('.'))
            .collect();
        subdirs.sort();
        for (name, path) in subdirs {
            self.walk(&path, &join(&name), depth + 1);
        }
    }
}

/// A regular file, not a symbolic link.
fn is_regular(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_file())
}

fn modified(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}

/// The fields of a replay a listing shows; the rest is skipped unparsed.
#[derive(Deserialize)]
struct ReplayHead {
    code: Option<MazeCode>,
    config: ConfigHead,
    players: Vec<PlayerHead>,
    result: GameResult,
}

#[derive(Deserialize)]
struct ConfigHead {
    seating: Seating,
}

#[derive(Deserialize)]
struct PlayerHead {
    name: String,
}

#[derive(Deserialize)]
struct Format {
    format: u32,
}

/// Reads `bytes` as a file of version `expected`.
fn parse<T: for<'de> Deserialize<'de>>(bytes: &[u8], expected: u32) -> Result<T, String> {
    let Format { format } = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if format != expected {
        return Err(format!("unsupported format {format}"));
    }
    serde_json::from_slice(bytes).map_err(|e| e.to_string())
}

fn game(file: &Path, path: String) -> GameSummary {
    let mut summary = GameSummary {
        path,
        modified: modified(file),
        ..GameSummary::default()
    };
    let head = fs::read(file)
        .map_err(|e| e.to_string())
        .and_then(|bytes| parse::<ReplayHead>(&bytes, REPLAY_FORMAT));
    match head {
        Ok(head) => {
            summary.code = head.code;
            summary.seating = Some(head.config.seating);
            summary.bots = head.players.into_iter().map(|p| p.name).collect();
            summary.result = Some(head.result);
        }
        Err(e) => summary.error = Some(e),
    }
    summary
}

fn tournament(dir: &Path, path: &str, relative: &str) -> TournamentSummary {
    let file = dir.join("tournament.json");
    let mut summary = TournamentSummary {
        path: path.to_owned(),
        modified: modified(&file),
        ..TournamentSummary::default()
    };
    let report = fs::read(&file)
        .map_err(|e| e.to_string())
        .and_then(|bytes| parse::<TournamentReport>(&bytes, TOURNAMENT_FORMAT));
    let report = match report {
        Ok(report) => report,
        Err(e) => {
            summary.error = Some(e);
            return summary;
        }
    };
    let name = |bot: usize| report.bots.get(bot).map(|b| b.name.clone());
    summary.mode = Some(report.mode);
    summary.bots = report.bots.iter().map(|b| b.name.clone()).collect();
    summary.leaders = report
        .leaderboard
        .iter()
        .filter(|s| s.rank == 1)
        .map(|s| s.name.clone())
        .collect();
    summary.games = report
        .games
        .iter()
        .map(|entry| {
            let replay = dir.join(&entry.replay);
            GameSummary {
                path: if relative.is_empty() {
                    entry.replay.clone()
                } else {
                    format!("{relative}/{}", entry.replay)
                },
                modified: modified(&replay),
                error: (!is_regular(&replay)).then(|| "replay missing".to_owned()),
                code: entry.plan.code,
                seating: Some(entry.plan.seating),
                bots: entry.plan.bots.iter().filter_map(|&b| name(b)).collect(),
                result: Some(entry.result.clone()),
            }
        })
        .collect();
    summary
}
