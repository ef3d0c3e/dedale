//! Arbitrary bytes through the replay reader, verification and rewinding.

#![no_main]

use dedale_core::record::GameRecord;
use dedale_core::verify::verify;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(record) = GameRecord::from_json(text) else {
        return;
    };
    let verified = verify(&record);
    let last = u32::try_from(record.turns.len()).unwrap_or(u32::MAX);
    for turn in [0, last / 2, last] {
        let state = record.state_at(turn).expect("turns up to the last exist");
        assert_eq!(state.turn, turn);
    }
    assert!(record.state_at(last.saturating_add(1)).is_none() || last == u32::MAX);
    let again = GameRecord::from_json(&record.to_json()).expect("written replays read back");
    assert_eq!(verify(&again), verified);
    assert_eq!(again, record);
});
