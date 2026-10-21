#![no_main]
//! Differential fuzz target: on the subset of HTML that dom-tree-rs claims to
//! handle like a browser, its tree must match html5ever's.
//!
//! Run with: `cargo +nightly fuzz run differential`
//!
//! The fuzzer's bytes do not go to the parsers directly: they drive a
//! generator that only writes markup from the **claimed subset**, so every
//! divergence it finds is a bug (in our parser, or in this description of
//! what we handle) rather than a known non-goal.
//!
//! # The subset
//!
//! Body content, as a fragment, built from:
//!
//! - **Blocks** `div section article header footer nav aside main blockquote
//!   figure`, holding blocks, paragraphs, headings, lists, `<hr>` and inline
//!   content.
//! - **Paragraphs and headings** `p h1 h2 h3`, holding inline content only.
//! - **Lists** `ul ol`, holding `li` (and whitespace); `li` holds anything a
//!   block holds. `li` appears only directly in a list.
//! - **Inline elements**: the formatting elements `b i em strong code small a`
//!   and the plain `span` and custom `x-card`, holding inline content only (no
//!   blocks inside inline elements). A formatting element never nests inside
//!   another of the same name: past three of them html5ever's "Noah's Ark"
//!   clause starts forgetting the outer ones, and their end tags then close
//!   different elements (adoption agency), as browsers do.
//! - **Void elements** `br img hr`, written `<br>` or `<br/>`.
//! - **Raw text and RCDATA**: `script`/`style` with code-like text (no `</`
//!   or `<!`), `title` with text, entities and stray `<b>`-like markup.
//! - **Text**: letters, digits, punctuation, whitespace (no `\r`, no NUL),
//!   non-ASCII, a `<` or `&` followed by a space, and `;`-terminated
//!   character references (`&amp;` `&lt;` `&gt;` `&quot;` `&nbsp;` `&copy;`
//!   `&mdash;` `&#65;` `&#x41;` `&#x20AC;` `&#128;`).
//! - **Comments** `<!--…-->` with letters and spaces.
//! - **Attributes** from `id class href title lang data-x hidden role`,
//!   double-, single- or un-quoted (unquoted: letters and digits only) or
//!   valueless, values with spaces, `<`, `>`, `=` and the references above;
//!   names and tag names in random letter case; duplicate names allowed.
//!
//! End tags are always written, except where omitting them is *simple*:
//!
//! - `</p>` when the next sibling is a block, list, heading, paragraph or
//!   `<hr>` (which closes it), or when it is the last child of a parent whose
//!   own end tag is written;
//! - `</li>` before the next `<li>`, or as the last child of its list;
//! - `</span>` / `</x-card>` as the last child of a parent whose end tag is
//!   written, unless that parent has the same name (its end tag would close
//!   the child and leave the parent open);
//! - any end tag at the very end of the input (everything still open).
//!
//! Out of the subset, on purpose: tables, forms, `select`, `template`,
//! `pre`/`textarea`/`listing` (leading-newline rules), `html`/`head`/`body`
//! and doctypes, formatting elements left open inside another element (the
//! adoption agency and active-formatting reconstruction), blocks inside
//! inline elements (`<p><span><div>`), stray end tags, `/>` on non-void
//! elements, legacy references without `;`, `\r` (newline normalization).
//!
//! # Normalization
//!
//! html5ever parses the input as a fragment in a `<body>` context (so no
//! implied `html`/`head`/`body` and no whitespace dropped before the body);
//! its fragment root's children are compared with our top-level nodes. Both
//! trees are flattened to a pre-order sequence of `Open(tag, attributes)`,
//! `Close`, `Text`, `Comment`, where:
//!
//! - adjacent text is merged (we split text at a stray `<`; html5ever does
//!   not);
//! - attributes keep only the first occurrence of each name (html5ever drops
//!   duplicates; we keep them, but `NodeRef::attr` returns the first).
//!
//! Nothing else is normalized: tag and attribute names, attribute values,
//! decoded text, raw text and comments must match exactly.

use std::fmt::Write;

use html5ever::tendril::TendrilSink;
use html5ever::{local_name, ns, QualName};
use libfuzzer_sys::arbitrary::{Result, Unstructured};
use libfuzzer_sys::fuzz_target;
use markup5ever_rcdom::{Handle, NodeData, RcDom};

// --- the generator --------------------------------------------------------

#[derive(Debug)]
enum Node {
    Text(String),
    Comment(String),
    Void {
        tag: &'static str,
        attrs: String,
        slash: bool,
    },
    Raw {
        tag: &'static str,
        text: String,
    },
    Element {
        tag: &'static str,
        attrs: String,
        children: Vec<Node>,
    },
}

const BLOCKS: &[&str] = &[
    "div",
    "section",
    "article",
    "header",
    "footer",
    "nav",
    "aside",
    "main",
    "blockquote",
    "figure",
];
const FORMATTING: &[&str] = &["b", "i", "em", "strong", "code", "small", "a"];
const PLAIN_INLINE: &[&str] = &["span", "x-card"];
const MAX_DEPTH: u32 = 7;

struct Gen<'a, 'b> {
    u: &'b mut Unstructured<'a>,
    /// Node budget, so inputs stay small and fast.
    budget: u32,
}

impl Gen<'_, '_> {
    fn pick<T: Copy>(&mut self, items: &[T]) -> Result<T> {
        Ok(*self.u.choose(items)?)
    }

    fn text(&mut self) -> Result<String> {
        const PIECES: &[&str] = &[
            "a",
            "b",
            "z",
            "Q",
            "0",
            "7",
            " ",
            " ",
            "\n",
            "\t",
            ".",
            ",",
            "!",
            "?",
            "'",
            "\"",
            ">",
            "=",
            "/",
            ";",
            "#",
            "-",
            "(",
            ")",
            "é",
            "🦀",
            "\u{a0}",
            "< ",
            "& ",
            "&amp;",
            "&lt;",
            "&gt;",
            "&quot;",
            "&nbsp;",
            "&copy;",
            "&mdash;",
            "&#65;",
            "&#x41;",
            "&#x20AC;",
            "&#128;",
            "word",
            "two words",
        ];
        let n = self.u.int_in_range(1..=8)?;
        let mut s = String::new();
        for _ in 0..n {
            s.push_str(self.pick(PIECES)?);
        }
        Ok(s)
    }

    fn attrs(&mut self) -> Result<String> {
        const NAMES: &[&str] = &[
            "id", "class", "href", "title", "lang", "data-x", "hidden", "role", "ID", "Class",
            "DATA-X",
        ];
        const VALUE: &[&str] = &[
            "a", "b", "1", " ", "x y", "<", ">", "=", "/", "&amp;", "&lt;", "&quot;", "&#65;",
            "&nbsp;", "é",
        ];
        let mut out = String::new();
        for _ in 0..self.u.int_in_range(0..=3)? {
            let name = self.pick(NAMES)?;
            let mut value = String::new();
            for _ in 0..self.u.int_in_range(0..=3)? {
                value.push_str(self.pick(VALUE)?);
            }
            match self.u.int_in_range(0..=3)? {
                0 => write!(out, " {name}=\"{value}\""),
                1 => write!(out, " {name}='{value}'"),
                2 => {
                    let bare: String = value
                        .chars()
                        .filter(|c| c.is_ascii_alphanumeric())
                        .collect();
                    if bare.is_empty() {
                        write!(out, " {name}")
                    } else {
                        write!(out, " {name}={bare}")
                    }
                }
                _ => write!(out, " {name}"),
            }
            .ok();
        }
        Ok(out)
    }

    fn spend(&mut self) -> bool {
        if self.budget == 0 {
            return false;
        }
        self.budget -= 1;
        true
    }

    /// Inline content. `open`: the formatting elements it sits inside (bit
    /// `i` for `FORMATTING[i]`); none of them may open again in here.
    fn inline(&mut self, depth: u32, open: u8) -> Result<Vec<Node>> {
        let mut out = Vec::new();
        for _ in 0..self.u.int_in_range(0..=4)? {
            if !self.spend() {
                break;
            }
            let node = match self.u.int_in_range(0..=9)? {
                0..=3 => Node::Text(self.text()?),
                4 => Node::Comment(self.comment()?),
                5 => Node::Void {
                    tag: self.pick(&["br", "img"])?,
                    attrs: self.attrs()?,
                    slash: self.u.arbitrary()?,
                },
                6 if depth < MAX_DEPTH => self.raw()?,
                _ if depth < MAX_DEPTH => {
                    let tag = if self.u.arbitrary()? {
                        self.pick(PLAIN_INLINE)?
                    } else {
                        self.pick(FORMATTING)?
                    };
                    let bit = FORMATTING
                        .iter()
                        .position(|f| *f == tag)
                        .map_or(0, |i| 1u8 << i);
                    if open & bit != 0 {
                        Node::Text(self.text()?)
                    } else {
                        Node::Element {
                            tag,
                            attrs: self.attrs()?,
                            children: self.inline(depth + 1, open | bit)?,
                        }
                    }
                }
                _ => Node::Text(self.text()?),
            };
            out.push(node);
        }
        Ok(out)
    }

    /// Block ("flow") content.
    fn flow(&mut self, depth: u32) -> Result<Vec<Node>> {
        let mut out = Vec::new();
        for _ in 0..self.u.int_in_range(0..=5)? {
            if !self.spend() {
                break;
            }
            let node = match self.u.int_in_range(0..=11)? {
                0 | 1 => Node::Text(self.text()?),
                2 => Node::Comment(self.comment()?),
                3 => Node::Void {
                    tag: self.pick(&["br", "img", "hr"])?,
                    attrs: self.attrs()?,
                    slash: self.u.arbitrary()?,
                },
                4 if depth < MAX_DEPTH => Node::Element {
                    tag: self.pick(BLOCKS)?,
                    attrs: self.attrs()?,
                    children: self.flow(depth + 1)?,
                },
                5 | 6 if depth < MAX_DEPTH => Node::Element {
                    tag: self.pick(&["p", "p", "h1", "h2", "h3"])?,
                    attrs: self.attrs()?,
                    children: self.inline(depth + 1, 0)?,
                },
                7 if depth < MAX_DEPTH => {
                    let mut items = Vec::new();
                    for _ in 0..self.u.int_in_range(0..=4)? {
                        if self.u.ratio(1, 4)? {
                            items.push(Node::Text(self.pick(&[" ", "\n  ", "\t"])?.to_string()));
                        }
                        items.push(Node::Element {
                            tag: "li",
                            attrs: self.attrs()?,
                            children: self.flow(depth + 2)?,
                        });
                    }
                    Node::Element {
                        tag: self.pick(&["ul", "ol"])?,
                        attrs: self.attrs()?,
                        children: items,
                    }
                }
                8 if depth < MAX_DEPTH => self.raw()?,
                _ => {
                    let mut inline = self.inline(depth, 0)?;
                    match inline.pop() {
                        Some(n) => n,
                        None => Node::Text(self.text()?),
                    }
                }
            };
            out.push(node);
        }
        Ok(out)
    }

    fn comment(&mut self) -> Result<String> {
        let mut s = String::new();
        for _ in 0..self.u.int_in_range(1..=3)? {
            s.push_str(self.pick(&["note", " ", "x", "TODO"])?);
        }
        Ok(s)
    }

    fn raw(&mut self) -> Result<Node> {
        let tag = self.pick(&["script", "style", "title"])?;
        let pieces: &[&str] = if tag == "title" {
            &["Title", " ", "&amp;", "&lt;", "<b>", "</b>", "a < b", "é"]
        } else {
            &[
                "x", " ", "=", "1;", "(a)", "{ }", "a < b", "b > c", "'s'", "\"d\"", "&amp;",
            ]
        };
        let mut text = String::new();
        for _ in 0..self.u.int_in_range(0..=4)? {
            text.push_str(self.pick(pieces)?);
        }
        Ok(Node::Raw { tag, text })
    }
}

// --- writing the generated tree as HTML -----------------------------------

/// Start tags that close an open `<p>` in both parsers (of those generated).
fn closes_p(node: &Node) -> bool {
    match node {
        Node::Void { tag, .. } => *tag == "hr",
        Node::Element { tag, .. } => {
            BLOCKS.contains(tag) || matches!(*tag, "p" | "h1" | "h2" | "h3" | "ul" | "ol")
        }
        _ => false,
    }
}

fn case(u: &mut Unstructured<'_>, tag: &str) -> String {
    match u.int_in_range(0..=5).unwrap_or(0) {
        0 => tag.to_ascii_uppercase(),
        1 => {
            let mut c = tag.chars();
            c.next()
                .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                .unwrap_or_default()
        }
        _ => tag.to_string(),
    }
}

/// Write `nodes`. `closing_parent`: the parent's tag, if its end tag will be
/// written — so a last child may leave its own out where that is simple.
/// `at_eof`: nothing follows these nodes in the whole input.
fn write_nodes(
    out: &mut String,
    u: &mut Unstructured<'_>,
    nodes: &[Node],
    closing_parent: Option<&str>,
    at_eof: bool,
) {
    for (i, node) in nodes.iter().enumerate() {
        let next = nodes.get(i + 1);
        let last = next.is_none();
        match node {
            Node::Text(t) => out.push_str(t),
            Node::Comment(c) => write!(out, "<!--{c}-->").unwrap_or(()),
            Node::Void { tag, attrs, slash } => {
                let tag = case(u, tag);
                write!(out, "<{tag}{attrs}{}>", if *slash { "/" } else { "" }).unwrap_or(());
            }
            Node::Raw { tag, text } => {
                let (open, close) = (case(u, tag), case(u, tag));
                write!(out, "<{open}>{text}</{close}>").unwrap_or(());
            }
            Node::Element {
                tag,
                attrs,
                children,
            } => {
                // May the end tag be left out?
                let omissible = match *tag {
                    "p" => next.is_some_and(closes_p) || (last && closing_parent.is_some()),
                    "li" => true, // next sibling is an <li>, or it is last in its list
                    // Not inside a parent of the same name: its end tag would
                    // close this element instead, leaving the parent open.
                    "span" | "x-card" => last && closing_parent.is_some_and(|p| p != *tag),
                    _ => false,
                } || (last && at_eof);
                let close = !omissible || u.ratio(1, 3).unwrap_or(true);
                write!(out, "<{}{attrs}>", case(u, tag)).unwrap_or(());
                let parent = close.then_some(*tag);
                write_nodes(out, u, children, parent, last && at_eof && !close);
                if close {
                    write!(out, "</{}>", case(u, tag)).unwrap_or(());
                }
            }
        }
    }
}

// --- comparing ------------------------------------------------------------

#[derive(Debug, PartialEq)]
enum Item {
    Open(String, Vec<(String, String)>),
    Close,
    Text(String),
    Comment(String),
}

fn push_text(seq: &mut Vec<Item>, t: &str) {
    if let Some(Item::Text(prev)) = seq.last_mut() {
        prev.push_str(t);
    } else {
        seq.push(Item::Text(t.to_string()));
    }
}

fn first_occurrences(attrs: impl Iterator<Item = (String, String)>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for (k, v) in attrs {
        if !out.iter().any(|(n, _)| *n == k) {
            out.push((k, v));
        }
    }
    out
}

fn ours(html: &str) -> Vec<Item> {
    enum Step<'a> {
        Node(domtree::NodeRef<'a>),
        Close,
    }
    let dom = domtree::parse(html);
    let mut seq = Vec::new();
    let mut stack: Vec<Step<'_>> = dom.roots().map(Step::Node).collect();
    stack.reverse();
    while let Some(step) = stack.pop() {
        let node = match step {
            Step::Close => {
                seq.push(Item::Close);
                continue;
            }
            Step::Node(n) => n,
        };
        match node.kind() {
            domtree::NodeKind::Element { tag } => {
                let attrs = first_occurrences(
                    node.attributes()
                        .map(|(k, v)| (k.to_string(), v.to_string())),
                );
                seq.push(Item::Open(tag.to_string(), attrs));
                stack.push(Step::Close);
                let kids: Vec<_> = node.children().collect();
                stack.extend(kids.into_iter().rev().map(Step::Node));
            }
            domtree::NodeKind::Text(t) => push_text(&mut seq, t),
            domtree::NodeKind::Comment(c) => seq.push(Item::Comment(c.to_string())),
            domtree::NodeKind::Doctype(_) => {}
        }
    }
    seq
}

fn theirs(html: &str) -> Vec<Item> {
    enum Step {
        Node(Handle),
        Close,
    }
    let context = QualName::new(None, ns!(html), local_name!("body"));
    let dom =
        html5ever::parse_fragment(RcDom::default(), Default::default(), context, vec![], false)
            .one(html);
    // Document -> <html> (the fragment root) -> the fragment's nodes.
    let root = dom.document.children.borrow().first().cloned();
    let mut stack: Vec<Step> = root
        .map(|r| {
            r.children
                .borrow()
                .iter()
                .rev()
                .cloned()
                .map(Step::Node)
                .collect()
        })
        .unwrap_or_default();
    let mut seq = Vec::new();
    while let Some(step) = stack.pop() {
        let node = match step {
            Step::Close => {
                seq.push(Item::Close);
                continue;
            }
            Step::Node(n) => n,
        };
        match &node.data {
            NodeData::Element { name, attrs, .. } => {
                let attrs = first_occurrences(
                    attrs
                        .borrow()
                        .iter()
                        .map(|a| (a.name.local.to_string(), a.value.to_string())),
                );
                seq.push(Item::Open(name.local.to_string(), attrs));
                stack.push(Step::Close);
                stack.extend(node.children.borrow().iter().rev().cloned().map(Step::Node));
            }
            NodeData::Text { contents } => push_text(&mut seq, &contents.borrow()),
            NodeData::Comment { contents } => seq.push(Item::Comment(contents.to_string())),
            _ => {}
        }
    }
    seq
}

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);
    let mut generator = Gen {
        u: &mut u,
        budget: 60,
    };
    let Ok(tree) = generator.flow(0) else {
        return;
    };
    let mut html = String::new();
    write_nodes(&mut html, &mut u, &tree, None, true);

    let (a, b) = (ours(&html), theirs(&html));
    if a != b {
        let at = a
            .iter()
            .zip(&b)
            .position(|(x, y)| x != y)
            .unwrap_or(a.len().min(b.len()));
        panic!(
            "dom-tree-rs and html5ever disagree on\n{html:?}\nfirst difference at item {at}:\n  ours:   {:?}\n  theirs: {:?}\nours:   {a:?}\ntheirs: {b:?}",
            a.get(at),
            b.get(at)
        );
    }
});
