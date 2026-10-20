//! # dom-tree-rs
//!
//! A tiny, **zero-dependency**, forgiving HTML parser. It turns messy
//! real-world HTML into a clean DOM tree (and JSON) and **never panics** on bad
//! input — ideal for the browser, the edge, or anywhere you don't want to pull
//! in a 100 KB+ parser.
//!
//! It is *not* a spec-compliant WHATWG parser (see
//! [`html5ever`](https://crates.io/crates/html5ever) for that) and *not* a
//! speed record (see [`tl`](https://crates.io/crates/tl)). It is the smallest
//! way to get a usable tree out of arbitrary HTML.
//!
//! ## Quick start
//!
//! ```
//! let dom = domtree::parse(r#"<ul id="list"><li class="item">Item 1</li></ul>"#);
//!
//! let li = dom.find_by_tag("li").next().unwrap();
//! assert_eq!(li.attr("class"), Some("item"));
//! assert_eq!(li.text_content(), "Item 1");
//!
//! // The first list element:
//! let ul = dom.root().unwrap();
//! assert_eq!(ul.tag_name(), Some("ul"));
//! assert_eq!(ul.children().count(), 1);
//! # #[cfg(feature = "json")]
//! # assert!(dom.to_json().contains("\"tag\":\"li\""));
//! ```
//!
//! ## What it handles
//!
//! Void elements (`<br>`, `<img>`, …), self-closing tags, comments, doctypes,
//! CDATA, raw text (`<script>`/`<style>`), boolean and unquoted attributes,
//! hyphenated/custom tags, common HTML entities, and the everyday "forgot a
//! closing tag" cases (`<li>`, `<p>`, table cells, …) via implicit closing.
//!
//! ## What it deliberately does *not* do
//!
//! Full WHATWG tree construction: no adoption-agency algorithm, no automatic
//! `<html>`/`<head>`/`<body>` insertion, no foster parenting. Anything it has
//! to recover from is recorded in [`Dom::errors`].
//!
//! ## Getting content out
//!
//! - [`Dom::to_text`] — readable plain text: no `<script>`/`<style>`/`<head>`,
//!   whitespace collapsed, a line per block, a blank line between paragraphs.
//! - [`Dom::to_markdown`] — CommonMark + GFM (headings, lists, links, images,
//!   code blocks, tables, quotes): for LLM prompts, RAG indexes and notes.
//! - [`Dom::to_html`], [`NodeRef::inner_html`], [`NodeRef::outer_html`] — HTML.
//!
//! ```
//! let dom = domtree::parse(
//!     "<article><h2>News</h2><p>It <em>works</em>!</p><script>track()</script></article>",
//! );
//! assert_eq!(dom.to_text(), "News\n\nIt works!");
//! # #[cfg(feature = "markdown")]
//! assert_eq!(dom.to_markdown(), "## News\n\nIt *works*!");
//! ```
//!
//! ## Features
//!
//! - `json` *(default)* — the zero-dependency [`Dom::to_json`] serializer.
//! - `markdown` *(default)* — the zero-dependency [`Dom::to_markdown`].
//! - `serde` — optional `serde::Serialize` for the tree (adds the `serde` dep).
//! - `query` — a small CSS-selector engine, [`Dom::select`].
//! - `std` — adds `std`-only conveniences; the crate is `no_std + alloc` by default.

#![no_std]
#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![warn(missing_docs)]

extern crate alloc;

#[cfg(any(feature = "std", test))]
extern crate std;

mod builder;
mod entities;
mod error;
mod node;
mod tags;
mod token;
mod tokenizer;
mod tree;

mod serialize;

#[cfg(feature = "query")]
mod query;

pub use error::{ErrorKind, ParseError};
pub use node::{NodeId, NodeKind};
pub use token::{AttrValue, Token};
pub use tokenizer::Tokenizer;
pub use tree::{Descendants, Dom, NodeRef};

/// Parse a full HTML document into a best-effort tree.
///
/// Never panics. Any recovery the parser performed is recorded in
/// [`Dom::errors`].
///
/// ```
/// let dom = domtree::parse("<p>hello<p>world");
/// assert_eq!(dom.find_by_tag("p").count(), 2); // implicit </p>
/// ```
pub fn parse(html: &str) -> Dom {
    builder::parse_document(html)
}

/// Parse an HTML fragment.
///
/// Uses the same engine as [`parse`]; the name simply documents intent (a
/// fragment commonly has several top-level nodes).
pub fn parse_fragment(html: &str) -> Dom {
    builder::parse_document(html)
}

/// Create a streaming [`Tokenizer`] over `html` without building a tree.
///
/// ```
/// use domtree::Token;
/// let kinds: usize = domtree::tokenize("<b>hi</b>").count();
/// assert_eq!(kinds, 3); // <b>, "hi", </b>
/// ```
pub fn tokenize(html: &str) -> Tokenizer<'_> {
    Tokenizer::new(html)
}
