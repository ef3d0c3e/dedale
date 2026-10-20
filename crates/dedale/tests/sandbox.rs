//! Bots in the sandbox: what a hostile bot can and cannot do.
//! Skipped, with a note, where the sandbox cannot run.

mod common;

use std::ffi::OsStr;
use std::fs;
use std::time::Duration;

use common::{example_bot, fake_bot_path, python};
use dedale::bot::BotSpec;
use dedale::python::PythonRuntime;
use dedale::referee::{Entrant, MatchConfig, run_match};
use dedale::sandbox::{Limits, Sandbox, cores};
use dedale_core::code::MazeCode;
use dedale_core::game::{EndReason, Seating};
use dedale_core::generator::generate_normalized;
use dedale_core::protocol::About;
use dedale_core::record::{GameRecord, StartupOutcome};

/// Tries everything a bot should not do, and says how each attempt went.
const HOSTILE: &str = r#"
import os
import socket
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent


def attempt(name, action):
    try:
        result = action()
    except BaseException as e:
        print(f"{name}: refused ({type(e).__name__})")
    else:
        print(f"{name}: allowed ({result})")


def forks():
    children = []
    try:
        for _ in range(100):
            pid = os.fork()
            if pid == 0:
                time.sleep(10)
                os._exit(0)
            children.append(pid)
    except OSError:
        pass
    for pid in children:
        os.kill(pid, 9)
        os.waitpid(pid, 0)
    return len(children)


def init():
    attempt("network", lambda: socket.create_connection(("1.1.1.1", 53), timeout=1).close())
    attempt("own dir", lambda: (HERE / "written").write_text("x"))
    attempt("tmp", lambda: Path("/tmp/scratch").write_text("x"))
    attempt("secret file", lambda: Path(SECRET_FILE).read_text())
    print("secret env:", "DEDALE_TEST_SECRET" in os.environ)
    attempt("memory", lambda: len(bytearray(1 << 30)))
    attempt("big file", lambda: Path("/tmp/big").write_bytes(b"x" * (100 << 20)))
    print("cpus:", sorted(os.sched_getaffinity(0)))
    print("forks:", forks())


def move():
    return "stay"
"#;

fn sandbox(limits: Limits) -> Option<Sandbox> {
    match Sandbox::new(limits, OsStr::new(&python())) {
        Ok(sandbox) => Some(sandbox),
        Err(e) => {
            common::no_sandbox(&e);
            None
        }
    }
}

fn game(entrants: Vec<Entrant>, seating: Seating) -> GameRecord {
    let code: MazeCode = "ded2-7x7-l25-p65535-0000000000000003".parse().unwrap();
    let (maze, code) = generate_normalized(&code);
    let mut config = MatchConfig::new(seating);
    config.startup = Duration::from_secs(5);
    config.max_turns = Some(20);
    config.end_grace = Duration::from_millis(500);
    config.sandbox = true;
    run_match(maze, Some(code), entrants, &config, &mut ()).unwrap()
}

#[test]
fn a_hostile_bot_is_contained() {
    let limits = Limits {
        memory_mib: 512,
        processes: 16,
        ..Limits::default()
    };
    let Some(sandbox) = sandbox(limits) else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let secret = dir.path().join("secret.txt");
    fs::write(&secret, "hunter2").unwrap();
    let bots = dir.path().join("bots");
    fs::create_dir(&bots).unwrap();
    let bot = bots.join("hostile.py");
    fs::write(
        &bot,
        format!(
            "SECRET_FILE = {:?}\n{HOSTILE}",
            secret.display().to_string()
        ),
    )
    .unwrap();

    let runtime = PythonRuntime::extract().unwrap();
    let launcher = runtime.launcher(python()).sandboxed(Some(sandbox));
    let cpu = cores(1)[0];
    let mut command = BotSpec::from_path(&bot).command_on(&launcher, Some(cpu));
    // What a CI job would hold: the bot must not see it.
    command.env("DEDALE_TEST_SECRET", "hunter2");
    let entrant = Entrant {
        name: "hostile".to_owned(),
        path: bot.display().to_string(),
        about: About::default(),
        cpu: Some(cpu),
        command,
        stderr_log: None,
    };
    let record = game(vec![entrant], Seating::Solo);
    let player = &record.players[0];
    assert_eq!(player.startup.outcome, StartupOutcome::Ok, "{player:?}");
    let log = player.startup.log.join("\n");
    for line in [
        "network: refused",
        "own dir: refused",
        "tmp: allowed",
        "secret file: refused",
        "secret env: False",
        "memory: refused (MemoryError)",
        "big file: refused",
        &format!("cpus: [{cpu}]"),
    ] {
        assert!(log.contains(line), "{line:?} in:\n{log}");
    }
    let forks: usize = log
        .lines()
        .find_map(|l| l.strip_prefix("forks: "))
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("no fork count in:\n{log}"));
    assert!(forks < 16, "{forks} forks within a cap of 16 processes");
    assert!(!bots.join("written").exists());
    assert_eq!(player.cpu, Some(u32::try_from(cpu).unwrap()));
    assert!(record.config.sandbox);
}

#[test]
fn python_bots_and_programs_play_as_usual() {
    let Some(sandbox) = sandbox(Limits::default()) else {
        return;
    };
    let runtime = PythonRuntime::extract().unwrap();
    let launcher = runtime.launcher(python()).sandboxed(Some(sandbox));
    let greedy = BotSpec::from_path(example_bot("greedy_bot"));
    let fake = BotSpec::from_path(fake_bot_path());
    // Read through the sandbox too.
    let about = greedy.about(&launcher, Duration::from_secs(10));
    assert_eq!(about.name.as_deref(), Some("Greedy"));
    let cpus = cores(2);
    let entrants = [&greedy, &fake]
        .into_iter()
        .zip(&cpus)
        .map(|(spec, &cpu)| Entrant {
            name: spec.name(),
            path: spec.path().display().to_string(),
            about: About::default(),
            cpu: Some(cpu),
            command: spec.command_on(&launcher, Some(cpu)),
            stderr_log: None,
        })
        .collect();
    let record = game(entrants, Seating::Duo);
    for player in &record.players {
        assert_eq!(player.startup.outcome, StartupOutcome::Ok, "{player:?}");
    }
    assert!(record.result.half_points[0] > 0, "{:?}", record.result);
    assert_ne!(record.result.reason, EndReason::Abandoned);
}
