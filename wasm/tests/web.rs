//! Tests for the JS API, run in Node with: `wasm-pack test --node`.
//!
//! This file is compiled as a module of the library (see the bottom of
//! `src/lib.rs`), so it calls the exported functions directly and inspects the
//! JS values they return. `tests/node.mjs` covers the built npm package itself.

#![cfg(target_arch = "wasm32")]

use crate::{
    Document, errors, parse, parse_json, select, select_first, to_html, to_markdown, to_text,
};
use js_sys::{JSON, Reflect};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::wasm_bindgen_test;

/// `JSON.stringify(value)`; `"undefined"` for `undefined`.
fn js(value: &JsValue) -> String {
    if value.is_undefined() {
        return "undefined".into();
    }
    JSON::stringify(value).unwrap().as_string().unwrap()
}

fn get(value: &JsValue, key: &str) -> JsValue {
    Reflect::get(value, &JsValue::from_str(key)).unwrap()
}

/// Assert `err` is the `SelectorError` we throw, and return (message, position).
fn selector_error(err: JsValue) -> (String, f64) {
    assert!(
        err.is_instance_of::<js_sys::Error>(),
        "not an Error: {err:?}"
    );
    assert_eq!(get(&err, "name").as_string().unwrap(), "SelectorError");
    (
        get(&err, "message").as_string().unwrap(),
        get(&err, "position").as_f64().unwrap(),
    )
}

const LIST: &str = "<ul id=list><li class=item>One<li>Two &amp; more</ul>";

#[wasm_bindgen_test]
fn parse_returns_a_js_object() {
    let doc = parse("<p class='x'>hi &amp; bye</p>");
    assert_eq!(
        js(&doc),
        r#"{"type":"document","errors":0,"children":[{"type":"element","tag":"p","attrs":{"class":"x"},"children":[{"type":"text","value":"hi & bye"}]}]}"#
    );
    // Recovered problems are counted.
    assert!(get(&parse("<b><i>x</b>"), "errors").as_f64().unwrap() > 0.0);
}

#[wasm_bindgen_test]
fn parse_json_returns_the_same_tree_as_a_string() {
    let html = "<!doctype html><!-- c --><a href=/x>link</a>";
    assert_eq!(parse_json(html), js(&parse(html)));
    assert_eq!(
        parse_json("<br>"),
        r#"{"type":"document","errors":0,"children":[{"type":"element","tag":"br","attrs":{},"children":[]}]}"#
    );
}

#[wasm_bindgen_test]
fn select_returns_matches() {
    let hits = select(LIST, "li").unwrap();
    assert_eq!(
        js(&hits),
        concat!(
            r#"[{"tag":"li","attrs":{"class":"item"},"text":"One","html":"<li class=\"item\">One</li>","nodeIndex":1},"#,
            r#"{"tag":"li","attrs":{},"text":"Two & more","html":"<li>Two &amp; more</li>","nodeIndex":3}]"#
        )
    );
    assert_eq!(js(&select(LIST, "table").unwrap()), "[]");
}

#[wasm_bindgen_test]
fn node_index_is_the_depth_first_position_in_the_tree() {
    // Mis-nested and unclosed on purpose.
    let html = "<!doctype html><div><p>a<b>b<i>c</b>d</i><p>e</div><span>f";
    let tree = parse(html);
    let mut order = Vec::new(); // tag (or "#text" etc.) in depth-first order
    let mut stack = vec![get(&tree, "children")];
    let mut cursors = vec![0u32];
    while let Some(list) = stack.last().cloned() {
        let array: js_sys::Array = list.into();
        let i = cursors.last().copied().unwrap();
        if i >= array.length() {
            stack.pop();
            cursors.pop();
            continue;
        }
        *cursors.last_mut().unwrap() += 1;
        let node = array.get(i);
        let kind = get(&node, "type").as_string().unwrap();
        if kind == "element" {
            order.push(get(&node, "tag").as_string().unwrap());
            stack.push(get(&node, "children"));
            cursors.push(0);
        } else {
            order.push(format!("#{kind}"));
        }
    }
    let hits: js_sys::Array = select(html, "*").unwrap().into();
    assert!(hits.length() > 5);
    for hit in hits.iter() {
        let index = get(&hit, "nodeIndex").as_f64().unwrap() as usize;
        assert_eq!(order[index], get(&hit, "tag").as_string().unwrap());
    }
}

#[wasm_bindgen_test]
fn invalid_selectors_throw_a_selector_error() {
    let (message, position) = selector_error(select(LIST, "div > p)").unwrap_err());
    assert_eq!(message, "unexpected ')' at byte 7");
    assert_eq!(position, 7.0);

    let (message, position) = selector_error(select(LIST, "a:hover").unwrap_err());
    assert_eq!(message, "unsupported pseudo-class ':hover' at byte 1");
    assert_eq!(position, 1.0);

    // `position` is a JS string index (UTF-16), not a byte offset.
    let (message, position) = selector_error(select(LIST, "p.é)").unwrap_err());
    assert_eq!(message, "unexpected ')' at byte 4");
    assert_eq!(position, 3.0);

    // Every function that takes a selector throws the same way.
    selector_error(select_first(LIST, "").unwrap_err());
    selector_error(to_text(LIST, Some("li >".into())).unwrap_err());
    selector_error(to_markdown(LIST, Some("::before".into())).unwrap_err());
    let doc = Document::new(LIST);
    selector_error(doc.select("[").unwrap_err());
    selector_error(doc.select_first("li:nope").unwrap_err());
    selector_error(doc.to_text(Some(",".into())).unwrap_err());
    selector_error(doc.to_markdown(Some("a >> b".into())).unwrap_err());
}

#[wasm_bindgen_test]
fn select_first_returns_a_match_or_undefined() {
    let hit = select_first(LIST, "li:last-child").unwrap();
    assert_eq!(
        js(&hit),
        r#"{"tag":"li","attrs":{},"text":"Two & more","html":"<li>Two &amp; more</li>","nodeIndex":3}"#
    );
    assert!(select_first(LIST, "table").unwrap().is_undefined());
}

#[wasm_bindgen_test]
fn to_text_whole_document_or_matches() {
    let html = "<head><title>T</title></head><h1>Hello</h1><p>Some   <b>bold</b>\n text.</p>\
                <ul><li>a</li><li></li><li>b</li></ul><script>x()</script>";
    assert_eq!(
        to_text(html, None).unwrap(),
        "Hello\n\nSome bold text.\n\na\nb"
    );
    // Matches joined by a blank line; empty ones skipped.
    assert_eq!(to_text(html, Some("li".into())).unwrap(), "a\n\nb");
    assert_eq!(to_text(html, Some("table".into())).unwrap(), "");
}

#[wasm_bindgen_test]
fn to_markdown_whole_document_or_matches() {
    let html = "<h1>Title</h1><p>Some <b>bold</b> and a <a href='/x'>link</a>.</p>\
                <ul><li>one</li><li>two</li></ul>";
    assert_eq!(
        to_markdown(html, None).unwrap(),
        "# Title\n\nSome **bold** and a [link](/x).\n\n- one\n- two"
    );
    assert_eq!(
        to_markdown(html, Some("h1, li".into())).unwrap(),
        "# Title\n\n- one\n\n- two"
    );
}

#[wasm_bindgen_test]
fn to_html_repairs_markup() {
    assert_eq!(
        to_html("<ul><li>a<li>b</ul><p class=x>1 < 2"),
        r#"<ul><li>a</li><li>b</li></ul><p class="x">1 &lt; 2</p>"#
    );
}

#[wasm_bindgen_test]
fn errors_lists_positions_and_messages() {
    assert_eq!(js(&errors("<p>clean</p>")), "[]");
    assert_eq!(
        js(&errors("<b><i>x</b></i>")),
        r#"[{"position":7,"message":"end tag closed mis-nested elements"},{"position":11,"message":"stray end tag with no matching open element"}]"#
    );
    // Positions are JS string indices: "é" is 2 bytes but 1 UTF-16 unit, and
    // "😀" is 4 bytes but 2 units.
    let html = "<p>é😀</x>";
    let stray = js_sys::Array::from(&errors(html))
        .iter()
        .find(|e| get(e, "message").as_string().unwrap().starts_with("stray"))
        .unwrap();
    assert_eq!(get(&stray, "position").as_f64().unwrap(), 6.0); // html.indexOf("</x>") in JS
}

#[wasm_bindgen_test]
fn document_parses_once_and_queries_many_times() {
    let doc = Document::parse(LIST);
    assert_eq!(
        js(&doc.select("li").unwrap()),
        js(&select(LIST, "li").unwrap())
    );
    assert_eq!(
        js(&doc.select_first("#list > .item").unwrap()),
        js(&select_first(LIST, "#list > .item").unwrap())
    );
    assert!(doc.select_first("p").unwrap().is_undefined());
    assert_eq!(doc.to_text(None).unwrap(), "One\nTwo & more");
    assert_eq!(doc.to_text(Some("li".into())).unwrap(), "One\n\nTwo & more");
    assert_eq!(doc.to_markdown(None).unwrap(), "- One\n- Two & more");
    assert_eq!(doc.to_markdown(Some(".item".into())).unwrap(), "- One");
    assert_eq!(
        doc.to_html(),
        r#"<ul id="list"><li class="item">One</li><li>Two &amp; more</li></ul>"#
    );
    assert_eq!(js(&doc.tree()), parse_json(LIST));
    assert_eq!(js(&doc.to_json()), parse_json(LIST));
    assert_eq!(js(&doc.errors()), js(&errors(LIST)));
    assert_eq!(
        js(&Document::new("<p>x").errors()),
        r#"[{"position":4,"message":"element implicitly closed"}]"#
    );
}

#[wasm_bindgen_test]
fn strings_survive_the_json_round_trip() {
    let text = "quote \" backslash \\ tab \t nl \n ctrl \u{1} \u{1f} del \u{7f} \u{2028} 😀 é";
    let html = format!("<p title='{text}'>{text}</p>");
    let tree = parse(&html);
    let p = js_sys::Array::from(&get(&tree, "children")).get(0);
    assert_eq!(get(&get(&p, "attrs"), "title").as_string().unwrap(), text);
    let hit = select_first(&html, "p").unwrap();
    assert_eq!(get(&get(&hit, "attrs"), "title").as_string().unwrap(), text);
    let tab = js_sys::Array::from(&get(&p, "children")).get(0);
    assert_eq!(get(&tab, "value").as_string().unwrap(), text);
}

#[wasm_bindgen_test]
fn never_throws_on_garbage() {
    for html in [
        "",
        "<",
        "</",
        "<<>>",
        "<a b='",
        "&#xffffffff;",
        "<!--",
        "\u{0}<p\u{0}>",
    ] {
        let _ = parse(html);
        let _ = Document::new(html).errors();
        let _ = to_text(html, None).unwrap();
        let _ = to_markdown(html, None).unwrap();
        let _ = select(html, "*").unwrap();
        let _ = to_html(html);
    }
}
