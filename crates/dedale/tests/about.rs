//! What bots say about themselves, read before their games.

mod common;

use std::fs;
use std::time::{Duration, Instant};

use common::{example_bot, fake_bot_path, python};
use dedale::bot::{BotSpec, abouts};
use dedale::python::PythonRuntime;
use dedale_core::protocol::About;

const WAIT: Duration = Duration::from_secs(10);

#[test]
fn python_bots_are_read_without_running_them() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("ran");
    let bot = dir.path().join("bot.py");
    let source = format!("NAME = \"Wall\\x1b[2J\"\nAUTHOR = 'Ada'\nopen({marker:?}, 'w')\n");
    fs::write(&bot, source).unwrap();
    let runtime = PythonRuntime::extract().unwrap();
    let about = BotSpec::from_path(&bot).about(&runtime.launcher(python()), WAIT);
    // Cleaned: the escape sequence cannot reach a terminal.
    let expected = About {
        name: Some("Wall [2J".to_owned()),
        author: Some("Ada".to_owned()),
        description: None,
    };
    assert_eq!(about, expected);
    assert!(!marker.exists());
}

#[test]
fn example_bots_say_who_they_are() {
    let names = [
        "greedy_bot",
        "random_bot",
        "tour_bot",
        "rollout_bot",
        "combo_bot",
        "score_bot",
    ];
    let specs: Vec<_> = names
        .iter()
        .map(|name| BotSpec::from_path(example_bot(name)))
        .collect();
    let runtime = PythonRuntime::extract().unwrap();
    for (name, about) in names
        .iter()
        .zip(abouts(&specs, &runtime.launcher(python()), WAIT))
    {
        assert!(about.name.is_some(), "{name}");
        assert_eq!(about.author.as_deref(), Some("Dedale examples"), "{name}");
        assert!(about.description.is_some(), "{name}");
    }
}

#[test]
fn bots_that_say_nothing_have_nothing_said() {
    let specs = [
        // A plain hello.
        BotSpec::from_path(fake_bot_path()),
        // Silent until stopped.
        BotSpec::from_path("/bin/cat"),
        // Gone at once.
        BotSpec::from_path("/bin/true"),
        BotSpec::from_path("/nonexistent/bot"),
    ];
    let runtime = PythonRuntime::extract().unwrap();
    let started = Instant::now();
    let found = abouts(
        &specs,
        &runtime.launcher(python()),
        Duration::from_millis(500),
    );
    assert_eq!(found, vec![About::default(); specs.len()]);
    // Read all at once: only the silent bot waits, and only until the
    // timeout.
    assert!(started.elapsed() < Duration::from_secs(5));
}
