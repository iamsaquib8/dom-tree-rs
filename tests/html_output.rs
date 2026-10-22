//! Tests for HTML re-serialization (`to_html`).

use domq::parse;

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
fn parsed_raw_text_is_written_back_byte_for_byte() {
    // Content that only looks like an end tag. The serializer rewrites a
    // `</script` that would close the element early, which a parsed document
    // never contains: the tokenizer ends the content at the first one.
    let inputs = [
        "<script>if (a < b && c > d) x('<p>');</script>",
        "<script>document.write('<\\/script>')</script>",
        "<script>var s = '</scr' + 'ipt>';</script>",
        "<script>a</scriptx>b</SCRIPTX></script>",
        "<script>x</script</script>",
        "<script>'</style>' + '</textarea>'</script>",
        "<script><!-- <script> --></script>",
        "<script>a</</script>",
        "<style>a::before{content:'</script>'}</style>",
        "<style>a</styles>b</style>",
        "<title>a &lt;/title&gt; b</title>",
        "<textarea>&lt;/textarea&gt;</textarea>",
    ];
    for html in inputs {
        let dom = parse(html);
        assert_eq!(dom.to_html(), html, "for {html:?}");
        let root = dom.root().unwrap();
        assert_eq!(root.to_html(), html);
        let content = root.first_child().unwrap();
        assert_eq!(root.inner_html(), content.to_html());
        assert_eq!(dom.nodes().count(), 2, "one element, one text node");
    }
}

#[test]
fn edited_script_text_reparses_to_a_single_script() {
    let mut dom = parse("<script>var a=1</script><p>ok</p>");
    let s = dom.root().unwrap().id();
    dom.set_text(s, "x</script><img src=x onerror=alert(1)><script>");
    let html = dom.to_html();
    assert_eq!(
        html,
        "<script>x<\\/script><img src=x onerror=alert(1)><script></script><p>ok</p>"
    );
    let reparsed = parse(&html);
    let tags: Vec<_> = reparsed.nodes().filter_map(|n| n.tag_name()).collect();
    assert_eq!(tags, ["script", "p"]);
    assert_eq!(reparsed.root().unwrap().children().count(), 1);
    // And it is stable from there on.
    assert_eq!(reparsed.to_html(), html);
}

#[test]
fn subtree_serialization() {
    let dom = parse("<div><b>hi</b><i>yo</i></div>");
    let b = dom.find_by_tag("b").next().unwrap();
    assert_eq!(b.to_html(), "<b>hi</b>");
}

#[test]
fn inner_and_outer_html() {
    let dom = parse("<div id=x>a <b class=k>b &amp; c</b><br><!-- n --></div>");
    let div = dom.root().unwrap();
    assert_eq!(
        div.inner_html(),
        "a <b class=\"k\">b &amp; c</b><br><!-- n -->"
    );
    assert_eq!(div.outer_html(), div.to_html());
    assert_eq!(
        div.outer_html(),
        "<div id=\"x\">a <b class=\"k\">b &amp; c</b><br><!-- n --></div>"
    );

    // Void elements, text and empty elements have no inner HTML.
    assert_eq!(dom.find_by_tag("br").next().unwrap().inner_html(), "");
    assert_eq!(parse("<p></p>").root().unwrap().inner_html(), "");
    let text = div.first_child().unwrap();
    assert_eq!(text.inner_html(), "");
    assert_eq!(text.outer_html(), "a ");

    // Raw-text content stays raw.
    let dom = parse("<script>if (a < b) {}</script>");
    assert_eq!(dom.root().unwrap().inner_html(), "if (a < b) {}");
}
