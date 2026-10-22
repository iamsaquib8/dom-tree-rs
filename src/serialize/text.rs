//! Readable plain text (`to_text`): roughly what a person reads on the page.
//!
//! Not `textContent`: hidden elements (`<head>`, `<script>`, `<style>`, …) are
//! dropped, whitespace collapses as in a browser, block elements start a new
//! line and paragraphs are separated by one blank line, `<br>` breaks the line,
//! table cells are tab-separated, and `<pre>`/`<textarea>` text is kept
//! verbatim. One iterative pass over the tree (`walk`), so depth is no concern.

use alloc::string::String;

use super::{is_space, push_verbatim, skip_newline, walk};
use crate::node::{NodeId, NodeKind};
use crate::tags::{self, Display};
use crate::tree::{Dom, NodeRef};

impl Dom {
    /// The document as readable plain text.
    ///
    /// Unlike [`NodeRef::text_content`], this is what a reader sees: no
    /// `<head>`/`<script>`/`<style>` content, whitespace collapsed, a new line
    /// for each block element, a blank line between paragraphs, `<br>` as a line
    /// break, tab-separated table cells and `<pre>` kept verbatim. The result
    /// has no trailing whitespace on lines, no runs of blank lines, and no
    /// leading or trailing blank lines.
    ///
    /// ```
    /// let dom = domtree::parse(
    ///     "<head><title>T</title></head><h1>Hello</h1>\
    ///      <p>Some   <b>bold</b>\n text.<br>Next line.</p><script>x()</script>",
    /// );
    /// assert_eq!(dom.to_text(), "Hello\n\nSome bold text.\nNext line.");
    /// ```
    pub fn to_text(&self) -> String {
        let mut text = Text::default();
        for root in self.roots() {
            walk(root, |node, enter| text.visit(node, enter));
        }
        text.finish()
    }
}

impl NodeRef<'_> {
    /// This node's subtree as readable plain text; see [`Dom::to_text`].
    ///
    /// Hidden elements are left out only *inside* the node: called on a
    /// `<title>`, `<script>`, `<style>` or `<template>` itself, it returns
    /// that element's text — the page title, or the script's or style
    /// sheet's source as written (not collapsed).
    ///
    /// ```
    /// let dom = domtree::parse("<ul><li>One</li><li>Two <i>too</i></li></ul>");
    /// assert_eq!(dom.root().unwrap().to_text(), "One\nTwo too");
    ///
    /// let dom = domtree::parse(
    ///     "<head><title> My  page </title><script>\nif (a  < b) {}\n</script></head>",
    /// );
    /// assert_eq!(dom.find_by_tag("title").next().unwrap().to_text(), "My page");
    /// assert_eq!(dom.find_by_tag("script").next().unwrap().to_text(), "if (a  < b) {}");
    /// assert_eq!(dom.find_by_tag("head").next().unwrap().to_text(), ""); // hidden inside
    /// ```
    pub fn to_text(&self) -> String {
        let mut text = Text {
            root: Some(self.id()),
            ..Text::default()
        };
        walk(*self, |node, enter| text.visit(node, enter));
        text.finish()
    }
}

#[derive(Default)]
struct Text {
    out: String,
    /// Newlines owed before the next visible text (at most 2: one blank line).
    breaks: u8,
    /// Tabs owed before the next visible text (table cells).
    tabs: u32,
    /// A collapsed run of whitespace is pending.
    space: bool,
    /// Depth of `<pre>`/`<textarea>`, inside which whitespace is kept.
    pre: u32,
    /// The next verbatim text opens a `<pre>`: drop one leading newline.
    fresh: bool,
    /// Depth of lists; a nested list has no blank lines around it.
    lists: u32,
    /// At the start of a table cell, whose separator (tab or row break) is
    /// already owed: a block opening inside it adds no line break.
    cell: bool,
    /// The node `NodeRef::to_text` was called on: shown even if hidden.
    root: Option<NodeId>,
}

impl Text {
    fn visit(&mut self, node: NodeRef<'_>, enter: bool) -> bool {
        let tag = match node.kind() {
            NodeKind::Element { tag } => tag,
            NodeKind::Text(t) if self.pre > 0 => {
                self.verbatim(t);
                return false;
            }
            NodeKind::Text(t) => {
                self.collapse(t);
                return false;
            }
            NodeKind::Comment(_) | NodeKind::Doctype(_) => return false,
        };
        let list = matches!(tag, "ul" | "ol" | "menu" | "dl");
        if list && !enter {
            self.lists = self.lists.saturating_sub(1);
        }
        let root = self.root == Some(node.id());
        match tags::display(tag) {
            Display::Hidden if !root => return false,
            // The node itself, though hidden: a block. A `<script>` or
            // `<style>` reads verbatim, as its source.
            Display::Hidden => {
                self.newlines(1);
                if matches!(tag, "script" | "style") {
                    if enter {
                        self.pre += 1;
                        self.fresh = true;
                    } else {
                        self.pre = self.pre.saturating_sub(1);
                    }
                }
            }
            Display::Paragraph if list && self.lists > 0 => self.newlines(1),
            Display::Paragraph => self.newlines(2),
            Display::Block => self.newlines(1),
            Display::Inline => {}
        }
        if list && enter {
            self.lists += 1;
        }
        match tag {
            "br" if enter && self.pre > 0 => {
                self.fresh = false;
                self.content();
                self.out.push('\n');
            }
            "br" if enter => self.breaks = (self.breaks + 1).min(2),
            "pre" | "textarea" if enter => {
                self.pre += 1;
                self.fresh = true;
            }
            "pre" | "textarea" => {
                self.pre = self.pre.saturating_sub(1);
                // Trailing blank lines inside a `<pre>` are not content.
                let end = self.out.trim_end().len();
                self.out.truncate(end);
            }
            "td" | "th" if enter => {
                if is_cell(node.prev_element_sibling()) {
                    self.breaks = 0;
                    self.tabs += 1;
                }
                self.cell = true;
            }
            "td" | "th" => {
                // Breaks owed after the cell's own text (a closing `<p>`, a
                // trailing `<br>`) end with the cell: the next cell or row
                // decides what follows. An empty cell keeps its row's break.
                if !self.cell {
                    self.breaks = 0;
                }
                self.cell = false;
            }
            _ => {}
        }
        true
    }

    fn newlines(&mut self, n: u8) {
        if !self.cell {
            self.breaks = self.breaks.max(n);
            self.tabs = 0;
        }
    }

    fn collapse(&mut self, text: &str) {
        for (i, word) in text.split(is_space).enumerate() {
            if i > 0 {
                self.space = true;
            }
            if !word.is_empty() {
                self.content();
                self.out.push_str(word);
            }
        }
    }

    fn verbatim(&mut self, text: &str) {
        let text = if core::mem::take(&mut self.fresh) {
            skip_newline(text)
        } else {
            text
        };
        if !text.is_empty() {
            self.content();
            push_verbatim(&mut self.out, text);
        }
    }

    /// Settle the pending separators before visible text.
    fn content(&mut self) {
        if self.out.is_empty() {
            // Nothing to separate from: leading separators are dropped.
        } else if self.breaks > 0 {
            // Newlines already there (from `<pre>` text) count; look no further
            // back than needed, or long blank runs would make this quadratic.
            let want = usize::from(self.breaks);
            let have = self
                .out
                .bytes()
                .rev()
                .take(want)
                .take_while(|&b| b == b'\n')
                .count();
            for _ in have..want {
                self.out.push('\n');
            }
            for _ in 0..self.tabs {
                self.out.push('\t');
            }
        } else if self.tabs > 0 {
            for _ in 0..self.tabs {
                self.out.push('\t');
            }
        } else if self.space && !self.out.ends_with('\n') {
            self.out.push(' ');
        }
        self.breaks = 0;
        self.tabs = 0;
        self.space = false;
        self.cell = false;
    }

    fn finish(mut self) -> String {
        let end = self.out.trim_end().len();
        self.out.truncate(end);
        // Only `<pre>` text can open with blank lines; drop them but keep the
        // first line's indentation.
        let first = self.out.find(|c: char| !c.is_whitespace()).unwrap_or(0);
        let start = self
            .out
            .get(..first)
            .and_then(|lead| lead.rfind('\n'))
            .map_or(0, |i| i + 1);
        self.out.drain(..start);
        self.out
    }
}

fn is_cell(node: Option<NodeRef<'_>>) -> bool {
    matches!(node.and_then(|n| n.tag_name()), Some("td" | "th"))
}
