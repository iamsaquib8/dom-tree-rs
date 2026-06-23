//! Snapshot tests over a corpus of real-world-messy HTML fixtures.
//!
//! Snapshots live in `tests/snapshots/`. Regenerate with
//! `INSTA_UPDATE=always cargo test --test corpus` and review the diff.
#![cfg(feature = "json")]

use domtree::parse;

macro_rules! corpus_snapshot {
    ($name:ident, $file:literal) => {
        #[test]
        fn $name() {
            let html = include_str!(concat!("corpus/", $file));
            let dom = parse(html);
            insta::assert_snapshot!(dom.to_json());
        }
    };
}

corpus_snapshot!(basic, "basic.html");
corpus_snapshot!(messy, "messy.html");
corpus_snapshot!(document, "document.html");
