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
    let source =
        format!("NAME = \"Wall\\x1b[2J\"\nAUTHOR = 'Ada'\nVERSION = 4\nopen({marker:?}, 'w')\n");
    fs::write(&bot, source).unwrap();
    let runtime = PythonRuntime::extract().unwrap();
    let about = BotSpec::from_path(&bot).about(&runtime.launcher(python()), WAIT);
    // Cleaned: the escape sequence cannot reach a terminal.
    let expected = About {
        name: Some("Wall [2J".to_owned()),
        author: Some("Ada".to_owned()),
        description: None,
        version: Some(4),
    };
    assert_eq!(about, Some(expected));
    assert!(!marker.exists());
}

#[test]
fn example_bots_say_who_they_are() {
    let names = ["greedy_bot", "tour_bot", "rollout_bot"];
    let specs: Vec<_> = names
        .iter()
        .map(|name| BotSpec::from_path(example_bot(name)))
        .collect();
    let runtime = PythonRuntime::extract().unwrap();
    for (name, about) in names
        .iter()
        .zip(abouts(&specs, &runtime.launcher(python()), WAIT))
    {
        let about = about.unwrap_or_else(|| panic!("{name} said no hello"));
        assert!(about.name.is_some(), "{name}");
        assert_eq!(about.author.as_deref(), Some("Dedale examples"), "{name}");
        assert!(about.description.is_some(), "{name}");
        assert_eq!(about.version, Some(1), "{name}");
    }
}

#[test]
fn bots_that_say_nothing_are_told_apart_from_silent_ones() {
    let specs = [
        // A plain hello: it said nothing about itself.
        BotSpec::from_path(fake_bot_path()),
        // No hello at all. Silent until stopped.
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
    assert_eq!(found, [Some(About::default()), None, None, None]);
    // Read all at once: only the silent bot waits, and only until the
    // timeout.
    assert!(started.elapsed() < Duration::from_secs(5));
}
