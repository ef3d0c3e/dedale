//! Bot processes against the scripted fake bot.

mod common;

use std::fs;
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

use common::fake_bot;
use dedale::process::{BotProcess, Output, STDERR_LOG_CAP, STDERR_TAIL};
use dedale_core::game::{EndReason, PlayerState};
use dedale_core::grid::Pos;
use dedale_core::protocol::{
    DecodeError, End, FromBot, Init, InitPlayer, ToBot, Turn, decode_bot_line,
};
use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::unistd::Pid;

const SECOND: Duration = Duration::from_secs(1);

fn spawn(args: &[&str]) -> BotProcess {
    BotProcess::spawn(fake_bot(args), None).unwrap()
}

fn line(bot: &BotProcess) -> Vec<u8> {
    match bot.recv_until(Instant::now() + 5 * SECOND) {
        Some(Output::Line { line, .. }) => line,
        other => panic!("expected a line, got {other:?}"),
    }
}

fn message(bot: &BotProcess) -> FromBot {
    decode_bot_line(&line(bot)).unwrap()
}

fn init() -> ToBot {
    ToBot::Init(Init {
        protocol: 1,
        code: None,
        width: 5,
        height: 5,
        cells: "_.....###.......###....._".to_owned(),
        you: 0,
        players: vec![InitPlayer {
            pos: Pos::new(0, 0),
        }],
        startup_ms: 1000,
        move_ms: 100,
        max_turns: 100,
    })
}

fn turn(turn: u32) -> ToBot {
    ToBot::Turn(Turn {
        turn,
        players: vec![PlayerState {
            pos: Pos::new(0, 0),
            half_points: 0,
            last_move: None,
        }],
        collected: vec![],
    })
}

fn end() -> ToBot {
    ToBot::End(End {
        reason: EndReason::Cleared,
        turn: 1,
        half_points: vec![0],
    })
}

/// Waits for the hello, sends `init` and waits for `ready`.
fn start(bot: &BotProcess) {
    assert!(matches!(message(bot), FromBot::Hello(_)));
    bot.send(&init()).unwrap();
    assert!(matches!(message(bot), FromBot::Ready(_)));
}

fn move_of(msg: &FromBot) -> (u32, Option<&str>) {
    match msg {
        FromBot::Move(reply) => (reply.turn, reply.mv.as_deref()),
        other => panic!("expected a move, got {other:?}"),
    }
}

/// True once `pid` names no process, zombies included.
fn gone(pid: u32) -> bool {
    let pid = Pid::from_raw(i32::try_from(pid).unwrap());
    let deadline = Instant::now() + 2 * SECOND;
    loop {
        let zombie = fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
            stat.rsplit(')')
                .next()
                .is_some_and(|s| s.trim_start().starts_with('Z'))
        });
        if kill(pid, None) == Err(Errno::ESRCH) || zombie {
            return true;
        }
        if Instant::now() > deadline {
            return false;
        }
        sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_whole_conversation() {
    let bot = spawn(&["--moves", "up,left"]);
    start(&bot);
    for (t, want) in [(1, "up"), (2, "left"), (3, "up")] {
        bot.send(&turn(t)).unwrap();
        assert_eq!(move_of(&message(&bot)), (t, Some(want)));
    }
    bot.send(&end()).unwrap();
    assert!(matches!(
        bot.recv_until(Instant::now() + 5 * SECOND),
        Some(Output::Closed { .. })
    ));
    let report = bot.finish(5 * SECOND);
    assert!(report.status.unwrap().success());
    assert!(!report.killed);
}

#[test]
fn deadlines_are_respected() {
    let bot = spawn(&["--sleep-at", "1:400"]);
    start(&bot);
    bot.send(&turn(1)).unwrap();
    let sent = Instant::now();
    assert_eq!(bot.recv_until(sent + Duration::from_millis(100)), None);
    let waited = sent.elapsed();
    assert!(waited >= Duration::from_millis(100), "{waited:?}");
    assert!(waited < Duration::from_millis(350), "{waited:?}");
    // The answer still arrives, late.
    assert_eq!(move_of(&message(&bot)), (1, Some("stay")));
}

#[test]
fn a_crash_closes_stdout() {
    let bot = spawn(&["--crash-at", "1"]);
    start(&bot);
    bot.send(&turn(1)).unwrap();
    assert!(matches!(
        bot.recv_until(Instant::now() + 5 * SECOND),
        Some(Output::Closed { .. })
    ));
    let report = bot.finish(5 * SECOND);
    assert_eq!(report.status.unwrap().code(), Some(3));
    assert!(!report.killed);
}

#[test]
fn oversized_lines_are_dropped() {
    let bot = spawn(&["--oversized-at", "1"]);
    start(&bot);
    bot.send(&turn(1)).unwrap();
    assert!(matches!(
        bot.recv_until(Instant::now() + 5 * SECOND),
        Some(Output::Oversized { .. })
    ));
    bot.send(&turn(2)).unwrap();
    assert_eq!(move_of(&message(&bot)), (2, Some("stay")));
}

#[test]
fn garbage_is_delivered_as_is() {
    let bot = spawn(&["--garbage-at", "1"]);
    start(&bot);
    bot.send(&turn(1)).unwrap();
    assert!(matches!(
        decode_bot_line(&line(&bot)),
        Err(DecodeError::Json(_))
    ));
}

#[test]
fn stderr_goes_to_the_log_and_the_tail() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("bot.stderr.log");
    let bot = BotProcess::spawn(fake_bot(&["--stderr", "hello from stderr"]), Some(&log)).unwrap();
    start(&bot);
    bot.send(&end()).unwrap();
    let report = bot.finish(5 * SECOND);
    assert!(report.stderr_tail.contains("hello from stderr"));
    assert!(
        fs::read_to_string(&log)
            .unwrap()
            .contains("hello from stderr")
    );
}

#[test]
fn the_stderr_log_is_capped_without_blocking_the_bot() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("bot.stderr.log");
    let bytes = (STDERR_LOG_CAP + (1 << 20)).to_string();
    let bot = BotProcess::spawn(fake_bot(&["--stderr-bytes", &bytes]), Some(&log)).unwrap();
    start(&bot);
    bot.send(&end()).unwrap();
    let report = bot.finish(5 * SECOND);
    assert!(!report.killed);
    assert_eq!(report.stderr_tail.len(), STDERR_TAIL);
    let logged = fs::read(&log).unwrap();
    assert!(logged.starts_with(&vec![b'e'; STDERR_LOG_CAP]));
    assert!(logged.ends_with(b"\n[dedale: stderr log truncated]\n"));
    assert!(logged.len() < STDERR_LOG_CAP + 64);
}

#[test]
fn a_bot_ignoring_end_is_killed_after_the_grace_period() {
    let bot = spawn(&["--ignore-end"]);
    start(&bot);
    bot.send(&end()).unwrap();
    let pid = bot.id();
    let started = Instant::now();
    let report = bot.finish(Duration::from_millis(200));
    let took = started.elapsed();
    assert!(report.killed);
    assert!(
        took >= Duration::from_millis(200) && took < 2 * SECOND,
        "{took:?}"
    );
    assert!(gone(pid));
}

#[test]
fn dropping_kills_the_bot() {
    let bot = spawn(&[]);
    assert!(matches!(message(&bot), FromBot::Hello(_)));
    let pid = bot.id();
    drop(bot);
    assert!(gone(pid));
}

#[test]
fn the_whole_process_group_is_killed() {
    // The shell prints the pid of a child it leaves running.
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "sleep 1000 & echo $!; wait"]);
    let bot = BotProcess::spawn(command, None).unwrap();
    let grandchild: u32 = String::from_utf8(line(&bot))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let report = bot.finish(Duration::from_millis(50));
    assert!(report.killed);
    assert!(gone(grandchild));
}

#[test]
fn sending_after_closing_stdin_fails() {
    let mut bot = spawn(&[]);
    bot.close_stdin();
    assert!(bot.send(&init()).is_err());
    // With stdin closed, the bot reads EOF and exits by itself.
    assert!(!bot.finish(5 * SECOND).killed);
}

#[test]
fn a_missing_program_fails_to_spawn() {
    assert!(BotProcess::spawn(Command::new("/nonexistent/bot"), None).is_err());
}
