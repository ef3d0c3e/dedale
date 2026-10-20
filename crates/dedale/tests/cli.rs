//! The `dedale` binary, end to end.

mod common;

use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

use common::{example_bot, test_bot};
use dedale::replay;
use dedale::sandbox::{Limits, Sandbox};
use dedale_core::game::EndReason;
use dedale_core::maze::Maze;
use dedale_core::verify::verify;
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

fn dedale(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dedale"))
        .args(args)
        .env("DEDALE_PYTHON", common::python())
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

fn golden_maze(code: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../dedale-core/tests/golden")
        .join(format!("{code}.txt"));
    fs::read_to_string(path).unwrap()
}

fn path(p: &Path) -> &str {
    p.to_str().unwrap()
}

#[test]
fn maze_codes_reproduce_mazes() {
    let code = "ded2-11x7-l25-p65535-0123456789abcdef";
    let output = dedale(&["maze", "--maze", code]);
    assert!(output.status.success());
    assert_eq!(stdout(&output), golden_maze(code));
    // The printed code carries the number of points actually placed.
    let points = Maze::parse(&stdout(&output)).unwrap().point_count();
    let normalized = format!("ded2-11x7-l25-p{points}-0123456789abcdef");
    assert_eq!(stderr(&output).trim(), normalized);
    let again = dedale(&["maze", "--maze", &normalized]);
    assert_eq!(stdout(&again), stdout(&output));
    assert_eq!(stderr(&again).trim(), normalized);

    let output = dedale(&[
        "maze", "--size", "21x15", "--loops", "100", "--points", "60", "--seed", "deadbeef",
    ]);
    assert_eq!(
        stdout(&output),
        golden_maze("ded2-21x15-l100-p60-00000000deadbeef")
    );
}

#[test]
fn point_counts_are_capped() {
    let maze = |points: &str| {
        let output = dedale(&["maze", "--size", "11x11", "--seed", "7", "--points", points]);
        assert!(output.status.success(), "{}", stderr(&output));
        let maze = Maze::parse(&stdout(&output)).unwrap();
        (maze.point_count(), stderr(&output).trim().to_owned())
    };
    assert_eq!(
        maze("10"),
        (10, "ded2-11x11-l25-p10-0000000000000007".to_owned())
    );
    // The centre of an 11×11 maze is a pillar: points come in pairs.
    assert_eq!(maze("11").0, 10);
    let (all, code) = maze("1000000");
    let open = maze("65535");
    assert_eq!((all, &code), (open.0, &open.1));
    assert_eq!(code, format!("ded2-11x11-l25-p{all}-0000000000000007"));
}

#[test]
fn random_mazes_come_with_their_code() {
    let output = dedale(&["maze", "--size", "15x9"]);
    let maze = Maze::parse(&stdout(&output)).unwrap();
    assert_eq!((maze.width(), maze.height()), (15, 9));
    let code = stderr(&output).trim().to_owned();
    assert!(code.starts_with("ded2-15x9-l25-p"), "{code}");
    assert!(
        code.contains(&format!("-p{}-", maze.point_count())),
        "{code}"
    );
    assert_eq!(stdout(&dedale(&["maze", "--maze", &code])), stdout(&output));
}

#[test]
fn bad_arguments_are_rejected() {
    for args in [
        &["maze", "--size", "30x31"][..],
        &["maze", "--maze", "ded9-31x31-l25-p100-0000000000000000"],
        &[
            "maze",
            "--maze",
            "ded2-31x31-l25-p100-0000000000000000",
            "--size",
            "11x11",
        ],
        &["maze", "--points", "lots"],
        &["maze", "--maze", "ded1-31x31-l25-p100-0000000000000000"],
        &["run"],
        &["run", "a.py", "b.py", "c.py"],
    ] {
        let output = dedale(args);
        assert!(!output.status.success(), "{args:?} succeeded");
        assert!(!stderr(&output).is_empty(), "{args:?}");
    }
}

#[test]
fn a_missing_bot_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out");
    let output = dedale(&[
        "run",
        "/nonexistent/bot",
        "--size",
        "5x5",
        "--out",
        path(&out),
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("cannot start bot"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_solo_run_writes_a_verified_replay() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out");
    let greedy = example_bot("greedy_bot");
    let output = dedale(&[
        "run",
        path(&greedy),
        "--size",
        "11x11",
        "--seed",
        "7",
        "--out",
        path(&out),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    let record = replay::read(&out.join("replay.json")).unwrap();
    let points = record.maze.point_count();
    let code = format!("ded2-11x11-l25-p{points}-0000000000000007");
    assert!(text.starts_with(&format!("maze    {code}\n")), "{text}");
    assert_eq!(record.code.unwrap().to_string(), code);
    assert!(text.contains("result  cleared on turn"), "{text}");
    // Called by the name it gives itself; its files keep its file name.
    assert!(text.contains("A  Greedy  Dedale examples"), "{text}");
    let record = replay::read(&out.join("replay.json")).unwrap();
    assert_eq!(verify(&record), Ok(()));
    assert_eq!(record.result.reason, EndReason::Cleared);
    let player = &record.players[0];
    assert_eq!(player.name, "Greedy");
    assert_eq!(player.about.author.as_deref(), Some("Dedale examples"));
    assert!(player.about.description.is_some());
    assert!(out.join("greedy_bot.stderr.log").exists());
}

#[test]
fn a_sandboxed_run() {
    let missing = dedale(&[
        "run",
        "--memory-mib",
        "100",
        path(&example_bot("greedy_bot")),
    ]);
    assert!(!missing.status.success());
    assert!(
        stderr(&missing).contains("--sandbox"),
        "{}",
        stderr(&missing)
    );

    let python = common::python();
    if let Err(e) = Sandbox::new(Limits::default(), OsStr::new(&python)) {
        eprintln!("skipped: {e}");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out");
    let greedy = example_bot("greedy_bot");
    let output = dedale(&[
        "run",
        "--sandbox",
        path(&greedy),
        "--size",
        "11x11",
        "--seed",
        "7",
        "--out",
        path(&out),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("\nsandbox on, CPU "), "{text}");
    // Read through the sandbox, as the game was played.
    assert!(text.contains("A  Greedy"), "{text}");
    let record = replay::read(&out.join("replay.json")).unwrap();
    assert!(record.config.sandbox);
    assert!(record.players[0].cpu.is_some());
    assert_eq!(record.result.reason, EndReason::Cleared);
}

#[test]
fn watching_a_duo_from_a_map() {
    let dir = tempfile::tempdir().unwrap();
    let map = dir.path().join("ring.txt");
    fs::write(&map, "_....\n.###.\n.###.\n.###.\n...._\n").unwrap();
    let out = dir.path().join("out");
    let (greedy, random) = (example_bot("greedy_bot"), example_bot("random_bot"));
    let args = [
        "run",
        path(&greedy),
        path(&random),
        "--map",
        path(&map),
        "--watch",
        "--ascii",
        "--color",
        "never",
        "--out",
        path(&out),
    ];
    let output = dedale(&args);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("turn 0/100 - 14 points left\n"), "{text}");
    assert!(text.contains("turn 1/100"), "{text}");
    assert!(text.contains("A . . . .\n"), "{text}");
    assert!(text.is_ascii());
    let record = replay::read(&out.join("replay.json")).unwrap();
    assert_eq!(record.code, None);
    assert_eq!(verify(&record), Ok(()));
}

#[test]
fn problems_show_in_the_summary() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out");
    let bot = test_bot("raise_move");
    let output = dedale(&[
        "run",
        path(&bot),
        "--size",
        "7x7",
        "--max-turns",
        "5",
        "--out",
        path(&out),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("1 error"), "{}", stdout(&output));
}

/// The children of `pid`, from /proc.
fn children(pid: u32) -> Vec<u32> {
    fs::read_to_string(format!("/proc/{pid}/task/{pid}/children"))
        .unwrap_or_default()
        .split_whitespace()
        .map(|p| p.parse().unwrap())
        .collect()
}

fn alive(pid: u32) -> bool {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
    !stat.is_empty()
        && !stat
            .rsplit(')')
            .next()
            .unwrap_or("")
            .trim_start()
            .starts_with('Z')
}

#[test]
fn ctrl_c_stops_the_game_and_its_bots() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out");
    let mut child = Command::new(env!("CARGO_BIN_EXE_dedale"))
        .args([
            "run",
            path(&test_bot("infinite_loop")),
            path(&example_bot("random_bot")),
            "--max-turns",
            "100000",
            "--out",
            path(&out),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut bots = Vec::new();
    while bots.len() < 2 && Instant::now() < deadline {
        sleep(Duration::from_millis(50));
        bots = children(child.id());
    }
    assert_eq!(bots.len(), 2, "the bots never started");
    sleep(Duration::from_millis(500));

    let pid = Pid::from_raw(i32::try_from(child.id()).unwrap());
    kill(pid, Signal::SIGINT).unwrap();
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(130));
    let deadline = Instant::now() + Duration::from_secs(2);
    while bots.iter().any(|&b| alive(b)) && Instant::now() < deadline {
        sleep(Duration::from_millis(20));
    }
    assert!(bots.iter().all(|&b| !alive(b)), "a bot survived");
}
