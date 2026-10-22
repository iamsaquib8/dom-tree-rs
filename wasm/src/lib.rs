//! WebAssembly / JavaScript bindings for [`domq`](https://crates.io/crates/domq).
//!
//! Parse messy HTML, query it with CSS selectors, and get readable text,
//! Markdown, JSON or repaired HTML out — in the browser, on the edge (Workers,
//! Deno, Bun) or in Node, from one small `.wasm` with zero JS dependencies.
//!
//! Structured results (matches, the tree, errors) are serialized to JSON in
//! Rust and turned into plain JS objects with a single `JSON.parse`: one
//! string crossing per call, no `serde-wasm-bindgen`.
//!
//! The crate is `no_std` (plus `alloc`) so the module doesn't carry std's
//! panic machinery (~2.4 KB gzipped, measured). The parser never panics; if
//! anything ever did, the module traps (`RuntimeError: unreachable`).

#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use domq::{Dom, NodeRef, Selector};
use wasm_bindgen::prelude::*;

/// The same allocator Rust's `std` uses on `wasm32-unknown-unknown`.
#[global_allocator]
static ALLOC: dlmalloc::GlobalDlmalloc = dlmalloc::GlobalDlmalloc;

/// Trap without formatting a message. (Tests link `std` and its handler. So
/// does building the library together with the tests, as `cargo clippy
/// --all-targets` does: the test-only dependencies enable wasm-bindgen's `std`.
/// Check the library and `--tests` separately instead.)
#[cfg(all(target_arch = "wasm32", not(test)))]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    core::arch::wasm32::unreachable()
}

#[wasm_bindgen(typescript_custom_section)]
const TS_TYPES: &str = r#"
/** An element in the parsed tree. */
export interface DomElement {
  type: "element";
  /** Lower-cased tag name, e.g. `"div"`. */
  tag: string;
  /** Attributes (names lower-cased, values entity-decoded; boolean attributes are `""`). */
  attrs: Record<string, string>;
  children: DomNode[];
}

/** Any node in the parsed tree. Text is already entity-decoded. */
export type DomNode =
  | DomElement
  | { type: "text"; value: string }
  | { type: "comment"; value: string }
  | { type: "doctype"; value: string };

/** The parsed document, as returned by `parse()` and `Document#tree()`. */
export interface DomDocument {
  type: "document";
  /** How many problems the parser recovered from (0 = clean input). See `errors()`. */
  errors: number;
  /** Top-level nodes, in document order. */
  children: DomNode[];
}

/** An element matched by a CSS selector. */
export interface Match {
  /** Lower-cased tag name. */
  tag: string;
  /** The element's attributes. */
  attrs: Record<string, string>;
  /** Readable text of the element (what `toText` gives: no scripts, collapsed whitespace). */
  text: string;
  /** The element's outer HTML (re-serialized, so it is well-formed). */
  html: string;
  /**
   * The element's position in a depth-first walk of the whole tree (0-based,
   * counting text, comment and doctype nodes too) — the order in which
   * `tree()` lists nodes. Use it to find a match in the tree.
   */
  nodeIndex: number;
}

/** A problem the parser recovered from. Parsing itself never fails. */
export interface ParseError {
  /** Index into the input string (a normal JS string index) where it happened. */
  position: number;
  /** Short description, e.g. `"element implicitly closed"`. */
  message: string;
}

/**
 * Thrown (as an `Error` with `name === "SelectorError"`) when a CSS selector is
 * invalid or unsupported, e.g. `"a:hover"` or `"div >"`.
 */
export interface SelectorError extends Error {
  name: "SelectorError";
  /** Index into the selector string where the problem is. */
  position: number;
}
"#;

// --- the Document class ----------------------------------------------------

/// A parsed HTML document. Parse once, then query it as often as you like.
///
/// Free it with `free()` (or a `using` declaration) when you are done, or let
/// the garbage collector do it.
#[wasm_bindgen]
pub struct Document {
    dom: Dom,
    /// Parse errors with positions already converted to JS string indices.
    errors: Vec<(usize, &'static str)>,
}

#[wasm_bindgen]
impl Document {
    /// Parse `html` (a full page or a fragment). Never throws: broken markup is
    /// repaired and the repairs are listed by `errors()`.
    #[wasm_bindgen(constructor)]
    pub fn new(html: &str) -> Document {
        let dom = domq::parse(html);
        let errors = js_positions(html, &dom);
        Document { dom, errors }
    }

    /// Same as `new Document(html)`.
    pub fn parse(html: &str) -> Document {
        Document::new(html)
    }

    /// Every element matching a CSS `selector`, in document order. Throws a
    /// `SelectorError` if the selector is invalid.
    #[wasm_bindgen(unchecked_return_type = "Match[]")]
    pub fn select(&self, selector: &str) -> Result<JsValue, JsValue> {
        let sel = compile(selector)?;
        let mut out = String::from("[");
        for node in self.dom.select(&sel) {
            if out.len() > 1 {
                out.push(',');
            }
            push_match(&mut out, node);
        }
        out.push(']');
        Ok(json(&out))
    }

    /// The first element matching a CSS `selector`, or `undefined`. Throws a
    /// `SelectorError` if the selector is invalid.
    #[wasm_bindgen(js_name = selectFirst, unchecked_return_type = "Match | undefined")]
    pub fn select_first(&self, selector: &str) -> Result<JsValue, JsValue> {
        let sel = compile(selector)?;
        Ok(match self.dom.select_first(&sel) {
            Some(node) => {
                let mut out = String::new();
                push_match(&mut out, node);
                json(&out)
            }
            None => JsValue::UNDEFINED,
        })
    }

    /// Readable plain text: no `<script>`/`<style>`/`<head>`, whitespace
    /// collapsed, a line per block, a blank line between paragraphs. With a
    /// `selector`, the text of every match, separated by a blank line.
    #[wasm_bindgen(js_name = toText)]
    pub fn to_text(&self, selector: Option<String>) -> Result<String, JsValue> {
        self.each(selector, Dom::to_text, |n| n.to_text())
    }

    /// Markdown (CommonMark + GFM tables): headings, lists, links, images,
    /// code blocks, tables, quotes. With a `selector`, every match converted
    /// and separated by a blank line.
    #[wasm_bindgen(js_name = toMarkdown)]
    pub fn to_markdown(&self, selector: Option<String>) -> Result<String, JsValue> {
        self.each(selector, Dom::to_markdown, |n| n.to_markdown())
    }

    /// The document re-serialized as HTML: every tag closed and nested
    /// properly, attributes quoted, text escaped.
    #[wasm_bindgen(js_name = toHtml)]
    pub fn to_html(&self) -> String {
        self.dom.to_html()
    }

    /// The whole tree as plain JS objects.
    #[wasm_bindgen(unchecked_return_type = "DomDocument")]
    pub fn tree(&self) -> JsValue {
        json(&self.dom.to_json())
    }

    /// Same as `tree()`, so `JSON.stringify(doc)` gives the JSON tree.
    #[wasm_bindgen(js_name = toJSON, unchecked_return_type = "DomDocument")]
    pub fn to_json(&self) -> JsValue {
        self.tree()
    }

    /// The problems the parser recovered from (empty for clean input).
    #[wasm_bindgen(unchecked_return_type = "ParseError[]")]
    pub fn errors(&self) -> JsValue {
        let mut out = String::from("[");
        for &(position, message) in &self.errors {
            if out.len() > 1 {
                out.push(',');
            }
            out.push_str("{\"position\":");
            push_uint(&mut out, position);
            out.push_str(",\"message\":");
            push_str(&mut out, message);
            out.push('}');
        }
        out.push(']');
        json(&out)
    }
}

impl Document {
    /// The whole document through `whole`, or every match of `selector`
    /// through `part`, joined by blank lines (empty results are skipped).
    fn each(
        &self,
        selector: Option<String>,
        // Function pointers, not generics: one copy of this code, not two.
        whole: fn(&Dom) -> String,
        part: fn(NodeRef<'_>) -> String,
    ) -> Result<String, JsValue> {
        let Some(selector) = selector else {
            return Ok(whole(&self.dom));
        };
        let sel = compile(&selector)?;
        let mut out = String::new();
        for node in self.dom.select(&sel) {
            let piece = part(node);
            if piece.is_empty() {
                continue;
            }
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str(&piece);
        }
        Ok(out)
    }
}

// --- one-shot functions ----------------------------------------------------

/// Parse `html` into a tree of plain JS objects (see `DomDocument`).
#[wasm_bindgen(unchecked_return_type = "DomDocument")]
pub fn parse(html: &str) -> JsValue {
    json(&domq::parse(html).to_json())
}

/// Parse `html` and return the tree as a JSON string (the same shape as
/// `parse()`), e.g. to send it over the network without re-encoding.
#[wasm_bindgen(js_name = parseJson)]
pub fn parse_json(html: &str) -> String {
    domq::parse(html).to_json()
}

/// Every element of `html` matching a CSS `selector`. Throws a
/// `SelectorError` if the selector is invalid.
#[wasm_bindgen(unchecked_return_type = "Match[]")]
pub fn select(html: &str, selector: &str) -> Result<JsValue, JsValue> {
    compile(selector)?;
    Document::new(html).select(selector)
}

/// The first element of `html` matching a CSS `selector`, or `undefined`.
/// Throws a `SelectorError` if the selector is invalid.
#[wasm_bindgen(js_name = selectFirst, unchecked_return_type = "Match | undefined")]
pub fn select_first(html: &str, selector: &str) -> Result<JsValue, JsValue> {
    compile(selector)?;
    Document::new(html).select_first(selector)
}

/// `html` as readable plain text — or, with a `selector`, the text of every
/// match separated by a blank line.
#[wasm_bindgen(js_name = toText)]
pub fn to_text(html: &str, selector: Option<String>) -> Result<String, JsValue> {
    Document::new(html).to_text(selector)
}

/// `html` as Markdown — or, with a `selector`, every match converted and
/// separated by a blank line.
#[wasm_bindgen(js_name = toMarkdown)]
pub fn to_markdown(html: &str, selector: Option<String>) -> Result<String, JsValue> {
    Document::new(html).to_markdown(selector)
}

/// `html` parsed and re-serialized: the repaired, well-formed version.
#[wasm_bindgen(js_name = toHtml)]
pub fn to_html(html: &str) -> String {
    domq::parse(html).to_html()
}

/// The problems the parser recovered from while parsing `html`.
#[wasm_bindgen(unchecked_return_type = "ParseError[]")]
pub fn errors(html: &str) -> JsValue {
    Document::new(html).errors()
}

// --- helpers ---------------------------------------------------------------

// The three JS built-ins we need, bound directly (cheaper than `js-sys`).
#[wasm_bindgen]
extern "C" {
    /// `JSON.parse`. Only ever called on JSON we built, so it cannot throw.
    #[wasm_bindgen(js_namespace = JSON, js_name = parse)]
    fn json(text: &str) -> JsValue;

    type Error;

    #[wasm_bindgen(constructor)]
    fn new(message: &str) -> Error;

    #[wasm_bindgen(method, setter)]
    fn set_name(this: &Error, name: &str);

    #[wasm_bindgen(method, setter)]
    fn set_position(this: &Error, position: u32);
}

/// Parse a selector, or build the JS `SelectorError` to throw.
fn compile(selector: &str) -> Result<Selector, JsValue> {
    Selector::parse(selector).map_err(|e| {
        // Same text as `SelectorError`'s `Display`, without `core::fmt`.
        let mut message = String::from(e.message());
        message.push_str(" at byte ");
        push_uint(&mut message, e.position());
        let err = Error::new(&message);
        err.set_name("SelectorError");
        err.set_position(utf16_len(selector.get(..e.position()).unwrap_or(selector)) as u32);
        err.into()
    })
}

/// Append `{"tag":…,"attrs":{…},"text":…,"html":…,"nodeIndex":N}`.
fn push_match(out: &mut String, node: NodeRef<'_>) {
    out.push_str("{\"tag\":");
    push_str(out, node.tag_name().unwrap_or(""));
    out.push_str(",\"attrs\":{");
    for (i, (k, v)) in node.attributes().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_str(out, k);
        out.push(':');
        push_str(out, v);
    }
    out.push_str("},\"text\":");
    push_str(out, &node.to_text());
    out.push_str(",\"html\":");
    push_str(out, &node.outer_html());
    out.push_str(",\"nodeIndex\":");
    // The arena is allocated in document order (and this crate never removes
    // nodes), so the arena index is the pre-order position.
    push_uint(out, node.id().index());
    out.push('}');
}

/// Append `s` as a JSON string literal.
fn push_str(out: &mut String, s: &str) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str("\\u00");
                out.push(HEX[(c as usize) >> 4] as char);
                out.push(HEX[(c as usize) & 0xf] as char);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Append a non-negative integer without going through `core::fmt`.
fn push_uint(out: &mut String, mut n: usize) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    for &b in &buf[i..] {
        out.push(b as char);
    }
}

/// Length of `s` in UTF-16 code units (the unit of JS string indices).
fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// The document's parse errors, with byte offsets turned into JS string
/// indices. Errors come (almost) in source order, so a running cursor keeps
/// this linear in the input size.
fn js_positions(html: &str, dom: &Dom) -> Vec<(usize, &'static str)> {
    let ascii = html.is_ascii();
    let (mut byte, mut index) = (0, 0);
    let mut out = Vec::with_capacity(dom.errors().len());
    for e in dom.errors() {
        let mut at = e.position.min(html.len());
        if !ascii {
            while !html.is_char_boundary(at) {
                at -= 1;
            }
            if at < byte {
                (byte, index) = (0, 0);
            }
            index += utf16_len(html.get(byte..at).unwrap_or(""));
            byte = at;
            at = index;
        }
        out.push((at, e.message()));
    }
    out
}

// The tests live in tests/web.rs but are compiled into this crate: the
// cdylib-only crate type (see Cargo.toml) can't be linked by an integration
// test. Run them with `wasm-pack test --node`.
#[cfg(test)]
#[path = "../tests/web.rs"]
mod web;
