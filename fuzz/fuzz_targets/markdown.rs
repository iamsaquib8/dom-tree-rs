#![no_main]
//! Fuzz target: the content serializers (`to_text`, `to_markdown`,
//! `inner_html`) must never panic on any parsed tree, and must not emit
//! trailing whitespace.
//!
//! Run with: `cargo +nightly fuzz run markdown`

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };

    let dom = domq::parse(s);

    let md = dom.to_markdown();
    assert_eq!(md, md.trim_end(), "to_markdown output is trimmed");
    let text = dom.to_text();
    assert_eq!(text, text.trim_end(), "to_text output is trimmed");

    // Subtree serialization from arbitrary starting points (capped so a large
    // input stays linear-ish).
    for node in dom.nodes().take(64) {
        let _ = node.inner_html();
        let _ = node.to_text();
        let _ = node.to_markdown();
    }
});
