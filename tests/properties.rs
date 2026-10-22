//! Property tests: the parser must never panic and must always produce a
//! structurally consistent tree, for arbitrary input.

use domq::parse;
use proptest::prelude::*;

/// HTML-ish soup drawn from a small alphabet of structural characters — far
/// more likely to exercise tokenizer/builder edge cases than random text.
fn html_soup() -> impl Strategy<Value = String> {
    let alphabet = vec![
        '<', '>', '/', '=', '"', '\'', ' ', '\n', '!', '-', '?', '&', ';', '#', '[', ']', 'a', 'b',
        'p', 'l', 'i', '1', '\u{0}', '🦀',
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

    #[test]
    fn source_ranges_are_valid_and_nested(s in html_soup()) {
        let dom = parse(&s);
        for node in dom.nodes() {
            // Every parsed node has a range, and it slices the input cleanly.
            let range = node.source_range();
            prop_assert!(range.is_some());
            let range = range.unwrap_or(0..0);
            prop_assert!(s.get(range.clone()).is_some(), "bad slice {:?}", range);
            // Never empty: elements start with `<`, empty text makes no node.
            prop_assert!(!range.is_empty());
            // A parent's range contains its children's, and siblings come in
            // source order without overlapping.
            let mut prev_end = range.start;
            for child in node.children() {
                let c = child.source_range().unwrap_or(0..0);
                prop_assert!(range.start <= c.start && c.end <= range.end,
                    "child {:?} outside parent {:?}", c, range);
                prop_assert!(prev_end <= c.start, "siblings overlap");
                prev_end = c.end;
            }
        }
        // Top-level nodes are in source order too.
        let mut prev_end = 0;
        for r in dom.roots() {
            let range = r.source_range().unwrap_or(0..0);
            prop_assert!(prev_end <= range.start);
            prev_end = range.end;
        }
    }

    #[test]
    fn line_col_never_panics(s in ".{0,64}", byte in 0usize..80) {
        let (line, col) = domq::line_col(&s, byte);
        prop_assert!(line >= 1 && col >= 1);
        prop_assert!(line <= s.len() + 1 && col <= s.chars().count() + 1);
    }
}
