#![no_main]
//! Fuzz target: any selector against any document must never panic, and the
//! `&str` and pre-parsed `&Selector` paths must agree.
//!
//! Input: `<selector>\n<html>` (split on the first newline).
//! Run with: `cargo +nightly fuzz run select`

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let (selector, html) = s.split_once('\n').unwrap_or((s, ""));
    let dom = domtree::parse(html);

    let by_str: Vec<_> = dom.select(selector).map(|n| n.id()).collect();
    match domtree::Selector::parse(selector) {
        Ok(parsed) => {
            let by_parsed: Vec<_> = dom.select(&parsed).map(|n| n.id()).collect();
            assert_eq!(by_str, by_parsed, "&str and &Selector disagree");
            for node in dom.nodes() {
                let _ = node.closest(&parsed);
                let _ = node.select(&parsed).count();
            }
        }
        Err(e) => {
            assert!(by_str.is_empty(), "invalid selector matched");
            assert!(selector.is_char_boundary(e.position()));
        }
    }
});
