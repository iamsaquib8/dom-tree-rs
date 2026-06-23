//! Zero-dependency JSON serializer.
//!
//! Iterative (explicit stack) so it can serialize arbitrarily deep trees
//! without risking a stack overflow — keeping the "never panics" promise even
//! on pathological input.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write as _;

use crate::node::NodeKind;
use crate::tree::{Dom, NodeRef};

impl Dom {
    /// Serialize the document to a compact JSON string.
    ///
    /// The shape is self-describing: every node is an object with a `"type"` of
    /// `element` / `text` / `comment` / `doctype`, elements carry `tag`,
    /// `attrs` (an object) and `children` (an array), and the whole document is
    /// wrapped in `{ "type": "document", "errors": N, "children": [...] }`.
    ///
    /// ```
    /// let dom = domtree::parse("<a href=\"/x\">hi</a>");
    /// let json = dom.to_json();
    /// assert!(json.contains("\"tag\":\"a\""));
    /// assert!(json.contains("\"href\":\"/x\""));
    /// assert!(json.contains("\"value\":\"hi\""));
    /// ```
    pub fn to_json(&self) -> String {
        let mut out = String::with_capacity(self.len() * 24 + 32);
        out.push_str("{\"type\":\"document\",\"errors\":");
        let _ = write!(out, "{}", self.errors().len());
        out.push_str(",\"children\":");
        write_forest(&mut out, self);
        out.push('}');
        out
    }
}

impl<'a> NodeRef<'a> {
    /// Serialize just this node and its subtree to a compact JSON string,
    /// using the same shape as [`Dom::to_json`].
    ///
    /// ```
    /// let dom = domtree::parse("<div><b>hi</b></div>");
    /// let b = dom.find_by_tag("b").next().unwrap();
    /// assert_eq!(b.to_json(), r#"{"type":"element","tag":"b","attrs":{},"children":[{"type":"text","value":"hi"}]}"#);
    /// ```
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        let mut stack: Vec<Step<'a>> = Vec::new();
        stack.push(Step::Open(*self));
        process(&mut out, &mut stack);
        out
    }
}

enum Step<'a> {
    Open(NodeRef<'a>),
    Lit(&'static str),
}

fn write_forest(out: &mut String, dom: &Dom) {
    out.push('[');
    let mut stack: Vec<Step<'_>> = Vec::new();
    // Push roots in reverse so they pop (and serialize) in document order.
    for r in dom.roots().collect::<Vec<_>>().into_iter().rev() {
        stack.push(Step::Open(r));
    }
    process(out, &mut stack);
    out.push(']');
}

fn process(out: &mut String, stack: &mut Vec<Step<'_>>) {
    while let Some(step) = stack.pop() {
        match step {
            Step::Lit(s) => out.push_str(s),
            Step::Open(node) => write_open(out, node, stack),
        }
    }
}

fn write_open<'a>(out: &mut String, node: NodeRef<'a>, stack: &mut Vec<Step<'a>>) {
    maybe_comma(out);
    match node.kind() {
        NodeKind::Element(e) => {
            out.push_str("{\"type\":\"element\",\"tag\":");
            write_str(out, e.name());
            out.push_str(",\"attrs\":{");
            let mut first = true;
            for (k, v) in e.attributes() {
                if !first {
                    out.push(',');
                }
                first = false;
                write_str(out, k);
                out.push(':');
                write_str(out, v);
            }
            out.push_str("},\"children\":[");
            // Close the children array + the object once the subtree is drained.
            stack.push(Step::Lit("]}"));
            // Push children in reverse (walk last_child -> prev_sibling) so they
            // pop in document order. No allocation.
            let mut child = node.last_child();
            while let Some(c) = child {
                stack.push(Step::Open(c));
                child = c.prev_sibling();
            }
        }
        NodeKind::Text(t) => {
            out.push_str("{\"type\":\"text\",\"value\":");
            write_str(out, t);
            out.push('}');
        }
        NodeKind::Comment(c) => {
            out.push_str("{\"type\":\"comment\",\"value\":");
            write_str(out, c);
            out.push('}');
        }
        NodeKind::Doctype(d) => {
            out.push_str("{\"type\":\"doctype\",\"value\":");
            write_str(out, d);
            out.push('}');
        }
    }
}

/// Emit a separating comma unless we're at the very start of the buffer or
/// right after an array opener (`[`) — the only places a node is written
/// without a preceding sibling.
fn maybe_comma(out: &mut String) {
    if !out.is_empty() && out.as_bytes().last() != Some(&b'[') {
        out.push(',');
    }
}

/// Write a JSON string literal with RFC 8259 escaping.
fn write_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str("\\u");
                push_hex4(out, c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn push_hex4(out: &mut String, v: u32) {
    const HEX: [u8; 16] = *b"0123456789abcdef";
    for shift in [12u32, 8, 4, 0] {
        let nib = ((v >> shift) & 0xf) as usize;
        out.push(HEX[nib] as char);
    }
}
