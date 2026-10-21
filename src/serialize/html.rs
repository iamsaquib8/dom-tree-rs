//! Re-serialize a parsed tree back to HTML.
//!
//! Iterative (explicit stack), so it handles arbitrarily deep trees without
//! risking a stack overflow. Text and attribute values are escaped; the content
//! of raw-text elements (`<script>`/`<style>`) is emitted verbatim, so a
//! `parse` → `to_html` → `parse` round-trip is stable.
//!
//! Editing can put an end tag inside an element whose content ends only there
//! — `<script>`, `<style>`, `<textarea>`, `<title>` — with
//! `set_text(script, "</script><img …>")`, or an element or comment moved
//! into one. Such a `</script` is written `<\/script`, so the output never
//! closes the element early. A parsed document never has one (the tokenizer
//! ends the content at the end tag), so its output is unchanged.

use alloc::string::String;
use alloc::vec::Vec;

use crate::node::NodeKind;
use crate::scan;
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
        // Top-level nodes are linked siblings: push them last to first.
        let mut root = self.roots().last();
        while let Some(r) = root {
            stack.push(Step::Open(r));
            root = r.prev_sibling();
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
    /// For a `<script>`, `<style>`, `<textarea>` or `<title>` it is exactly
    /// the content [`NodeRef::to_html`] writes, with any end tag of the
    /// element itself written `<\/…` (see [`Dom::set_text`]).
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
        if let Some(tag) = self.tag_name().filter(|t| tags::ends_only_at_end_tag(t)) {
            neutralize_end_tags(&mut out, 0, tag);
        }
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
    /// The end tag of the outermost element in the output whose content ends
    /// only at its end tag.
    CloseTextOnly(&'a str),
}

/// Serialize the steps on `stack` into `out`.
fn process(out: &mut String, stack: &mut Vec<Step<'_>>) {
    // Where the content of the outermost element that ends only at its end
    // tag starts, while one is open. Only that one needs checking: up to its
    // end tag, everything in it — nested ones included — is read as text.
    let mut text_only = None;
    while let Some(step) = stack.pop() {
        let tag = match step {
            Step::Open(node) => {
                write_open(out, node, stack, &mut text_only);
                continue;
            }
            Step::Close(tag) => tag,
            Step::CloseTextOnly(tag) => {
                if let Some(start) = text_only.take() {
                    neutralize_end_tags(out, start, tag);
                }
                tag
            }
        };
        out.push_str("</");
        out.push_str(tag);
        out.push('>');
    }
}

/// Write `node`'s start tag (pushing its children and end tag on `stack`) or
/// the whole node. `text_only` is where the content of the outermost element
/// whose content ends only at its end tag started, if one is open.
fn write_open<'a>(
    out: &mut String,
    node: NodeRef<'a>,
    stack: &mut Vec<Step<'a>>,
    text_only: &mut Option<usize>,
) {
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
            stack.push(if text_only.is_none() && tags::ends_only_at_end_tag(tag) {
                *text_only = Some(out.len());
                Step::CloseTextOnly(tag)
            } else {
                Step::Close(tag)
            });
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

/// Rewrite every `</tag` in `out[start..]` that would end the element `tag`
/// early as `<\/tag` (`\/` is `/` inside JavaScript and CSS strings). Like
/// the tokenizer, it matches `tag` ASCII case-insensitively, followed by
/// whitespace, `/` or `>`. A `</tag` at the very end is left alone: the
/// element's own end tag follows it, and `</tag<` ends nothing (parsed
/// content can end that way: `<script>x</script</script>`).
fn neutralize_end_tags(out: &mut String, start: usize, tag: &str) {
    let Some(content) = out.get(start..) else {
        return;
    };
    let bytes = content.as_bytes();
    // Built only if something needs rewriting: parsed content never does.
    let mut fixed = String::new();
    let mut copied = 0;
    let mut at = 0;
    while let Some(i) = bytes.get(at..).and_then(|b| scan::find(b"</", b)) {
        at += i + 2;
        if let (Some(name), Some(b)) = (bytes.get(at..at + tag.len()), bytes.get(at + tag.len())) {
            if name.eq_ignore_ascii_case(tag.as_bytes())
                && matches!(b, b'\t' | b'\n' | b'\x0c' | b'\r' | b' ' | b'/' | b'>')
            {
                fixed.push_str(content.get(copied..at - 1).unwrap_or_default());
                fixed.push('\\');
                copied = at - 1;
            }
        }
    }
    if copied > 0 {
        fixed.push_str(content.get(copied..).unwrap_or_default());
        out.truncate(start);
        out.push_str(&fixed);
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
