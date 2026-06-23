//! WebAssembly bindings for [`dom-tree-rs`](https://crates.io/crates/dom-tree-rs).
//!
//! Parse messy HTML into a JSON tree from JavaScript — in the browser, on the
//! edge, or in any wasm runtime — without shipping a 100 KB+ parser.
//!
//! The functions return **JSON strings** (cheapest possible boundary crossing:
//! one copy, no `serde-wasm-bindgen`). Call `JSON.parse` on the result in JS.

use wasm_bindgen::prelude::*;

/// Parse an HTML document and return it as a JSON string.
///
/// Shape: `{ "type": "document", "errors": N, "children": [ ...nodes ] }`.
/// See the package README for the `DomNode` TypeScript type.
#[wasm_bindgen]
pub fn parse(html: &str) -> String {
    domtree::parse(html).to_json()
}

/// Number of recovery diagnostics produced while parsing `html`
/// (`0` means the input was clean).
#[wasm_bindgen(js_name = parseErrorCount)]
pub fn parse_error_count(html: &str) -> usize {
    domtree::parse(html).errors().len()
}

/// Parse `html`, then return a JSON **array** of the subtrees of every element
/// matching the CSS `selector` (see the core crate's `Dom::select` for the
/// supported subset).
///
/// Available when the crate is built with the `query` feature.
#[cfg(feature = "query")]
#[wasm_bindgen]
pub fn query(html: &str, selector: &str) -> String {
    let dom = domtree::parse(html);
    let mut out = String::from("[");
    let mut first = true;
    for node in dom.select(selector) {
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str(&node.to_json());
    }
    out.push(']');
    out
}
