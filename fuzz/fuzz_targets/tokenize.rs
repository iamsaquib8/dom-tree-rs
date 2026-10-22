#![no_main]
//! Fuzz target: the standalone tokenizer must never panic and must always
//! terminate on arbitrary input.
//!
//! Run with: `cargo +nightly fuzz run tokenize`

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // Bound iterations defensively; the tokenizer should always terminate
        // because every token consumes at least one byte.
        let mut count = 0usize;
        for _ in domq::tokenize(s) {
            count += 1;
            assert!(count <= s.len() + 1, "tokenizer must make progress");
        }
    }
});
