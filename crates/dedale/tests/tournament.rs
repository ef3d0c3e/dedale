//! Tournaments: the library with fake bots, the command with Python bots.

mod common;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

use common::{example_bot, fake_bot};
use dedale::referee::{Entrant, MatchConfig};
use dedale::replay;
use dedale::tournament::{Mode, TournamentReport, play, schedule, standings};
use dedale_core::game::Seating;
use dedale_core::maze::Maze;
use dedale_core::verify::verify;

const RING: &str = "_....\n.###.\n.###.\n.###.\n...._\n";
const LAP: &str = "right,right,right,right,down,down,down,down,left,left,left,left,up,up,up,up";

fn config() -> MatchConfig {
    let mut config = MatchConfig::new(Seating::Solo);
    config.startup = Duration::from_millis(500);
    config.max_turns = Some(40);
    config.end_grace = Duration::from_millis(300);
    config
}

/// Two fake bots: `lap` walks the ring, `sitter` never moves.
fn fake_entrant(bot: usize, dir: &Path) -> Entrant {
    let (name, args) = [("lap", vec!["--moves", LAP]), ("sitter", vec![])][bot].clone();
    Entrant {
        name: name.to_owned(),
        path: format!("fake:{name}"),
        command: fake_bot(&args),
        stderr_log: Some(dir.join(format!("{name}.stderr.log"))),
    }
}

#[test]
fn a_duo_tournament_with_fake_bots() {
    let dir = tempfile::tempdir().unwrap();
    let names = vec!["lap".to_owned(), "sitter".to_owned()];
    let plans = schedule(Mode::Duo, 2, 2, None, 1);
    let ring = Maze::parse(RING).unwrap();
    let mut seen = 0;
    let games = play(
        &plans,
        &names,
        |_| ring.clone(),
        fake_entrant,
        &config(),
        dir.path(),
        |_, _| seen += 1,
    )
    .unwrap();
    assert_eq!((games.len(), seen), (4, 4));
    for (plan, record) in &games {
        assert_eq!(record.config.seating, plan.seating);
        assert_eq!(verify(record), Ok(()));
    }
    let table = standings(Mode::Duo, &names, &games);
    assert_eq!(table[0].name, "lap");
    assert_eq!(table[0].wins, 4);
    assert_eq!(table[1].losses, 4);
    let first = dir.path().join("games/001-map-duo/replay.json");
    assert_eq!(replay::read(&first).unwrap(), games[0].1);
    assert!(
        dir.path()
            .join("games/002-map-duo-swapped/sitter.stderr.log")
            .exists()
    );
}

#[test]
fn a_solo_tournament_with_fake_bots() {
    let dir = tempfile::tempdir().unwrap();
    let names = vec!["lap".to_owned(), "sitter".to_owned()];
    let plans = schedule(Mode::Solo, 2, 2, None, 1);
    let ring = Maze::parse(RING).unwrap();
    let games = play(
        &plans,
        &names,
        |_| ring.clone(),
        fake_entrant,
        &config(),
        dir.path(),
        |_, _| {},
    )
    .unwrap();
    let table = standings(Mode::Solo, &names, &games);
    assert_eq!(table[0].name, "lap");
    assert_eq!((table[0].cleared, table[0].turns_to_clear), (2, 30));
    assert_eq!(table[1].cleared, 0);
}

fn dedale(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dedale"))
        .args(args)
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn tournament(out: &Path, extra: &[&str]) -> (Output, TournamentReport) {
    let greedy = example_bot("greedy_bot");
    let random = example_bot("random_bot");
    let mut args = vec![
        "tournament",
        greedy.to_str().unwrap(),
        random.to_str().unwrap(),
    ];
    args.extend([
        "--size",
        "9x9",
        "--seed",
        "1",
        "--out",
        out.to_str().unwrap(),
    ]);
    args.extend(extra);
    let output = dedale(&args);
    assert!(output.status.success(), "{}", text(&output.stderr));
    let json = fs::read_to_string(out.join("tournament.json")).unwrap();
    (output, serde_json::from_str(&json).unwrap())
}

#[test]
fn the_duo_command() {
    let dir = tempfile::tempdir().unwrap();
    let (output, report) = tournament(&dir.path().join("a"), &["--games", "2"]);
    let stdout = text(&output.stdout);
    assert!(
        stdout.starts_with("tournament  duo, 2 mazes, 4 games, master seed 0000000000000001\n")
    );
    assert_eq!(stdout.matches("\ngame ").count(), 4, "{stdout}");
    assert_eq!(report.mode, Mode::Duo);
    assert_eq!(report.games.len(), 4);
    assert_eq!(report.leaderboard[0].name, "greedy_bot");
    for game in &report.games {
        let record = replay::read(&dir.path().join("a").join(&game.replay)).unwrap();
        assert_eq!(verify(&record), Ok(()));
        assert_eq!(record.result, game.result);
    }

    // The same master seed draws the same mazes.
    let (_, again) = tournament(&dir.path().join("b"), &["--games", "2"]);
    let codes = |r: &TournamentReport| r.games.iter().map(|g| g.plan.code).collect::<Vec<_>>();
    assert_eq!(codes(&again), codes(&report));
}

#[test]
fn the_solo_command() {
    let dir = tempfile::tempdir().unwrap();
    let (output, report) = tournament(
        dir.path(),
        &["--solo", "--games", "2", "--max-turns", "300"],
    );
    assert!(text(&output.stdout).contains("cleared"));
    assert_eq!(report.mode, Mode::Solo);
    assert_eq!(report.games.len(), 4);
    let first = &report.leaderboard[0];
    assert_eq!((first.name.as_str(), first.cleared), ("greedy_bot", 2));
}

#[test]
fn bad_tournaments_are_rejected() {
    let bot = example_bot("random_bot");
    let bot = bot.to_str().unwrap();
    let output = dedale(&["tournament", bot, bot, bot, "--games", "1"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(text(&output.stderr).contains("add --solo"));
    let code = "ded2-9x9-l25-p100-0000000000000001";
    let output = dedale(&["tournament", bot, "--maze", code]);
    assert_eq!(output.status.code(), Some(1));
    assert!(text(&output.stderr).contains("--seed"));
}
