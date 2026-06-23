//! Property tests: the parser must never panic and must always produce a
//! structurally consistent tree, for arbitrary input.

use domtree::parse;
use proptest::prelude::*;

/// HTML-ish soup drawn from a small alphabet of structural characters — far
/// more likely to exercise tokenizer/builder edge cases than random text.
fn html_soup() -> impl Strategy<Value = String> {
    let alphabet = vec![
        '<', '>', '/', '=', '"', '\'', ' ', '\n', '!', '-', '?', '&', ';', '#', '[', ']', 'a',
        'b', 'p', 'l', 'i', '1', '\u{0}', '🦀',
    ];
    proptest::collection::vec(proptest::sample::select(alphabet), 0..300)
        .prop_map(|v| v.into_iter().collect())
}

proptest! {
    // Run a generous number of cases — this is our primary local guard for the
    // "never panics" promise (the cargo-fuzz harness in fuzz/ adds coverage-
    // guided fuzzing on top, in CI / for contributors).
    #![proptest_config(ProptestConfig { cases: 4096, ..ProptestConfig::default() })]

    #[test]
    fn parse_never_panics(s in html_soup()) {
        let dom = parse(&s);
        let _ = dom.to_json();
    }

    #[test]
    fn parse_never_panics_on_text(s in ".{0,300}") {
        let _ = parse(&s);
    }

    #[test]
    fn tree_is_consistent(s in html_soup()) {
        let dom = parse(&s);

        // Every node is reachable from the roots exactly once: the count of
        // roots plus all their descendants equals the arena size (no orphans,
        // no duplicates, no cycles — descendants() terminates).
        let mut reachable = 0usize;
        for r in dom.roots() {
            reachable += 1 + r.descendants().count();
        }
        prop_assert_eq!(reachable, dom.len());

        // Child/parent links agree.
        for node in dom.nodes() {
            for child in node.children() {
                prop_assert_eq!(child.parent().map(|p| p.id()), Some(node.id()));
            }
        }
    }
}
