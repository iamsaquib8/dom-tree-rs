//! Re-serialize a parsed tree back to HTML.
//!
//! Iterative (explicit stack), so it handles arbitrarily deep trees without
//! risking a stack overflow. Text and attribute values are escaped; the content
//! of raw-text elements (`<script>`/`<style>`) is emitted verbatim, so a
//! `parse` → `to_html` → `parse` round-trip is stable.

use alloc::string::String;
use alloc::vec::Vec;

use crate::node::NodeKind;
use crate::tags;
use crate::tree::{Dom, NodeRef};

impl Dom {
    /// Serialize the whole document back to an HTML string.
    ///
    /// ```
    /// let dom = domtree::parse("<p class='x'>1 &lt; 2 &amp; 3</p>");
    /// assert_eq!(dom.to_html(), r#"<p class="x">1 &lt; 2 &amp; 3</p>"#);
    /// ```
    pub fn to_html(&self) -> String {
        let mut out = String::with_capacity(self.len() * 16);
        let mut stack: Vec<Step<'_>> = Vec::new();
        for r in self.roots().collect::<Vec<_>>().into_iter().rev() {
            stack.push(Step::Open(r));
        }
        process(&mut out, &mut stack);
        out
    }
}

impl<'a> NodeRef<'a> {
    /// Serialize just this node and its subtree back to an HTML string.
    ///
    /// ```
    /// let dom = domtree::parse("<div><img src=a.png><br></div>");
    /// let div = dom.find_by_tag("div").next().unwrap();
    /// assert_eq!(div.to_html(), r#"<div><img src="a.png"><br></div>"#);
    /// ```
    pub fn to_html(&self) -> String {
        let mut out = String::new();
        let mut stack: Vec<Step<'a>> = Vec::new();
        stack.push(Step::Open(*self));
        process(&mut out, &mut stack);
        out
    }

    /// The HTML of this node's children, without the node's own tags (the DOM
    /// `innerHTML`). Empty for text, comment and void nodes.
    ///
    /// ```
    /// let dom = domtree::parse("<div id=x><b>hi</b> there</div>");
    /// let div = dom.root().unwrap();
    /// assert_eq!(div.inner_html(), "<b>hi</b> there");
    /// ```
    pub fn inner_html(&self) -> String {
        let mut out = String::new();
        let mut stack: Vec<Step<'a>> = Vec::new();
        let mut child = self.last_child();
        while let Some(c) = child {
            stack.push(Step::Open(c));
            child = c.prev_sibling();
        }
        process(&mut out, &mut stack);
        out
    }

    /// This node and its subtree as HTML (the DOM `outerHTML`); the same as
    /// [`NodeRef::to_html`].
    ///
    /// ```
    /// let dom = domtree::parse("<div id=x><b>hi</b></div>");
    /// let b = dom.find_by_tag("b").next().unwrap();
    /// assert_eq!(b.outer_html(), "<b>hi</b>");
    /// ```
    pub fn outer_html(&self) -> String {
        self.to_html()
    }
}

enum Step<'a> {
    Open(NodeRef<'a>),
    Close(&'a str),
}

fn process(out: &mut String, stack: &mut Vec<Step<'_>>) {
    while let Some(step) = stack.pop() {
        match step {
            Step::Close(tag) => {
                out.push_str("</");
                out.push_str(tag);
                out.push('>');
            }
            Step::Open(node) => write_open(out, node, stack),
        }
    }
}

fn write_open<'a>(out: &mut String, node: NodeRef<'a>, stack: &mut Vec<Step<'a>>) {
    match node.kind() {
        NodeKind::Element { tag } => {
            out.push('<');
            out.push_str(tag);
            for (k, v) in node.attributes() {
                out.push(' ');
                out.push_str(k);
                if !v.is_empty() {
                    out.push_str("=\"");
                    escape_attr(out, v);
                    out.push('"');
                }
            }
            out.push('>');
            if tags::is_void(tag) {
                return; // void elements have no children and no end tag
            }
            stack.push(Step::Close(tag));
            // Push children in reverse so they emit in document order.
            let mut child = node.last_child();
            while let Some(c) = child {
                stack.push(Step::Open(c));
                child = c.prev_sibling();
            }
        }
        NodeKind::Text(t) => {
            // Raw-text element content (script/style) is emitted verbatim.
            let raw = node
                .parent()
                .and_then(|p| p.tag_name())
                .is_some_and(tags::is_raw_text);
            if raw {
                out.push_str(t);
            } else {
                escape_text(out, t);
            }
        }
        NodeKind::Comment(c) => {
            out.push_str("<!--");
            out.push_str(c);
            out.push_str("-->");
        }
        NodeKind::Doctype(d) => {
            out.push_str("<!doctype ");
            out.push_str(d);
            out.push('>');
        }
    }
}

fn escape_text(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
}

fn escape_attr(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
}
