//! Arbitrary bytes through the bot message decoder.

#![no_main]

use dedale_core::protocol::{
    FromBot, MAX_LABEL, MAX_LOG_BYTES, MAX_MARK_CELLS, MAX_MARKS, Mark, decode_bot_line,
    encode_line,
};
use libfuzzer_sys::fuzz_target;

fn check_caps(log: &[String], marks: &[Mark]) {
    assert!(log.iter().map(String::len).sum::<usize>() <= MAX_LOG_BYTES);
    assert!(marks.len() <= MAX_MARKS);
    assert!(marks.iter().map(|m| m.cells.len()).sum::<usize>() <= MAX_MARK_CELLS);
    for mark in marks {
        for text in [&mark.label, &mark.color].into_iter().flatten() {
            assert!(text.len() <= MAX_LABEL);
        }
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok(msg) = decode_bot_line(data) else {
        return;
    };
    match &msg {
        FromBot::Hello(_) => {}
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
