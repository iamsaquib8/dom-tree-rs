//! Tests for the CSS-subset selector engine.
#![cfg(feature = "query")]

use domtree::parse;

#[test]
fn selectors() {
    let dom = parse("<div class='a'><p id='x'>1</p><p class='y z'>2</p></div><p>3</p>");
    assert_eq!(dom.select("p").count(), 3);
    assert_eq!(dom.select("div p").count(), 2);
    assert_eq!(dom.select("div > p").count(), 2);
    assert_eq!(dom.select("#x").count(), 1);
    assert_eq!(dom.select(".y").count(), 1);
    assert_eq!(dom.select("p.y.z").count(), 1);
    assert_eq!(dom.select("p.y.q").count(), 0);
    assert_eq!(dom.select("div.a > p#x").count(), 1);
    assert_eq!(dom.select("p, div").count(), 4);
    assert_eq!(dom.select("*").count(), 4);
    assert_eq!(dom.select("[id=x]").count(), 1);
    assert_eq!(dom.select("[id]").count(), 1);
    assert_eq!(dom.select("[class]").count(), 2);
}

#[test]
fn child_vs_descendant() {
    let dom = parse("<div><section><p>deep</p></section><p>shallow</p></div>");
    assert_eq!(dom.select("div p").count(), 2);
    assert_eq!(dom.select("div > p").count(), 1);
    assert_eq!(
        dom.select("div > p").next().unwrap().text_content(),
        "shallow"
    );
}

#[test]
fn attribute_quoted_value() {
    let dom = parse(r#"<a href="/x">1</a><a href="/y">2</a>"#);
    assert_eq!(dom.select(r#"a[href="/x"]"#).count(), 1);
    assert_eq!(dom.select("a[href='/y']").count(), 1);
}

#[test]
fn empty_or_garbage_selector_matches_nothing() {
    let dom = parse("<p>x</p>");
    assert_eq!(dom.select("").count(), 0);
    assert_eq!(dom.select("   ").count(), 0);
}
