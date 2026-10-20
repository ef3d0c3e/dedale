//! `dedale view`: listing a results directory and serving its files.

use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use dedale::tournament::{
    BotInfo, GameEntry, GamePlan, MazeSource, Mode, TOURNAMENT_FORMAT, TournamentReport,
};
use dedale::web::browse::{Browse, Entry, scan};
use dedale::web::http::{Body, Method, Request, Response};
use dedale::web::server::Handler;
use dedale_core::game::Seating;
use dedale_core::protocol::About;
use dedale_core::record::GameRecord;
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

const GOLDEN: &str = include_str!("golden/replay.json");

fn write(path: &Path, contents: &str, modified: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
    let time = SystemTime::UNIX_EPOCH + Duration::from_secs(modified);
    File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(time)
        .unwrap();
}

fn report() -> TournamentReport {
    let record = GameRecord::from_json(GOLDEN).unwrap();
    let game = |index, seating, replay: &str| GameEntry {
        plan: GamePlan {
            index,
            code: record.code,
            seating,
            bots: vec![0, 1],
        },
        result: record.result.clone(),
        replay: replay.to_owned(),
    };
    TournamentReport {
        format: TOURNAMENT_FORMAT,
        dedale: "0.1.0".to_owned(),
        master_seed: "0000000000000001".to_owned(),
        mode: Mode::Duo,
        maze: MazeSource::Map {
            path: "ring.txt".to_owned(),
        },
        startup_ms: 500,
        move_ms: 100,
        max_turns: None,
        bots: vec![
            BotInfo {
                name: "lap".to_owned(),
                path: "lap".to_owned(),
                about: About::default(),
            },
            BotInfo {
                name: "sitter".to_owned(),
                path: "sitter".to_owned(),
                about: About::default(),
            },
        ],
        games: vec![
            game(1, Seating::Duo, "games/001-duo/replay.json"),
            game(2, Seating::DuoSwapped, "games/002-duo-swapped/replay.json"),
        ],
        leaderboard: Vec::new(),
    }
}

/// A results tree, plus a directory outside it.
fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("out");
    write(&root.join("run-a/replay.json"), GOLDEN, 1000);
    write(&root.join("run-a/notes.txt"), "secret", 1000);
    write(&root.join("run-b/replay.json"), GOLDEN, 3000);
    let report = serde_json::to_string(&report()).unwrap();
    write(&root.join("cup/tournament.json"), &report, 2000);
    write(&root.join("cup/games/001-duo/replay.json"), GOLDEN, 1900);
    write(&root.join("broken/replay.json"), "not json", 500);
    write(&root.join("old/replay.json"), r#"{"format": 99}"#, 400);
    write(&root.join("deep/a/b/replay.json"), GOLDEN, 300);
    write(&root.join(".hidden/replay.json"), GOLDEN, 5000);
    write(&dir.path().join("outside/replay.json"), GOLDEN, 6000);
    symlink(dir.path().join("outside"), root.join("link")).unwrap();
    fs::create_dir_all(root.join("run-a/evil")).unwrap();
    symlink("../notes.txt", root.join("run-a/evil/replay.json")).unwrap();
    dir
}

#[test]
fn lists_games_and_tournaments_newest_first() {
    let dir = fixture();
    let listing = scan(&dir.path().join("out"));
    assert!(!listing.truncated);
    let paths: Vec<&str> = listing
        .entries
        .iter()
        .map(|e| match e {
            Entry::Game(g) => g.path.as_str(),
            Entry::Tournament(t) => t.path.as_str(),
        })
        .collect();
    // Hidden directories and symbolic links are skipped; tournament games
    // are listed under their tournament.
    assert_eq!(
        paths,
        [
            "run-b/replay.json",
            "cup/tournament.json",
            "run-a/replay.json",
            "broken/replay.json",
            "old/replay.json",
            "deep/a/b/replay.json",
        ]
    );

    let record = GameRecord::from_json(GOLDEN).unwrap();
    let Entry::Game(run) = &listing.entries[0] else {
        panic!("{:?}", listing.entries[0]);
    };
    assert_eq!(run.modified, 3000);
    assert_eq!(run.error, None);
    assert_eq!(run.code, record.code);
    assert_eq!(run.seating, Some(record.config.seating));
    let names: Vec<String> = record.players.iter().map(|p| p.name.clone()).collect();
    assert_eq!(run.bots, names);
    assert_eq!(run.result.as_ref(), Some(&record.result));

    let Entry::Tournament(cup) = &listing.entries[1] else {
        panic!("{:?}", listing.entries[1]);
    };
    assert_eq!((cup.modified, cup.mode), (2000, Some(Mode::Duo)));
    assert_eq!(cup.bots, ["lap", "sitter"]);
    assert_eq!(cup.games.len(), 2);
    assert_eq!(cup.games[0].path, "cup/games/001-duo/replay.json");
    assert_eq!(
        (cup.games[0].modified, cup.games[0].error.as_deref()),
        (1900, None)
    );
    assert_eq!(cup.games[1].seating, Some(Seating::DuoSwapped));
    assert_eq!(cup.games[1].error.as_deref(), Some("replay missing"));

    // Unreadable files are listed with their error.
    for (entry, needle) in [(3, "expected"), (4, "unsupported format 99")] {
        let Entry::Game(game) = &listing.entries[entry] else {
            panic!();
        };
        let error = game.error.as_deref().unwrap();
        assert!(error.contains(needle), "{error}");
        assert_eq!(game.result, None);
    }
}

fn get(browse: &Browse, path: &str, query: &[(&str, &str)]) -> Response {
    browse.api(&Request {
        method: Method::Get,
        path: path.to_owned(),
        query: query
            .iter()
            .map(|&(k, v)| (k.to_owned(), v.to_owned()))
            .collect(),
        headers: Vec::new(),
        body: Vec::new(),
    })
}

fn bytes(response: &Response) -> &[u8] {
    match &response.body {
        Body::Bytes(bytes) => bytes,
        Body::Events(_) => panic!("a stream"),
    }
}

#[test]
fn serves_result_files_and_nothing_else() {
    let dir = fixture();
    let browse = Browse::new(&dir.path().join("out")).unwrap();
    for path in [
        "run-a/replay.json",
        "cup/tournament.json",
        "deep/a/b/replay.json",
    ] {
        let response = get(&browse, "/api/file", &[("path", path)]);
        assert_eq!(response.status, 200, "{path}");
        assert_eq!(response.content_type, "application/json");
        let expected = fs::read(dir.path().join("out").join(path)).unwrap();
        assert_eq!(bytes(&response), expected);
    }
    let outside = dir.path().join("outside/replay.json");
    let outside = outside.to_str().unwrap();
    for (path, status) in [
        ("", 400),
        ("run-a", 400),
        ("run-a/notes.txt", 400),
        ("../outside/replay.json", 400),
        ("run-a/../run-b/replay.json", 400),
        ("./run-a/replay.json", 400),
        (outside, 400),
        ("/etc/passwd", 400),
        // Symbolic links out of the root, or to another file.
        ("link/replay.json", 403),
        ("run-a/evil/replay.json", 403),
        ("missing/replay.json", 404),
    ] {
        let response = get(&browse, "/api/file", &[("path", path)]);
        assert_eq!(response.status, status, "{path}");
        assert!(!bytes(&response).starts_with(b"secret"));
    }
    assert_eq!(get(&browse, "/api/file", &[]).status, 400);
    assert_eq!(get(&browse, "/api/nope", &[]).status, 404);

    let session = get(&browse, "/api/session", &[]);
    assert_eq!(
        session.status, 404,
        "the server answers /api/session itself"
    );
    assert_eq!(browse.session()["mode"], "browse");
    let runs: serde_json::Value =
        serde_json::from_slice(bytes(&get(&browse, "/api/runs", &[]))).unwrap();
    assert_eq!(runs["entries"].as_array().unwrap().len(), 6);
    assert_eq!(runs["entries"][1]["kind"], "tournament");
}

#[test]
fn a_missing_root_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    assert!(Browse::new(&dir.path().join("nope")).is_err());
    let file = dir.path().join("file");
    fs::write(&file, "").unwrap();
    assert!(Browse::new(&file).is_err());
}

/// One GET over HTTP; the status and body.
fn http_get(port: u16, target: &str) -> (u16, Vec<u8>) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        stream,
        "GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"
    )
    .unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();
    let end = bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let status = std::str::from_utf8(&bytes[9..12]).unwrap().parse().unwrap();
    (status, bytes[end + 4..].to_vec())
}

#[test]
fn the_view_command_serves_until_ctrl_c() {
    let dir = fixture();
    let mut child = Command::new(env!("CARGO_BIN_EXE_dedale"))
        .args([
            "view",
            dir.path().join("out").to_str().unwrap(),
            "--no-open",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let url = line
        .strip_prefix("view    http://127.0.0.1:")
        .expect(&line)
        .trim();
    let (port, rest) = url.split_once('/').unwrap();
    let port: u16 = port.parse().unwrap();
    let token = rest.strip_prefix("?token=").unwrap();

    let (status, body) = http_get(port, &format!("/api/runs?token={token}"));
    assert_eq!(status, 200);
    let runs: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(runs["entries"][0]["path"], "run-b/replay.json");
    let (status, body) = http_get(
        port,
        &format!("/api/file?path=run-a%2Freplay.json&token={token}"),
    );
    assert_eq!((status, body.as_slice()), (200, GOLDEN.as_bytes()));
    let (status, _) = http_get(
        port,
        &format!("/api/file?path=%2e%2e%2fout%2Frun-a%2Freplay.json&token={token}"),
    );
    assert_eq!(status, 400);
    let (status, _) = http_get(port, "/api/runs");
    assert_eq!(status, 403);

    let pid = Pid::from_raw(i32::try_from(child.id()).unwrap());
    kill(pid, Signal::SIGINT).unwrap();
    assert!(child.wait().unwrap().success());
}

#[test]
fn the_view_command_needs_a_directory() {
    let output = Command::new(env!("CARGO_BIN_EXE_dedale"))
        .args(["view", "/nonexistent/results", "--no-open"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("cannot browse /nonexistent/results"),
        "{stderr}"
    );
}
