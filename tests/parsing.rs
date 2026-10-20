//! Behavioural tests for the parser and tree.

use domtree::{NodeKind, Token, parse, tokenize};

#[test]
fn basic_nesting() {
    let dom = parse("<a><b>hi</b></a>");
    let a = dom.root().unwrap();
    assert_eq!(a.tag_name(), Some("a"));
    let b = a.first_child().unwrap();
    assert_eq!(b.tag_name(), Some("b"));
    assert_eq!(b.text_content(), "hi");
    assert_eq!(b.parent().unwrap().id(), a.id());
}

#[test]
fn void_elements_have_no_children() {
    let dom = parse("<div><br><img src=x>text</div>");
    let div = dom.root().unwrap();
    let kids: Vec<_> = div.children().collect();
    assert_eq!(kids.len(), 3);
    assert_eq!(kids[0].tag_name(), Some("br"));
    assert_eq!(kids[1].tag_name(), Some("img"));
    assert_eq!(kids[1].attr("src"), Some("x"));
    assert_eq!(kids[2].text(), Some("text"));
    assert_eq!(div.child_elements().count(), 2);
}

#[test]
fn self_closing_tag() {
    let dom = parse("<div><span/>x</div>");
    let div = dom.root().unwrap();
    let kids: Vec<_> = div.children().collect();
    assert_eq!(kids[0].tag_name(), Some("span"));
    assert_eq!(kids[0].children().count(), 0);
    assert_eq!(kids[1].text(), Some("x"));
}

#[test]
fn attribute_variants() {
    let dom = parse(r#"<input type="text" disabled value=hello data-x='y'>"#);
    let input = dom.root().unwrap();
    assert_eq!(input.attr("type"), Some("text")); // double-quoted
    assert_eq!(input.attr("disabled"), Some("")); // boolean
    assert_eq!(input.attr("value"), Some("hello")); // unquoted
    assert_eq!(input.attr("data-x"), Some("y")); // single-quoted
    assert_eq!(input.attributes().count(), 4);
}

#[test]
fn attribute_names_lowercased() {
    let dom = parse(r#"<DIV CLASS="X" Id="y">"#);
    let div = dom.root().unwrap();
    assert_eq!(div.tag_name(), Some("div"));
    assert_eq!(div.attr("class"), Some("X")); // name lowered, value preserved
    assert_eq!(div.attr("id"), Some("y"));
}

#[test]
fn comments_doctype_cdata() {
    let dom = parse("<!doctype html><!-- hi --><p>x</p>");
    let roots: Vec<_> = dom.roots().collect();
    assert_eq!(roots.len(), 3);
    assert!(matches!(roots[0].kind(), NodeKind::Doctype(_)));
    assert_eq!(roots[1].comment(), Some(" hi "));
    assert_eq!(roots[2].tag_name(), Some("p"));
}

#[test]
fn script_content_is_raw() {
    let dom = parse("<script>if (a < b && c > d) { x('<p>'); }</script>");
    let s = dom.root().unwrap();
    assert_eq!(s.tag_name(), Some("script"));
    assert_eq!(s.children().count(), 1);
    assert_eq!(s.text_content(), "if (a < b && c > d) { x('<p>'); }");
}

#[test]
fn entities_decoded_in_text_but_not_in_script() {
    let dom = parse("<p>Tom &amp; Jerry &lt;3 &#169; &#x1F600;</p>");
    assert_eq!(dom.root().unwrap().text_content(), "Tom & Jerry <3 © 😀");

    let dom = parse("<style>a::before{content:'&amp;'}</style>");
    assert_eq!(
        dom.root().unwrap().text_content(),
        "a::before{content:'&amp;'}"
    );
}

#[test]
fn implicit_close_p_and_li() {
    let dom = parse("<ul><li>a<li>b</ul><p>x<p>y");
    assert_eq!(dom.find_by_tag("li").count(), 2);
    assert_eq!(dom.find_by_tag("p").count(), 2);
    let ul = dom.find_by_tag("ul").next().unwrap();
    assert_eq!(ul.child_elements().count(), 2);
}

#[test]
fn mismatched_and_stray_end_tags_recover() {
    let dom = parse("<b><i>x</b>y</i>");
    assert!(!dom.errors().is_empty());
    let i = dom.find_by_tag("i").next().unwrap();
    assert_eq!(i.parent().unwrap().tag_name(), Some("b"));
    assert_eq!(i.text_content(), "x");
}

#[test]
fn custom_and_namespaced_tags() {
    let dom = parse("<my-component data-id='1'><svg:rect/></my-component>");
    let c = dom.root().unwrap();
    assert_eq!(c.tag_name(), Some("my-component"));
    assert_eq!(c.attr("data-id"), Some("1"));
    assert_eq!(c.first_child().unwrap().tag_name(), Some("svg:rect"));
}

#[test]
fn find_helpers() {
    let dom = parse("<div id='main'><span class='a b'>1</span><span class='a'>2</span></div>");
    assert_eq!(dom.find_by_id("main").unwrap().tag_name(), Some("div"));
    assert_eq!(dom.find_by_class("a").count(), 2);
    assert_eq!(dom.find_by_class("b").count(), 1);
    assert_eq!(dom.find_by_tag("span").count(), 2);
}

#[test]
fn ancestors_and_siblings() {
    let dom = parse("<a><b><c>x</c></b></a>");
    let c = dom.find_by_tag("c").next().unwrap();
    let names: Vec<_> = c.ancestors().filter_map(|n| n.tag_name()).collect();
    assert_eq!(names, vec!["b", "a"]);

    let dom = parse("<ul><li>1</li><li>2</li><li>3</li></ul>");
    let first = dom.find_by_tag("li").next().unwrap();
    assert_eq!(first.next_sibling().unwrap().text_content(), "2");
    assert!(first.prev_sibling().is_none());
}

#[test]
fn element_siblings_skip_other_nodes() {
    let dom = parse("<ul>\n  <li>1</li>\n  <!-- c -->\n  <li>2</li>text<li>3</li>\n</ul>");
    let items: Vec<_> = dom.find_by_tag("li").collect();
    // Plain siblings include the whitespace text...
    assert!(items[0].next_sibling().unwrap().is_text());
    // ...element siblings skip text and comments.
    assert_eq!(items[0].next_element_sibling().unwrap().id(), items[1].id());
    assert_eq!(items[1].next_element_sibling().unwrap().id(), items[2].id());
    assert!(items[2].next_element_sibling().is_none());
    assert_eq!(items[2].prev_element_sibling().unwrap().id(), items[1].id());
    assert_eq!(items[1].prev_element_sibling().unwrap().id(), items[0].id());
    assert!(items[0].prev_element_sibling().is_none());
    // Text nodes have element siblings too.
    let text = items[1].next_sibling().unwrap();
    assert_eq!(text.next_element_sibling().unwrap().id(), items[2].id());
}

#[test]
fn tokenizer_stream() {
    let toks: Vec<_> = tokenize("<a href='x'>hi</a>").collect();
    assert_eq!(toks.len(), 3);
    assert!(matches!(toks[0], Token::StartTag { name: "a", .. }));
    assert!(matches!(toks[1], Token::Text("hi")));
    assert!(matches!(toks[2], Token::EndTag { name: "a" }));
}

#[test]
fn never_panics_on_garbage() {
    let inputs = [
        "",
        "<",
        "<<>>",
        "<a",
        "<!--",
        "</>",
        "<a b=>",
        "<a b='",
        "<![CDATA[",
        "&",
        "&#;",
        "&#xZZ;",
        "<a/></a>",
        "<\u{0}>",
        "🦀<b>",
        "<a =",
        "<!",
        "<?php ?>",
        "</",
        "<div><div><div>",
        "<p</p>",
        "=>'\"<>",
    ];
    for i in inputs {
        let _ = parse(i);
        let _ = parse(i).to_json();
    }
}

#[test]
fn deeply_nested_does_not_overflow() {
    // 50k nested <div>: builder and serializer must stay iterative.
    let depth = 50_000;
    let mut html = String::with_capacity(depth * 5);
    for _ in 0..depth {
        html.push_str("<div>");
    }
    let dom = parse(&html);
    assert_eq!(dom.find_by_tag("div").count(), depth);
    // to_json is iterative, so this must not blow the stack either.
    assert!(dom.to_json().len() > depth);
}
