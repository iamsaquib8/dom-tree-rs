//! Property and robustness tests for the structure-and-data APIs: links,
//! resources, metadata, tables, main content, URL resolution and Markdown
//! chunking must never panic, keep their promises, and stay within bounded
//! stack and work on hostile input.

use domtree::{Dom, parse, resolve_srcset, resolve_url, to_csv};
use proptest::prelude::*;

/// Markup soup biased towards what the extractors look at.
fn html_soup() -> impl Strategy<Value = String> {
    let pieces = vec![
        "<html lang=en>",
        "<head>",
        "<title>",
        "</title>",
        "<base href='/b/'>",
        "<base href='https://x.dev/a/'>",
        "<meta name=description content='d, e'>",
        "<meta property=og:title content=T>",
        "<meta property=og:image content=/i.png>",
        "<meta name=twitter:card content=c>",
        "<meta name=keywords content='a,,b'>",
        "<link rel=canonical href=/c>",
        "<link rel=alternate type=application/rss+xml href=/f>",
        "<script type=application/ld+json>",
        "{\"@type\":\"X\"}",
        "</script>",
        "<svg>",
        "</svg>",
        "<body>",
        "<main>",
        "</main>",
        "<article>",
        "</article>",
        "<nav>",
        "</nav>",
        "<aside>",
        "<header>",
        "<footer>",
        "<div class=comments>",
        "<div class=post-content>",
        "<div role=main>",
        "<div hidden>",
        "<div style='display:none'>",
        "<div>",
        "</div>",
        "<section>",
        "<p>",
        "</p>",
        "<a href=x.html>",
        "<a href='javascript:x()'>",
        "<a href=#top rel=author>",
        "<a>",
        "</a>",
        "<area href=/m alt=m>",
        "<img src=i.png alt=alt srcset='a 1x, b,c 2x,'>",
        "<time datetime=2026>",
        "<table>",
        "</table>",
        "<thead>",
        "<tbody>",
        "<tfoot>",
        "<tr>",
        "</tr>",
        "<td>",
        "<td colspan=3>",
        "<td rowspan=0>",
        "<td rowspan=4 colspan=2>",
        "<th>",
        "</td>",
        "<caption>",
        "<ul>",
        "<li>",
        "</li>",
        "</ul>",
        "<blockquote>",
        "</blockquote>",
        "<pre>",
        "</pre>",
        "<code>",
        "</code>",
        "<h1>",
        "</h1>",
        "<h2>",
        "</h2>",
        "<h3>",
        "</h3>",
        "<b>",
        "</b>",
        "<br>",
        "<hr>",
        "This is a fairly long sentence, with commas, to pass the minimums. ",
        "text",
        "```",
        "# hash",
        "1. one",
        "- dash",
        "| pipe",
        "`",
        "*",
        "\\",
        "&amp;",
        "&nbsp;",
        " ",
        "\n",
        "\n\n",
        "\t",
        "🦀",
    ];
    proptest::collection::vec(proptest::sample::select(pieces), 0..120).prop_map(|v| v.concat())
}

/// Raw characters, as in `tests/properties.rs`.
fn char_soup() -> impl Strategy<Value = String> {
    let alphabet = vec![
        '<', '>', '/', '=', '"', '\'', ' ', '\n', '!', '-', '?', '&', ';', '#', '[', ']', 'a', 'b',
        'p', 't', 'd', 'r', '1', '\u{0}', '\u{a0}', '🦀', '`', ',',
    ];
    proptest::collection::vec(proptest::sample::select(alphabet), 0..300)
        .prop_map(|v| v.into_iter().collect())
}

/// URL-ish strings.
fn url_soup() -> impl Strategy<Value = String> {
    let pieces = vec![
        "http:", "https://", "//", "/", "./", "../", ".", "..", "a", "b", "?", "#", ":", "@", ",",
        " ", "\t", "\n", "1x", "%2e", "🦀", "g;x=1", "mailto:", "",
    ];
    proptest::collection::vec(proptest::sample::select(pieces), 0..16).prop_map(|v| v.concat())
}

/// Run every extractor over the document.
fn extract_all(dom: &Dom) {
    extract(dom, usize::MAX);
}

/// As [`extract_all`], but only the first `tables` tables: each nested table's
/// cell text contains every deeper table, so visiting all of them in a deep
/// nest is quadratic by construction.
fn extract(dom: &Dom, tables: usize) {
    let main = dom.main_content();
    if let Some(m) = main {
        let _ = m.to_text();
    }
    for base in [
        None,
        Some("https://ex.dev/p/q.html"),
        Some("rel/base"),
        Some(""),
    ] {
        let _ = dom.base_url(base);
        for link in dom.links(base) {
            assert!(link.node.tag_name().is_some());
            assert!(!link.text.contains(['\n', '\t']), "{:?}", link.text);
        }
        for r in dom.resources(base) {
            assert!(!r.value.trim().is_empty());
        }
    }
    let _ = dom.metadata();
    for table in dom.find_by_tag("table").take(tables) {
        let rows = table.table_rows();
        let _ = to_csv(&rows);
    }
}

/// `small` with whitespace removed is a subsequence of `big` with whitespace
/// removed: nothing was lost or reordered (repeated fences/headers allowed).
#[cfg(feature = "markdown")]
fn keeps_content(small: &str, big: &str) -> bool {
    let mut big = big.chars().filter(|c| !c.is_whitespace());
    small
        .chars()
        .filter(|c| !c.is_whitespace())
        .all(|c| big.any(|b| b == c))
}

#[cfg(feature = "markdown")]
fn check_chunks(dom: &Dom, max: usize) -> Result<(), TestCaseError> {
    let md = dom.to_markdown();
    let chunks = dom.to_markdown_chunks(max);
    for c in &chunks {
        prop_assert!(!c.markdown.trim().is_empty(), "blank chunk");
        prop_assert!(
            c.markdown.chars().count() <= max || !c.markdown.contains('\n'),
            "chunk over budget {}: {:?}",
            max,
            c.markdown
        );
        prop_assert!(c.heading_path.len() <= 6);
    }
    let joined: String = chunks
        .iter()
        .map(|c| c.markdown.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    prop_assert!(
        keeps_content(&md, &joined),
        "lost content\nmd: {:?}\nchunks: {:?}",
        md,
        chunks
    );
    if max >= md.chars().count() {
        // Nothing was split inside a block: the chunks are the Markdown.
        let exact: Vec<_> = chunks.iter().map(|c| c.markdown.as_str()).collect();
        prop_assert_eq!(exact.join("\n\n"), md);
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 1024, ..ProptestConfig::default() })]

    #[test]
    fn extractors_never_panic_on_markup(s in html_soup()) {
        extract_all(&parse(&s));
    }

    #[test]
    fn extractors_never_panic_on_chars(s in char_soup()) {
        extract_all(&parse(&s));
    }

    #[test]
    fn extractors_never_panic_on_any_text(s in ".{0,200}") {
        extract_all(&parse(&s));
    }

    #[cfg(feature = "markdown")]
    #[test]
    fn chunks_keep_their_promises(s in html_soup(), max in 0usize..300) {
        check_chunks(&parse(&s), max)?;
    }

    #[cfg(feature = "markdown")]
    #[test]
    fn chunks_never_panic_on_chars(s in char_soup(), max in 0usize..100) {
        check_chunks(&parse(&s), max)?;
    }

    #[test]
    fn url_resolution_never_panics(base in url_soup(), reference in url_soup()) {
        let out = resolve_url(&base, &reference);
        prop_assert!(!out.contains(['\t', '\n']));
        let _ = resolve_srcset(&base, &reference);
    }

    #[test]
    fn url_resolution_never_panics_on_any_text(base in ".{0,40}", reference in ".{0,40}") {
        let _ = resolve_url(&base, &reference);
        let _ = resolve_srcset(&base, &reference);
    }

    #[test]
    fn absolute_results_for_absolute_bases(path in "([a-z.][a-z./]{0,19})?") {
        // (Not starting with `//`, which would replace the authority.)
        let out = resolve_url("https://ex.dev/a/b", &path);
        prop_assert!(out.starts_with("https://ex.dev/") || out.starts_with("https://ex.dev?"), "{}", out);
    }

    #[test]
    fn csv_round_trips_field_count(fields in proptest::collection::vec(".{0,10}", 0..8)) {
        let csv = to_csv([&fields]);
        prop_assert!(csv.ends_with("\r\n"));
        // Quoted fields keep their commas; count the separators outside quotes.
        let mut quoted = false;
        let mut commas = 0;
        for c in csv.chars() {
            match c {
                '"' => quoted = !quoted,
                ',' if !quoted => commas += 1,
                _ => {}
            }
        }
        prop_assert_eq!(commas, fields.len().saturating_sub(1));
    }
}

// --- deep and wide input: bounded stack, linear work ------------------------

const DEPTH: usize = 100_000;

#[test]
fn deep_divs() {
    let mut html = "<div>".repeat(DEPTH);
    html.push_str("<p>A paragraph long enough, with a comma, to be scored as content.</p>");
    let dom = parse(&html);
    extract(&dom, 2);
    #[cfg(feature = "markdown")]
    assert_eq!(dom.to_markdown_chunks(10).len(), 1);
}

#[test]
fn deep_mixed_structures() {
    let chunk =
        "<div class=x><a href=/l><article><table><tr><td><h2>h</h2><img src=i srcset='a 1x'>";
    let dom = parse(&chunk.repeat(DEPTH / 10));
    extract(&dom, 2);
    #[cfg(feature = "markdown")]
    {
        let chunks = dom.to_markdown_chunks(50);
        assert!(!chunks.is_empty());
    }
}

#[test]
fn deep_headings_and_lists() {
    let dom = parse(&"<h1>a<ul><li>b<h2>c".repeat(DEPTH / 10));
    extract(&dom, 2);
    #[cfg(feature = "markdown")]
    let _ = dom.to_markdown_chunks(0);
}

#[cfg(feature = "markdown")]
#[test]
fn many_headings_stay_linear() {
    let html = "<h1>a</h1><h2>b</h2><h6>c</h6><p>d</p>".repeat(20_000);
    let dom = parse(&html);
    let chunks = dom.to_markdown_chunks(30);
    // `# a`, `## b`, `###### c` nest, so each repetition is one chunk.
    assert_eq!(chunks.len(), 20_000);
    assert!(chunks.iter().all(|c| c.heading_path.len() <= 3));
}

#[cfg(feature = "markdown")]
#[test]
fn huge_code_block_at_zero_budget() {
    let html = format!("<pre>{}</pre>", "x\n".repeat(50_000));
    let chunks = parse(&html).to_markdown_chunks(0);
    assert_eq!(chunks.len(), 50_002); // fence + 50,000 lines + fence
}
