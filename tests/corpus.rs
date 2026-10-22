//! Snapshot tests over a corpus of real-world-messy HTML fixtures.
//!
//! Each fixture is snapshotted twice: as JSON (the full tree, including
//! whitespace text and the error count) and re-serialized as HTML, which shows
//! at a glance where the parser closed elements implicitly.
//!
//! Snapshots live in `tests/snapshots/`. Regenerate with
//! `INSTA_UPDATE=always cargo test --test corpus` and review the diff.
#![cfg(feature = "json")]

use domq::parse;

macro_rules! corpus_snapshot {
    ($name:ident, $html_name:ident, $file:literal) => {
        #[test]
        fn $name() {
            let html = include_str!(concat!("corpus/", $file));
            let dom = parse(html);
            insta::assert_snapshot!(dom.to_json());
        }

        #[test]
        fn $html_name() {
            let html = include_str!(concat!("corpus/", $file));
            insta::assert_snapshot!(parse(html).to_html());
        }
    };
}

corpus_snapshot!(basic, html_basic, "basic.html");
corpus_snapshot!(messy, html_messy, "messy.html");
corpus_snapshot!(document, html_document, "document.html");
corpus_snapshot!(tables, html_tables, "tables.html");
corpus_snapshot!(ruby, html_ruby, "ruby.html");
corpus_snapshot!(select, html_select, "select.html");
corpus_snapshot!(omitted, html_omitted, "omitted.html");
