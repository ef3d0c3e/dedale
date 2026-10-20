//! Arbitrary bytes through the bot message decoder.

#![no_main]

use dedale_core::protocol::{
    About, FromBot, MAX_DESCRIPTION, MAX_LABEL, MAX_LOG_BYTES, MAX_MARK_CELLS, MAX_MARKS, MAX_NAME,
    Mark, decode_bot_line, encode_line,
};
use libfuzzer_sys::fuzz_target;

fn check_caps(log: &[String], marks: &[Mark]) {
    assert!(log.iter().map(String::len).sum::<usize>() <= MAX_LOG_BYTES);
    assert!(marks.len() <= MAX_MARKS);
    assert!(marks.iter().map(|m| m.cells.len()).sum::<usize>() <= MAX_MARK_CELLS);
    for label in marks.iter().filter_map(|m| m.label.as_ref()) {
        assert!(label.len() <= MAX_LABEL);
    }
}

fn check_about(about: &About) {
    for (text, max) in [
        (&about.name, MAX_NAME),
        (&about.author, MAX_NAME),
        (&about.description, MAX_DESCRIPTION),
    ] {
        if let Some(text) = text {
            assert!(!text.is_empty() && text.len() <= max);
            assert_eq!(text.trim(), text);
            assert!(
                !text
                    .chars()
                    .any(|c| c.is_control() || (c.is_whitespace() && c != ' '))
            );
            assert!(!text.contains("  "));
        }
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok(msg) = decode_bot_line(data) else {
        return;
    };
    match &msg {
        FromBot::Hello(hello) => check_about(&hello.about),
        FromBot::Ready(ready) => check_caps(&ready.log, &ready.marks),
        FromBot::Error(reply) => check_caps(&reply.log, &[]),
        FromBot::Move(reply) => {
            check_caps(&reply.log, &reply.marks);
            let _ = reply.requested();
        }
    }
    // Sanitizing is idempotent, whatever the re-encoded length.
    if let Ok(again) = decode_bot_line(encode_line(&msg).as_bytes()) {
        assert_eq!(again, msg);
    }
});
