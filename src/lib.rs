//! # domq
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
//! let dom = domq::parse(r#"<ul id="list"><li class="item">Item 1</li></ul>"#);
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
//! No unbounded search for the element an end tag closes: it must be among
//! the 256 innermost open elements or the 8 outermost (where `</body>` and
//! `</html>` find theirs). An end tag for an element open only in between is
//! ignored as stray ([`ErrorKind::StrayEndTag`]) and the element stays open.
//! Real documents almost never nest that deep, and the bound keeps every end
//! tag O(1) on hostile input.
//!
//! ## Getting content out
//!
//! - [`Dom::to_text`] — readable plain text: no `<script>`/`<style>`/`<head>`,
//!   whitespace collapsed, a line per block, a blank line between paragraphs.
//! - [`Dom::to_markdown`] — CommonMark + GFM (headings, lists, links, images,
//!   code blocks, tables, quotes): for LLM prompts, RAG indexes and notes.
//! - [`Dom::to_html`], [`NodeRef::inner_html`], [`NodeRef::outer_html`] — HTML.
//! - [`Dom::to_markdown_chunks`] — that Markdown split at headings into
//!   chunks under a size budget, each with its heading path: for RAG indexes.
//!
//! ```
//! let dom = domq::parse(
//!     "<article><h2>News</h2><p>It <em>works</em>!</p><script>track()</script></article>",
//! );
//! assert_eq!(dom.to_text(), "News\n\nIt works!");
//! # #[cfg(feature = "markdown")]
//! assert_eq!(dom.to_markdown(), "## News\n\nIt *works*!");
//! ```
//!
//! ## Editing and source positions
//!
//! A [`Dom`] doubles as a small HTML rewriter: [`Dom::remove`],
//! [`Dom::set_text`], [`Dom::set_attr`], [`Dom::rename`], [`Dom::wrap`],
//! [`Dom::unwrap`], parsed-fragment insertion ([`Dom::append_html`] and
//! friends) and node moves ([`Dom::append_child`], [`Dom::insert_before`]).
//! Parsed nodes also remember where they came from:
//! [`NodeRef::source_range`] plus [`line_col`].
//!
//! ```
//! let src = "<p>Hello <b>world</b></p>";
//! let mut dom = domq::parse(src);
//! let b = dom.find_by_tag("b").next().unwrap();
//! assert_eq!(b.source_range(), Some(9..21));
//! assert_eq!(domq::line_col(src, 9), (1, 10));
//!
//! let b = b.id();
//! dom.rename(b, "strong");
//! dom.insert_html_after(b, "!");
//! assert_eq!(dom.to_html(), "<p>Hello <strong>world</strong>!</p>");
//! ```
//!
//! ## Structure and data
//!
//! - [`Dom::main_content`] — the article / post / docs body without the
//!   navigation, sidebars, comments and footer ("reader mode").
//! - [`Dom::links`] / [`Dom::resources`] — every link (with its text) and
//!   every embedded URL, resolved against the page URL and `<base href>`
//!   ([`resolve_url`], [`resolve_srcset`]: RFC 3986, zero-dependency).
//! - [`Dom::metadata`] — title, description, canonical URL, language,
//!   OpenGraph / Twitter-card fields, JSON-LD blocks and feeds.
//! - [`NodeRef::table_rows`] — a `<table>` as rows of cell text with
//!   `colspan`/`rowspan` expanded; [`to_csv`] formats them.
//!
//! ```
//! let dom = domq::parse(r#"<html lang="en"><head><title>Release notes</title></head>
//!   <body><nav><a href="/">Home</a></nav>
//!   <article><h1>Version 2</h1>
//!     <p>This release adds link extraction, page metadata, tables as data and a
//!        reader mode, all without a single new dependency.</p>
//!     <p>See the <a href="changelog.html">full changelog</a> for details.</p>
//!   </article></body></html>"#);
//!
//! assert_eq!(dom.metadata().title.as_deref(), Some("Release notes"));
//! let article = dom.main_content().unwrap();
//! assert_eq!(article.tag_name(), Some("article"));
//! let links = dom.links(Some("https://example.com/news/"));
//! assert_eq!(links[1].url, "https://example.com/news/changelog.html");
//! assert_eq!(links[1].text, "full changelog");
//! ```
//!
//! ## Declarative extraction
//!
//! With the `derive` feature, `#[derive(FromHtml)]` reads a struct out of a
//! page: each field names a CSS selector and what to read from the element it
//! matches. [`Dom::extract`] reads one value, [`Dom::extract_all`] one per
//! match of the type's own selector, [`NodeRef::extract`] one from an element.
//!
//! ```
//! # #[cfg(feature = "derive")] {
//! use domq::FromHtml;
//!
//! #[derive(FromHtml, Debug, PartialEq)]
//! struct Product {
//!     #[html(select = "h1")]
//!     name: String,
//!     #[html(select = ".price", attr = "data-value")]
//!     price: f64,
//!     #[html(select = ".tags li")]
//!     tags: Vec<String>,
//!     #[html(select = ".discount")]
//!     discount: Option<String>,
//!     #[html(select = ".seller")]
//!     seller: Seller,
//!     #[html(select = ".review")]
//!     reviews: Vec<Review>,
//! }
//!
//! #[derive(FromHtml, Debug, PartialEq)]
//! struct Seller {
//!     #[html(select = ".name")]
//!     name: String,
//!     #[html(select = "a", attr = "href")]
//!     url: String,
//! }
//!
//! #[derive(FromHtml, Debug, PartialEq)]
//! struct Review {
//!     #[html(select = ".stars", attr = "data-value")]
//!     stars: u8,
//!     #[html(select = "p")]
//!     text: String,
//! }
//!
//! let dom = domq::parse(r#"
//!   <h1>Acme Anvil</h1>
//!   <span class="price" data-value="149.99">$149.99</span>
//!   <ul class="tags"><li>tools</li><li>heavy</li></ul>
//!   <div class="seller"><b class="name">Wile E.</b> <a href="/s/77">profile</a></div>
//!   <div class="review"><i class="stars" data-value="5"></i><p>Beep beep!</p></div>
//!   <div class="review"><i class="stars" data-value="1"></i><p>It fell on me.</p></div>
//! "#);
//! let product: Product = dom.extract().unwrap();
//! assert_eq!(product.name, "Acme Anvil");
//! assert_eq!(product.price, 149.99);
//! assert_eq!(product.tags, ["tools", "heavy"]);
//! assert_eq!(product.discount, None);
//! assert_eq!(product.seller.url, "/s/77");
//! assert_eq!(product.reviews[1], Review { stars: 1, text: "It fell on me.".into() });
//!
//! // Errors say which field, selector and value failed:
//! let err = domq::parse("<h1>Anvil</h1><span class=price data-value='$5'>")
//!     .extract::<Product>()
//!     .unwrap_err();
//! assert_eq!(
//!     err.to_string(),
//!     r#"Product.price (".price" @data-value): cannot parse "$5" as f64: invalid float literal"#,
//! );
//! # }
//! ```
//!
//! **Field keys** — every field has `#[html(...)]` (a field without one is a
//! compile error):
//!
//! | key | meaning |
//! |---|---|
//! | `select = "css"` | the elements to read; without it, the scope itself (the document, or the element of a nested struct) |
//! | *(no source key)* | the [readable text](Dom::to_text) — or, for a type that implements [`FromHtml`] itself (a nested struct), that type read from the match |
//! | `attr = "name"` | an attribute's value |
//! | `html` / `outer_html` | the inner / outer HTML |
//! | `markdown` | the Markdown (needs the `markdown` feature) |
//! | `with = "path::to::fn"` | convert the value (text by default) with `fn(&str) -> Result<T, E>` where `E: Display`, instead of [`FromHtmlValue`] |
//! | `default` | no match or no attribute gives `Default::default()` instead of an error |
//! | `skip` | read nothing: `Default::default()` |
//!
//! At most one source per field. `attr`, `html`, `outer_html` and `markdown`
//! convert the string with [`FromHtmlValue`] (implemented for `String`, the
//! number types, `bool` and `char`; numbers, `bool` and `char` ignore
//! surrounding whitespace, and `bool` is `true` or `false` in any ASCII case).
//! On the struct, `#[html(select = "css")]` sets [`FromHtml::SELECTOR`]:
//! where values of the type live, for [`Dom::extract_all`] (one per match)
//! and [`Dom::extract`] (the first match).
//!
//! **The field type decides how many matches are read:** `Option<T>` is the
//! first match or `None` (also `None` when the attribute is missing; a value
//! that fails to convert is still an error), `Vec<T>` is every match in
//! document order (it needs a `select`; an element without the attribute is
//! an error, so select `a[href]` to skip them), and any other type is the
//! first match, a [`NoMatch`](ExtractErrorKind::NoMatch) error without one.
//! `Option` and `Vec` are recognized by name, so a type alias for one is read
//! as a single value.
//!
//! **Errors** ([`ExtractError`]) carry the field path (`Product.seller.name`,
//! `Product.reviews[1].stars`, `Product[1].title` for [`Dom::extract_all`]
//! items), the selector, the source and the reason; fields are read in
//! declaration order and the first failure is returned.
//!
//! **Notes.** Selectors are parsed at run time, once per field each time a
//! value is read (so a `Vec<Review>` field parses `Review`'s selectors once per
//! review); an invalid one is an [`InvalidSelector`](ExtractErrorKind::InvalidSelector)
//! error naming the field. That is cheap next to matching, but in a hot loop
//! parse each page once and read everything you need from that one [`Dom`].
//! Generic structs work: the macro adds `T: FromHtml` (or [`FromHtmlValue`],
//! or `Default`) bounds for the fields that need them. The generated code
//! names only `::domq::__private` re-exports, never `std`, so it works in
//! `no_std` crates — but it does need this crate to be reachable as
//! `domq` (the default; a renamed dependency is not supported). Implement
//! [`FromHtml`] by hand for anything the attributes can't express; a
//! hand-written type nests in derived ones like any other.
//!
//! ## Features
//!
//! - `json` *(default)* — the zero-dependency [`Dom::to_json`] serializer.
//! - `markdown` *(default)* — the zero-dependency [`Dom::to_markdown`] and
//!   [`Dom::to_markdown_chunks`].
//! - `serde` — optional `serde::Serialize` for the tree, [`Metadata`] and
//!   [`Chunk`] (adds the `serde` dep).
//! - `query` *(default)* — a zero-dependency CSS selector engine
//!   ([`Dom::select`], [`Selector`]), and the extraction traits built on it
//!   ([`FromHtml`], [`Dom::extract`]).
//! - `derive` — `#[derive(FromHtml)]` (implies `query`): declarative
//!   extraction into structs. Re-exports the proc-macro from the companion
//!   `domq-derive` crate, which uses `syn`, `quote` and `proc-macro2` at
//!   compile time only; nothing is added to the compiled program.
//! - `entities-full` — decode all 2,125 WHATWG named character references
//!   instead of the curated ~180 (a sorted static table, about 70 KB).
//! - `simd` — scan text and attribute values with
//!   [`memchr`](https://crates.io/crates/memchr) (SIMD, `no_std`) instead of
//!   the built-in word-at-a-time search. Adds that one dependency; tokens and
//!   trees are identical either way. Worth it only for very text-heavy input.
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
mod edit;
mod entities;
#[cfg(feature = "entities-full")]
mod entities_full;
mod error;
mod events;
mod extract;
mod node;
mod scan;
mod source;
mod tags;
mod token;
mod tokenizer;
mod tree;
mod url;

mod serialize;

#[cfg(feature = "query")]
mod query;

#[cfg(feature = "query")]
mod from_html;

pub use error::{ErrorKind, ParseError};
pub use events::{Event, Events};
pub use extract::{Link, Metadata, Resource, to_csv};
pub use node::{NodeId, NodeKind};
pub use source::line_col;
pub use token::{AttrValue, Token};
pub use tokenizer::Tokenizer;
pub use tree::{Descendants, Dom, NodeRef};
pub use url::{resolve_srcset, resolve_url};

#[cfg(feature = "markdown")]
pub use serialize::Chunk;

#[cfg(feature = "query")]
pub use query::{AsSelector, Selector, SelectorError};

#[cfg(feature = "query")]
pub use from_html::{ExtractError, ExtractErrorKind, FromHtml, FromHtmlValue, Scope, ValueSource};

/// `#[derive(FromHtml)]`: declarative extraction into structs (the `derive`
/// feature); see [Declarative extraction](crate#declarative-extraction).
#[cfg(feature = "derive")]
pub use domq_derive::FromHtml;

/// What `#[derive(FromHtml)]` expands to. Not public API: no semver
/// guarantees.
#[cfg(feature = "query")]
#[doc(hidden)]
pub mod __private {
    pub use alloc::string::String;
    pub use alloc::vec::Vec;
    pub use core::default::Default;
    pub use core::option::Option::{self, None, Some};
    pub use core::result::Result::{self, Err, Ok};

    pub use crate::from_html::{Ctx, Src, all, nested, one, opt, value, with};
    pub use crate::{ExtractError, FromHtml, FromHtmlValue, Scope};

    pub use crate::__domq_require_markdown as require_markdown;
}

/// Stops the build of a `#[html(markdown)]` field when the `markdown`
/// feature is off.
#[cfg(not(feature = "markdown"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __domq_require_markdown {
    () => {
        ::core::compile_error!(
            "`#[html(markdown)]` needs the `markdown` feature of domq (on by default)"
        );
    };
}

/// Stops the build of a `#[html(markdown)]` field when the `markdown`
/// feature is off.
#[cfg(feature = "markdown")]
#[doc(hidden)]
#[macro_export]
macro_rules! __domq_require_markdown {
    () => {};
}

/// Parse a full HTML document into a best-effort tree.
///
/// Never panics. Any recovery the parser performed is recorded in
/// [`Dom::errors`].
///
/// ```
/// let dom = domq::parse("<p>hello<p>world");
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
/// use domq::Token;
/// let kinds: usize = domq::tokenize("<b>hi</b>").count();
/// assert_eq!(kinds, 3); // <b>, "hi", </b>
/// ```
pub fn tokenize(html: &str) -> Tokenizer<'_> {
    Tokenizer::new(html)
}

/// Stream a document as [`Event`]s — start and end of elements, text,
/// comments, doctypes — without building a tree.
///
/// Unlike [`tokenize`], the events come **after recovery**, from the same
/// tree builder [`parse`] uses: omitted end tags are implied (`<li>a<li>b`
/// ends the first `<li>`), void and self-closing elements end right away,
/// stray end tags are dropped, mismatched ones close what they skip, and
/// everything still open ends at the end of the input. So every `Start` has
/// exactly one matching `End`, and replaying the events builds exactly the
/// tree `parse` returns. Text and attribute values are entity-decoded, and
/// each event comes with its byte range in `html` (see [`Events`]).
///
/// Memory use is proportional to the nesting depth, not the document size —
/// except for the recovery diagnostics, which are kept for
/// [`Events::errors`] (16 bytes each; messy markup has about one per
/// implicitly closed element). It is not faster than [`parse`]: each start
/// tag with attributes gets its own `Vec`, where the tree shares one arena.
///
/// ```
/// use domq::Event;
///
/// let html = "<ul><li>Tom &amp; Jerry<li>Spike<br></ul>";
/// let mut depth = 0;
/// let mut items = Vec::new();
/// for (event, range) in domq::events(html) {
///     match event {
///         Event::Start { name, .. } => {
///             depth += 1;
///             if name == "li" {
///                 items.push(String::new());
///             }
///         }
///         Event::End { name, implied } => {
///             depth -= 1;
///             if name == "li" {
///                 // Both `</li>` were omitted: the first is closed by the
///                 // second `<li>`, the second by `</ul>`.
///                 assert!(implied);
///                 assert!(range.is_empty());
///             }
///         }
///         Event::Text(text) => {
///             if let Some(item) = items.last_mut() {
///                 item.push_str(&text);
///             }
///         }
///         _ => {}
///     }
/// }
/// assert_eq!(depth, 0);
/// assert_eq!(items, ["Tom & Jerry", "Spike"]);
/// ```
pub fn events(html: &str) -> Events<'_> {
    Events::new(html)
}
