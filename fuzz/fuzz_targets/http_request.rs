//! Arbitrary bytes through the local server's request parser.

#![no_main]

use std::io::Read;

use dedale::web::http::{HttpError, MAX_BODY, MAX_HEAD, read_request};
use libfuzzer_sys::fuzz_target;

/// Counts the bytes the parser takes.
struct Counting<'a> {
    data: &'a [u8],
    taken: usize,
}

impl Read for Counting<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.data.read(buf)?;
        self.taken += n;
        Ok(n)
    }
}

fuzz_target!(|data: &[u8]| {
    let mut input = Counting { data, taken: 0 };
    let result = read_request(&mut input);
    // Never more than the limits, give or take one read.
    assert!(input.taken <= MAX_HEAD + MAX_BODY + 1024);
    let Ok(request) = result else {
        return;
    };
    assert!(request.body.len() <= MAX_BODY);
    match read_request(&mut request.encode().as_slice()) {
        Ok(again) => assert_eq!(again, request),
        // A space after each header colon can grow a head at the limit.
        Err(HttpError::HeadTooLarge) => {}
        Err(e) => panic!("the re-encoded request is refused: {e}"),
    }
});
