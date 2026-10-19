//! `dedale replay` on the golden replay (a fixed 12-turn duo).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/replay.json")
}

fn replay(args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_dedale"))
        .arg("replay")
        .args(args)
        .args(["--ascii", "--color", "never"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[test]
fn one_turn() {
    let golden = golden();
    let output = replay(
        &[golden.to_str().unwrap(), "--turn", "3", "--player", "b"],
        "",
    );
    assert!(output.status.success(), "{}", text(&output.stderr));
    let want = "\
turn 3/12 - 13 points left
A lap         3  right  ok         0 us
B flaky       1  -      error      0 us
      A .
. ######.
. . . . .
. ######B
. . . .
[B flaky] error, think 1 us
  error: scripted move error
    Traceback: scripted
";
    assert_eq!(text(&output.stdout), want);
}

#[test]
fn an_interactive_session() {
    let golden = golden();
    let output = replay(
        &[golden.to_str().unwrap()],
        "n\n2\nb\nG\np\nzz\n99\n?\ng\nq\n",
    );
    assert!(output.status.success(), "{}", text(&output.stderr));
    let out = text(&output.stdout);
    let views: Vec<&str> = out.split("> ").collect();
    // The start, then one view per command until q.
    assert_eq!(views.len(), 11, "{out}");
    assert!(views[0].starts_with("turn 0/12"));
    assert!(views[0].contains("[A lap] startup ok"));
    assert!(views[1].starts_with("turn 1/12"));
    assert!(views[1].contains("  log: on my way"));
    assert!(views[2].starts_with("turn 2/12"));
    assert!(views[3].contains("[B flaky] invalid"));
    assert!(views[3].contains("  invalid: unknown move \"north\""));
    assert!(views[4].starts_with("turn 12/12"));
    assert!(views[4].contains("result  turn limit reached after 12 turns: A lap wins, 9 to 2"));
    assert!(views[5].starts_with("turn 11/12"));
    assert!(views[6].contains("unknown command \"zz\""));
    assert!(views[7].contains("turn 99 is past the end (turn 12)"));
    assert!(views[8].contains("commands:"));
    assert!(views[9].starts_with("turn 0/12"));
    assert_eq!(views[10], "");
}

#[test]
fn end_of_input_quits() {
    let golden = golden();
    let output = replay(&[golden.to_str().unwrap()], "n\n");
    assert!(output.status.success());
    assert_eq!(text(&output.stdout).matches("> ").count(), 2);
}

#[test]
fn verification() {
    let golden = golden();
    let output = replay(&[golden.to_str().unwrap(), "--verify"], "");
    assert!(output.status.success(), "{}", text(&output.stderr));
    assert!(text(&output.stdout).ends_with("verified, 12 turns\n"));

    let dir = tempfile::tempdir().unwrap();
    let tampered = dir.path().join("tampered.json");
    let json = fs::read_to_string(&golden).unwrap();
    // Give player A a point it never collected on turn 1.
    let json = json.replacen("\"gained\": 2", "\"gained\": 3", 1);
    fs::write(&tampered, json).unwrap();
    let output = replay(&[tampered.to_str().unwrap(), "--verify"], "");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        text(&output.stderr).contains("turn 1: player 0: the points gained"),
        "{}",
        text(&output.stderr)
    );
}

#[test]
fn errors() {
    let golden = golden();
    let output = replay(&[golden.to_str().unwrap(), "--turn", "13"], "");
    assert_eq!(output.status.code(), Some(1));
    assert!(text(&output.stderr).contains("past the end"));
    let output = replay(&["/nonexistent/replay.json"], "");
    assert_eq!(output.status.code(), Some(1));
    assert!(text(&output.stderr).contains("cannot read"));
}
