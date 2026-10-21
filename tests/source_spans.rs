//! `NodeRef::source_range` and `line_col`: every node kind, implicit and
//! mis-nested closes, entities, CRLF, multi-byte UTF-8.

use domtree::{Dom, line_col, parse};

/// The source text of every node, in document order.
fn spans<'s>(dom: &Dom, src: &'s str) -> Vec<&'s str> {
    dom.nodes()
        .map(|n| &src[n.source_range().expect("parsed nodes have a range")])
        .collect()
}

fn span_of<'s>(dom: &Dom, src: &'s str, tag: &str) -> &'s str {
    let n = dom.find_by_tag(tag).next().expect("tag present");
    &src[n.source_range().unwrap()]
}

#[test]
fn every_node_kind_spans_its_markup() {
    let src =
        "<!DOCTYPE html><!-- note --><p class=x>a<br/>b<img src=y>c</p><![CDATA[raw]]><?php x ?>";
    let dom = parse(src);
    assert_eq!(
        spans(&dom, src),
        [
            "<!DOCTYPE html>",
            "<!-- note -->",
            "<p class=x>a<br/>b<img src=y>c</p>",
            "a",
            "<br/>",
            "b",
            "<img src=y>",
            "c",
            "<![CDATA[raw]]>",
            "<?php x ?>",
        ]
    );
}

#[test]
fn raw_text_elements() {
    let src = "<script>if (a < b) x('</p>')</script><style>p{}</style><title>A &amp; B</title>";
    let dom = parse(src);
    assert_eq!(
        spans(&dom, src),
        [
            "<script>if (a < b) x('</p>')</script>",
            "if (a < b) x('</p>')",
            "<style>p{}</style>",
            "p{}",
            "<title>A &amp; B</title>",
            "A &amp; B",
        ]
    );
}

#[test]
fn entities_span_their_raw_source() {
    let src = "<p>Tom &amp; Jerry &copy; &#169;</p>";
    let dom = parse(src);
    let text = dom.root().unwrap().first_child().unwrap();
    assert_eq!(text.text(), Some("Tom & Jerry © ©"));
    assert_eq!(
        &src[text.source_range().unwrap()],
        "Tom &amp; Jerry &copy; &#169;"
    );
}

#[test]
fn implicitly_closed_elements_end_where_they_were_closed() {
    let src = "<ul><li>one\n<li>two</ul><p>a<p>b<div>c</div>";
    let dom = parse(src);
    let li: Vec<_> = dom
        .find_by_tag("li")
        .map(|n| &src[n.source_range().unwrap()])
        .collect();
    // Closed by the next <li>, and by the mismatched `</ul>`.
    assert_eq!(li, ["<li>one\n", "<li>two"]);
    assert_eq!(span_of(&dom, src, "ul"), "<ul><li>one\n<li>two</ul>");
    let p: Vec<_> = dom
        .find_by_tag("p")
        .map(|n| &src[n.source_range().unwrap()])
        .collect();
    assert_eq!(p, ["<p>a", "<p>b"]);
    assert_eq!(span_of(&dom, src, "div"), "<div>c</div>");
}

#[test]
fn mis_nested_and_stray_end_tags() {
    let src = "<b><i>x</b>y</i>z";
    let dom = parse(src);
    // `</b>` closes the open <i> too: <i> ends where `</b>` starts.
    assert_eq!(span_of(&dom, src, "b"), "<b><i>x</b>");
    assert_eq!(span_of(&dom, src, "i"), "<i>x");
    // The stray `</i>` is not part of any node.
    assert_eq!(spans(&dom, src), ["<b><i>x</b>", "<i>x", "x", "y", "z"]);

    // A stray end tag inside an implicitly closed element is inside its range.
    let src = "<li>a</em><li>b";
    let dom = parse(src);
    let first = dom.find_by_tag("li").next().unwrap();
    assert_eq!(&src[first.source_range().unwrap()], "<li>a</em>");
}

#[test]
fn unclosed_elements_run_to_the_end_of_input() {
    let src = "<div><section><p>text";
    let dom = parse(src);
    for tag in ["div", "section", "p"] {
        assert_eq!(
            dom.find_by_tag(tag)
                .next()
                .unwrap()
                .source_range()
                .unwrap()
                .end,
            src.len()
        );
    }
    // An unterminated start tag, too.
    let src = "<p>x<a href='y";
    let dom = parse(src);
    assert_eq!(span_of(&dom, src, "a"), "<a href='y");
    assert_eq!(span_of(&dom, src, "p"), src);
}

#[test]
fn crlf_and_multibyte_text() {
    let src = "<p>\r\n  café — 🦀\r\n</p>\r\n<p>ü</p>";
    let dom = parse(src);
    let p: Vec<_> = dom.find_by_tag("p").collect();
    let second = p[1].source_range().unwrap();
    assert_eq!(&src[second.clone()], "<p>ü</p>");
    assert_eq!(line_col(src, second.start), (4, 1));
    let text = p[0].first_child().unwrap().source_range().unwrap();
    assert_eq!(&src[text.clone()], "\r\n  café — 🦀\r\n");
    assert_eq!(line_col(src, text.start), (1, 4));
    // The crab: after "  café — " on line 2, i.e. column 10.
    assert_eq!(line_col(src, src.find('🦀').unwrap()), (2, 10));
}

#[test]
fn edited_and_inserted_nodes_have_no_range() {
    let src = "<p>a</p>";
    let mut dom = parse(src);
    let p = dom.root().unwrap().id();
    let ids = dom.append_html(p, "<b>b</b>");
    let b = dom.get(ids[0]).unwrap();
    assert!(b.source_range().is_none());
    assert!(b.first_child().unwrap().source_range().is_none());
    let created = dom.create_element("i").unwrap();
    assert!(dom.get(created).unwrap().source_range().is_none());
    // Edits don't move a parsed node's range.
    dom.rename(p, "div");
    dom.set_text(p, "zzz");
    assert_eq!(dom.root().unwrap().source_range(), Some(0..src.len()));
    // ...but the text node set_text created is new.
    assert!(
        dom.root()
            .unwrap()
            .first_child()
            .unwrap()
            .source_range()
            .is_none()
    );
}

#[test]
fn errors_positions_map_to_lines() {
    let src = "<div>\n  <p>ok</p>\n  </span>\n</div>";
    let dom = parse(src);
    let stray = dom.errors().first().expect("a stray end tag");
    assert_eq!(line_col(src, stray.position), (3, 3));
}

#[test]
fn line_col_edge_cases() {
    assert_eq!(line_col("", 0), (1, 1));
    assert_eq!(line_col("", 99), (1, 1));
    assert_eq!(line_col("abc", 3), (1, 4));
    assert_eq!(line_col("a\nb", 1), (1, 2)); // at the '\n'
    assert_eq!(line_col("a\nb", 2), (2, 1));
    assert_eq!(line_col("a\r\nb", 2), (1, 3)); // between '\r' and '\n'
    assert_eq!(line_col("a\r\nb", 3), (2, 1));
    assert_eq!(line_col("a\rb", 2), (2, 1)); // lone CR breaks a line
    assert_eq!(line_col("\n\n\n", 3), (4, 1));
    assert_eq!(line_col("\r\n\r\n", 4), (3, 1));
    // Inside a multi-byte character: clamped to its start.
    let s = "x🦀y";
    assert_eq!(line_col(s, 1), (1, 2));
    assert_eq!(line_col(s, 2), (1, 2));
    assert_eq!(line_col(s, 4), (1, 2));
    assert_eq!(line_col(s, 5), (1, 3));
    assert_eq!(line_col(s, usize::MAX), (1, 4));
}
