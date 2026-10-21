//! The streaming API (`domtree::events`): the same recovery as the tree
//! builder, without the tree. The central guarantee, checked on arbitrary
//! input: replaying the events into a tree gives exactly the tree `parse`
//! builds — same nodes, attributes, text, source ranges, `to_html()` and
//! diagnostics — and `End` events always balance `Start` events.

use std::borrow::Cow;
use std::ops::Range;

use domtree::{Event, NodeKind, NodeRef, parse};
use proptest::prelude::*;

// --- a minimal tree, built only from events ---------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Element {
        name: String,
        attrs: Vec<(String, String)>,
    },
    Text(String),
    Comment(String),
    Doctype(String),
}

#[derive(Debug)]
struct Node {
    kind: Kind,
    range: Range<usize>,
    children: Vec<usize>,
}

#[derive(Debug, Default)]
struct Replay {
    nodes: Vec<Node>,
    roots: Vec<usize>,
}

/// Replay `html`'s events into a [`Replay`] tree, checking on the way that
/// every `End` closes the innermost open `Start` of the same name.
fn replay(html: &str) -> Result<Replay, TestCaseError> {
    let mut tree = Replay::default();
    let mut open: Vec<usize> = Vec::new();
    let add = |tree: &mut Replay, open: &[usize], kind: Kind, range: Range<usize>| {
        let id = tree.nodes.len();
        tree.nodes.push(Node {
            kind,
            range,
            children: Vec::new(),
        });
        match open.last() {
            Some(&p) => tree.nodes[p].children.push(id),
            None => tree.roots.push(id),
        }
        id
    };
    for (event, range) in domtree::events(html) {
        prop_assert!(range.start <= range.end && range.end <= html.len());
        prop_assert!(
            html.get(range.clone()).is_some(),
            "range {:?} not on char boundaries",
            range
        );
        match event {
            Event::Start { name, attrs } => {
                let attrs = attrs
                    .into_iter()
                    .map(|(k, v)| (k.into_owned(), v.into_owned()))
                    .collect();
                let kind = Kind::Element {
                    name: name.into_owned(),
                    attrs,
                };
                let id = add(&mut tree, &open, kind, range);
                open.push(id);
            }
            Event::End { name, implied } => {
                let Some(id) = open.pop() else {
                    return Err(TestCaseError::fail(format!("unbalanced End {name}")));
                };
                let node = &mut tree.nodes[id];
                match &node.kind {
                    Kind::Element {
                        name: open_name, ..
                    } => prop_assert_eq!(open_name, &name),
                    _ => unreachable!("only elements are opened"),
                }
                if implied {
                    prop_assert!(range.is_empty(), "implied End with range {:?}", range);
                } else {
                    prop_assert!(html[range.clone()].starts_with("</"));
                }
                node.range.end = range.end;
            }
            Event::Text(t) => {
                prop_assert!(!t.is_empty());
                add(&mut tree, &open, Kind::Text(t.into_owned()), range);
            }
            Event::Comment(c) => {
                add(&mut tree, &open, Kind::Comment(c.to_string()), range);
            }
            Event::Doctype(d) => {
                add(&mut tree, &open, Kind::Doctype(d.to_string()), range);
            }
        }
    }
    prop_assert!(open.is_empty(), "Start without End: {:?}", open);
    Ok(tree)
}

fn kind_of(n: NodeRef<'_>) -> Kind {
    match n.kind() {
        NodeKind::Element { tag } => Kind::Element {
            name: tag.to_string(),
            attrs: n
                .attributes()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        },
        NodeKind::Text(t) => Kind::Text(t.to_string()),
        NodeKind::Comment(c) => Kind::Comment(c.to_string()),
        NodeKind::Doctype(d) => Kind::Doctype(d.to_string()),
    }
}

/// The replayed tree and `parse`'s tree are the same, node for node.
fn assert_same_tree(html: &str, tree: &Replay) -> Result<(), TestCaseError> {
    let dom = parse(html);
    // Pre-order walks of both, side by side (explicit stacks: inputs can nest
    // deeply).
    let mut ours: Vec<(Kind, Option<Range<usize>>, usize)> = Vec::new();
    let mut stack: Vec<NodeRef<'_>> = dom.roots().collect();
    stack.reverse();
    while let Some(n) = stack.pop() {
        ours.push((kind_of(n), n.source_range(), n.children().count()));
        let mut kids: Vec<_> = n.children().collect();
        kids.reverse();
        stack.extend(kids);
    }
    let mut theirs: Vec<(Kind, Option<Range<usize>>, usize)> = Vec::new();
    let mut stack: Vec<usize> = tree.roots.iter().rev().copied().collect();
    while let Some(id) = stack.pop() {
        let n = &tree.nodes[id];
        theirs.push((n.kind.clone(), Some(n.range.clone()), n.children.len()));
        stack.extend(n.children.iter().rev());
    }
    prop_assert_eq!(ours, theirs);
    Ok(())
}

// --- serializing the replayed tree with `to_html`'s rules -------------------

fn is_void(tag: &str) -> bool {
    matches!(
        tag,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

fn escape(out: &mut String, s: &str, attr: bool) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' if attr => out.push_str("&quot;"),
            '<' if !attr => out.push_str("&lt;"),
            '>' if !attr => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
}

fn to_html(tree: &Replay) -> String {
    enum Step {
        Open(usize, bool),
        Close(String),
    }
    let mut out = String::new();
    let mut stack: Vec<Step> = tree
        .roots
        .iter()
        .rev()
        .map(|&r| Step::Open(r, false))
        .collect();
    while let Some(step) = stack.pop() {
        match step {
            Step::Close(tag) => {
                out.push_str("</");
                out.push_str(&tag);
                out.push('>');
            }
            Step::Open(id, raw_parent) => match &tree.nodes[id].kind {
                Kind::Element { name, attrs } => {
                    out.push('<');
                    out.push_str(name);
                    for (k, v) in attrs {
                        out.push(' ');
                        out.push_str(k);
                        if !v.is_empty() {
                            out.push_str("=\"");
                            escape(&mut out, v, true);
                            out.push('"');
                        }
                    }
                    out.push('>');
                    if is_void(name) {
                        continue;
                    }
                    stack.push(Step::Close(name.clone()));
                    let raw = matches!(name.as_str(), "script" | "style");
                    for &c in tree.nodes[id].children.iter().rev() {
                        stack.push(Step::Open(c, raw));
                    }
                }
                Kind::Text(t) if raw_parent => out.push_str(t),
                Kind::Text(t) => escape(&mut out, t, false),
                Kind::Comment(c) => {
                    out.push_str("<!--");
                    out.push_str(c);
                    out.push_str("-->");
                }
                Kind::Doctype(d) => {
                    out.push_str("<!doctype ");
                    out.push_str(d);
                    out.push('>');
                }
            },
        }
    }
    out
}

/// Every guarantee at once, for one input.
fn check(html: &str) -> Result<(), TestCaseError> {
    let tree = replay(html)?;
    assert_same_tree(html, &tree)?;
    let dom = parse(html);
    prop_assert_eq!(to_html(&tree), dom.to_html());
    let mut events = domtree::events(html);
    for _ in events.by_ref() {}
    prop_assert_eq!(events.errors(), dom.errors());
    Ok(())
}

// --- property tests ---------------------------------------------------------

/// HTML-ish soup from a small alphabet of structural characters (as in
/// `tests/properties.rs`).
fn html_soup() -> impl Strategy<Value = String> {
    let alphabet = vec![
        '<', '>', '/', '=', '"', '\'', ' ', '\n', '!', '-', '?', '&', ';', '#', '[', ']', 'a', 'b',
        'p', 'l', 'i', '1', '\u{0}', '🦀',
    ];
    proptest::collection::vec(proptest::sample::select(alphabet), 0..300)
        .prop_map(|v| v.into_iter().collect())
}

/// Markup built from real tags, so implicit closes, void elements, raw text,
/// mismatched and stray end tags, entities and attributes all come up.
fn html_pieces() -> impl Strategy<Value = String> {
    let pieces = vec![
        "<div>",
        "</div>",
        "<DIV class=a>",
        "<p>",
        "</p>",
        "<li>",
        "<ul>",
        "</ul>",
        "<b>",
        "</b>",
        "</i>",
        "<br>",
        "<br/>",
        "<img src=x alt='a &amp; b'>",
        "<div/>",
        "<!--c-->",
        "<!doctype html>",
        "text",
        " ",
        "&amp;",
        "&bogus;",
        "&#169;",
        "<td>",
        "<tr>",
        "<table>",
        "</table>",
        "<script>if (a < b) x = '&amp;';</script>",
        "<style>p{}</style>",
        "<title>a &amp; b</title>",
        "<textarea>&lt;</textarea>",
        "<head>",
        "<meta charset=utf-8>",
        "</head>",
        "<colgroup>",
        "<col>",
        "<option>",
        "<select>",
        "<h1>",
        "<h2>",
        "<![CDATA[x<y]]>",
        "<?php ?>",
        "é",
        "<a href=\"/x?a=1&b=2\">",
        "</a>",
        "<p a=1 a=2 B=3>",
        "< x",
        "</>",
        "<x-y:z.w>",
    ];
    proptest::collection::vec(proptest::sample::select(pieces), 0..40).prop_map(|v| v.concat())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 2048, ..ProptestConfig::default() })]

    #[test]
    fn events_replay_to_the_parsed_tree_on_soup(s in html_soup()) {
        check(&s)?;
    }

    #[test]
    fn events_replay_to_the_parsed_tree_on_markup(s in html_pieces()) {
        check(&s)?;
    }

    #[test]
    fn events_never_panic_on_any_text(s in ".{0,200}") {
        check(&s)?;
    }
}

// --- focused cases ----------------------------------------------------------

fn collect(html: &str) -> Vec<(Event<'_>, Range<usize>)> {
    domtree::events(html).collect()
}

fn start<'a>(name: &'a str, attrs: &[(&'a str, &'a str)]) -> Event<'a> {
    Event::Start {
        name: Cow::Borrowed(name),
        attrs: attrs
            .iter()
            .map(|&(k, v)| (Cow::Borrowed(k), Cow::Borrowed(v)))
            .collect(),
    }
}

fn end(name: &str, implied: bool) -> Event<'_> {
    Event::End {
        name: Cow::Borrowed(name),
        implied,
    }
}

fn text(t: &str) -> Event<'_> {
    Event::Text(Cow::Borrowed(t))
}

#[test]
fn implicit_closes_void_elements_and_entities() {
    let html = "<ul><li>a &amp; b<li>c<br></ul>";
    assert_eq!(
        collect(html),
        [
            (start("ul", &[]), 0..4),
            (start("li", &[]), 4..8),
            (text("a & b"), 8..17),
            (end("li", true), 17..17), // closed by the next <li>
            (start("li", &[]), 17..21),
            (text("c"), 21..22),
            (start("br", &[]), 22..26),
            (end("br", true), 26..26), // void: ends with its start tag
            (end("li", true), 26..26), // closed by </ul>
            (end("ul", false), 26..31),
        ]
    );
}

#[test]
fn mismatched_and_stray_end_tags() {
    // </div> closes the <b> it skips over; </i> matches nothing and is dropped.
    let html = "<div><b>x</i></div>";
    assert_eq!(
        collect(html),
        [
            (start("div", &[]), 0..5),
            (start("b", &[]), 5..8),
            (text("x"), 8..9),
            (end("b", true), 13..13),
            (end("div", false), 13..19),
        ]
    );
    let mut events = domtree::events(html);
    for _ in events.by_ref() {}
    let kinds: Vec<_> = events.errors().iter().map(|e| e.kind.clone()).collect();
    assert_eq!(
        kinds,
        [
            domtree::ErrorKind::StrayEndTag,
            domtree::ErrorKind::MismatchedEndTag
        ]
    );
}

#[test]
fn attributes_are_lowercased_decoded_and_kept_in_order() {
    let html = r#"<A HREF="/x?a=1&amp;b=2" Download data-x='&lt;' href=dup>"#;
    let events = collect(html);
    assert_eq!(
        events[0].0,
        start(
            "a",
            &[
                ("href", "/x?a=1&b=2"),
                ("download", ""),
                ("data-x", "<"),
                ("href", "dup")
            ]
        )
    );
    // Open at the end of the input: implied End there.
    assert_eq!(events[1], (end("a", true), html.len()..html.len()));
}

#[test]
fn raw_text_is_not_decoded_but_rcdata_is() {
    let html = "<script>a &amp;&lt; b</script><title>a &amp; b</title><p>&nbsp;<![CDATA[<x>]]>";
    let texts: Vec<_> = collect(html)
        .into_iter()
        .filter_map(|(e, _)| match e {
            Event::Text(t) => Some(t.into_owned()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["a &amp;&lt; b", "a & b", "\u{a0}", "<x>"]);
}

#[test]
fn self_closing_comments_doctypes() {
    let html = "<!DOCTYPE html ><div/><!--hi-->";
    assert_eq!(
        collect(html),
        [
            (Event::Doctype("html"), 0..16),
            (start("div", &[]), 16..22),
            (end("div", true), 22..22),
            (Event::Comment("hi"), 22..31),
        ]
    );
}

#[test]
fn text_closes_an_open_head() {
    let html = "<head><title>t</title>Hello";
    let names: Vec<_> = collect(html)
        .into_iter()
        .map(|(e, r)| match e {
            Event::Start { name, .. } => format!("+{name}@{}", r.start),
            Event::End { name, .. } => format!("-{name}@{}", r.end),
            Event::Text(t) => format!("{t:?}"),
            _ => String::new(),
        })
        .collect();
    assert_eq!(
        names,
        [
            "+head@0",
            "+title@6",
            "\"t\"",
            "-title@22",
            "-head@22",
            "\"Hello\""
        ]
    );
}

#[test]
fn borrowed_where_possible() {
    // Lower-case names and entity-free text borrow from the input.
    for (event, _) in domtree::events("<p class=x>plain</p>") {
        match event {
            Event::Start { name, attrs } => {
                assert!(matches!(name, Cow::Borrowed(_)));
                assert!(
                    attrs
                        .iter()
                        .all(|(k, v)| matches!((k, v), (Cow::Borrowed(_), Cow::Borrowed(_))))
                );
            }
            Event::End { name, .. } => assert!(matches!(name, Cow::Borrowed(_))),
            Event::Text(t) => assert!(matches!(t, Cow::Borrowed(_))),
            _ => {}
        }
    }
}

#[test]
fn deep_nesting_streams_without_recursion() {
    let n = 100_000;
    let html = "<div>".repeat(n) + &"</x>".repeat(n) + &"</div>".repeat(n);
    let mut depth = 0usize;
    let mut max = 0usize;
    let mut events = domtree::events(&html);
    for (event, _) in events.by_ref() {
        match event {
            Event::Start { .. } => {
                depth += 1;
                max = max.max(depth);
            }
            Event::End { implied, .. } => {
                assert!(!implied);
                depth -= 1;
            }
            _ => unreachable!(),
        }
    }
    assert_eq!((depth, max), (0, n));
    assert_eq!(events.errors().len(), n); // every </x> is stray
    // Fused: stays exhausted.
    assert!(events.next().is_none());
}

#[test]
fn the_empty_document_has_no_events() {
    assert!(collect("").is_empty());
    let mut events = domtree::events("");
    assert!(events.next().is_none());
    assert!(events.errors().is_empty());
}
