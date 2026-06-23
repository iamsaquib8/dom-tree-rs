//! Browser/node tests for the wasm bindings.
//!
//! Run with: `wasm-pack test --node`

#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

#[wasm_bindgen_test]
fn parses_to_json() {
    let json = dom_tree_wasm::parse("<p class='x'>hi &amp; bye</p>");
    assert!(json.contains("\"type\":\"document\""));
    assert!(json.contains("\"tag\":\"p\""));
    assert!(json.contains("\"class\":\"x\""));
    assert!(json.contains("\"value\":\"hi & bye\""));
}

#[wasm_bindgen_test]
fn counts_errors() {
    assert_eq!(dom_tree_wasm::parse_error_count("<p>ok</p>"), 0);
    assert!(dom_tree_wasm::parse_error_count("<b><i>x</b></i>") > 0);
}
