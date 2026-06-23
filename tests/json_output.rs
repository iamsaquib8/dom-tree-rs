//! Tests for the zero-dependency JSON serializer.
#![cfg(feature = "json")]

use domtree::parse;

#[test]
fn document_shape() {
    let dom = parse("<ul id=list><li class=item>Item 1</li></ul>");
    let j = dom.to_json();
    assert!(j.starts_with("{\"type\":\"document\",\"errors\":0,\"children\":["));
    assert!(j.contains("\"type\":\"element\",\"tag\":\"ul\",\"attrs\":{\"id\":\"list\"}"));
    assert!(j.contains("\"type\":\"element\",\"tag\":\"li\",\"attrs\":{\"class\":\"item\"}"));
    assert!(j.contains("\"type\":\"text\",\"value\":\"Item 1\""));
    assert!(j.ends_with("}"));
}

#[test]
fn empty_children_array() {
    let dom = parse("<br>");
    assert!(
        dom.to_json()
            .contains("\"tag\":\"br\",\"attrs\":{},\"children\":[]")
    );
}

#[test]
fn string_escaping() {
    let dom = parse("<p>line1\nline2\t\"q\" \\z</p>");
    let j = dom.to_json();
    assert!(j.contains(r#"line1\nline2\t\"q\" \\z"#));
}

#[test]
fn control_char_escaping() {
    let dom = parse("<p>\u{1}\u{1f}</p>");
    let j = dom.to_json();
    assert!(j.contains("\\u0001\\u001f"));
}

#[test]
fn errors_count_reflected() {
    let dom = parse("<b><i>x</b></i>"); // mismatched + stray
    let j = dom.to_json();
    assert!(!dom.errors().is_empty());
    assert!(j.contains(&format!("\"errors\":{}", dom.errors().len())));
}
