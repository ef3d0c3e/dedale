//! A Python bot through the embedded harness and the referee.

use std::fs;
use std::time::Duration;

use dedale::bot::BotSpec;
use dedale::python::PythonRuntime;
use dedale::referee::{Entrant, MatchConfig, run_match};
use dedale_core::game::{EndReason, Outcome, Seating};
use dedale_core::maze::Maze;
use dedale_core::record::StartupOutcome;

const RING: &str = "_....\n.###.\n.###.\n.###.\n...._\n";

const LAP_BOT: &str = r#"
import dedale
from dedale import Move

LAP = [Move.RIGHT] * 4 + [Move.DOWN] * 4 + [Move.LEFT] * 4 + [Move.UP] * 4

def init():
    print("points:", len(dedale.points))

def move():
    dedale.mark([dedale.me.pos], label="me")
    return LAP[(dedale.turn - 1) % len(LAP)]
"#;

/// The interpreter: `DEDALE_PYTHON` or `python3`.
fn python() -> String {
    std::env::var("DEDALE_PYTHON").unwrap_or_else(|_| "python3".to_owned())
}

#[test]
fn a_python_bot_clears_the_ring() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lap.py");
    fs::write(&path, LAP_BOT).unwrap();
    let runtime = PythonRuntime::extract().unwrap();
    let spec = BotSpec::from_path(&path);
    let entrant = Entrant {
        name: spec.name(),
        path: path.display().to_string(),
        command: spec.command(&runtime.launcher(python())),
        stderr_log: Some(dir.path().join("lap.stderr.log")),
    };
    let mut config = MatchConfig::new(Seating::Solo);
    config.max_turns = Some(50);
    config.end_grace = Duration::from_secs(2);

    let maze = Maze::parse(RING).unwrap();
    let record = run_match(maze, None, vec![entrant], &config, |_, _| {}).unwrap();

    let startup = &record.players[0].startup;
    assert_eq!(startup.outcome, StartupOutcome::Ok, "{startup:?}");
    assert!(startup.init_us.is_some());
    assert_eq!(startup.log, vec!["points: 14".to_owned()]);
    assert_eq!(record.result.reason, EndReason::Cleared);
    assert_eq!(record.result.cleared_turn, Some(15));
    for turn in &record.turns {
        let p = &turn.players[0];
        assert_eq!(p.outcome, Outcome::Ok);
        assert!(p.think_us.is_some());
        assert_eq!(p.marks[0].label.as_deref(), Some("me"));
    }
    let exit = record.players[0].exit.as_ref().unwrap();
    assert_eq!((exit.code, exit.killed), (Some(0), false), "{exit:?}");
}
