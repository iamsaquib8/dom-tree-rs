#![no_main]
//! Fuzz target: parsing arbitrary bytes must never panic, and the resulting
//! tree must satisfy the arena invariants.
//!
//! Run with: `cargo +nightly fuzz run parse`

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };

    let dom = domq::parse(s);

    // Serializing must also never panic (it's iterative, so deep input is fine).
    let _ = dom.to_json();

    // Every node is reachable from the roots exactly once: no orphans, no
    // duplicates, no cycles (descendants() terminates).
    let mut reachable = 0usize;
    for root in dom.roots() {
        reachable += 1 + root.descendants().count();
    }
    assert_eq!(reachable, dom.len(), "arena reachability invariant");

    // Child/parent links agree.
    for node in dom.nodes() {
        for child in node.children() {
            assert_eq!(child.parent().map(|p| p.id()), Some(node.id()));
        }
    }

    // The selector engine must also be panic-free on arbitrary trees.
    let _ = dom.select("div > p.x, #id [data-y=z]").count();
});
