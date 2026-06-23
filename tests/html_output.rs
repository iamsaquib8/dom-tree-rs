//! Tests for HTML re-serialization (`to_html`).

use domtree::parse;

#[test]
fn roundtrip_is_idempotent() {
    let inputs = [
        "<p class=\"x\">hi</p>",
        "<div><img src=\"a.png\"><br></div>",
        "<ul><li>a</li><li>b</li></ul>",
        "<a href=\"/x?y=1&amp;z=2\">l</a>",
        "<!doctype html><!-- c --><p>x</p>",
        "<input type=\"text\" disabled required>",
        "<script>if (a < b && c > d) x('<p>');</script>",
        "<style>a::before{content:'<'}</style>",
    ];
    for html in inputs {
        let once = parse(html).to_html();
        let twice = parse(&once).to_html();
        assert_eq!(once, twice, "to_html must be idempotent for {html:?}");
    }
}

#[test]
fn escapes_text_and_attribute_values() {
    let dom = parse("<p title=\"a &amp; b\">x &lt; y &amp; z</p>");
    assert_eq!(dom.to_html(), "<p title=\"a &amp; b\">x &lt; y &amp; z</p>");
}

#[test]
fn void_elements_have_no_close_tag() {
    let dom = parse("<br><img src=logo.png>");
    assert_eq!(dom.to_html(), "<br><img src=\"logo.png\">");
}

#[test]
fn boolean_attributes_render_without_value() {
    let dom = parse("<input disabled required type=text>");
    assert_eq!(dom.to_html(), "<input disabled required type=\"text\">");
}

#[test]
fn comments_and_doctype() {
    let dom = parse("<!DOCTYPE html><!-- note --><p>x</p>");
    assert_eq!(dom.to_html(), "<!doctype html><!-- note --><p>x</p>");
}

#[test]
fn raw_text_is_not_escaped() {
    let dom = parse("<script>if (1 < 2) {}</script>");
    assert_eq!(dom.to_html(), "<script>if (1 < 2) {}</script>");
}

#[test]
fn subtree_serialization() {
    let dom = parse("<div><b>hi</b><i>yo</i></div>");
    let b = dom.find_by_tag("b").next().unwrap();
    assert_eq!(b.to_html(), "<b>hi</b>");
}
