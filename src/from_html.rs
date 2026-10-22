//! Declarative extraction: read a Rust value out of a document or an element
//! ([`FromHtml`], [`Dom::extract`], [`Dom::extract_all`]).
//!
//! The traits and the runtime they need live here, always compiled with the
//! `query` feature, so hand-written impls work without the `derive` feature;
//! `#[derive(FromHtml)]` (the `domq-derive` crate) only generates calls
//! into [`__private`](crate::__private).

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use crate::node::NodeKind;
use crate::query::{Selector, SelectorError};
use crate::tree::{Dom, NodeRef};

// --- scope ------------------------------------------------------------------

/// Where a [`FromHtml`] value is read from: a whole document or one element.
///
/// [`Dom::extract`] starts at [`Scope::Document`] (or at the first match of
/// the type's [`FromHtml::SELECTOR`]); a nested field starts at the element
/// its `select` matched. The helper methods are what generated code (and
/// hand-written impls) read the scope with.
///
/// ```
/// use domq::{Scope, Selector};
///
/// let dom = domq::parse("<div class=card data-id=7><h2>Hi</h2></div>");
/// let doc = Scope::Document(&dom);
/// assert!(doc.node().is_none());
/// assert_eq!(doc.attr("data-id"), None); // a document has no attributes
///
/// let h2 = Selector::parse("h2").unwrap();
/// let card = Scope::Element(dom.select_first(".card").unwrap());
/// assert_eq!(card.attr("data-id"), Some("7"));
/// assert_eq!(card.select_first(&h2).unwrap().to_text(), "Hi");
/// assert_eq!(card.inner_html(), "<h2>Hi</h2>");
/// ```
#[derive(Clone, Copy, Debug)]
pub enum Scope<'a> {
    /// The whole document.
    Document(&'a Dom),
    /// One element (or other node) and its subtree.
    Element(NodeRef<'a>),
}

impl<'a> From<&'a Dom> for Scope<'a> {
    fn from(dom: &'a Dom) -> Self {
        Scope::Document(dom)
    }
}

impl<'a> From<NodeRef<'a>> for Scope<'a> {
    fn from(node: NodeRef<'a>) -> Self {
        Scope::Element(node)
    }
}

impl<'a> Scope<'a> {
    /// The element of an [`Element`](Scope::Element) scope; `None` for a
    /// document.
    pub fn node(&self) -> Option<NodeRef<'a>> {
        match *self {
            Scope::Document(_) => None,
            Scope::Element(node) => Some(node),
        }
    }

    /// The first element matching `selector`, in document order: anywhere in
    /// the document, or among the element's descendants (not the element
    /// itself — see [`NodeRef::select`]).
    pub fn select_first(&self, selector: &Selector) -> Option<NodeRef<'a>> {
        match *self {
            Scope::Document(dom) => dom.select_first(selector),
            Scope::Element(node) => node.select_first(selector),
        }
    }

    /// Every element matching `selector`, in document order (see
    /// [`Scope::select_first`]).
    pub fn select<'s>(
        &self,
        selector: &'s Selector,
    ) -> impl Iterator<Item = NodeRef<'a>> + use<'a, 's> {
        let (doc, element) = match *self {
            Scope::Document(dom) => (Some(dom.select(selector)), None),
            Scope::Element(node) => (None, Some(node.select(selector))),
        };
        doc.into_iter()
            .flatten()
            .chain(element.into_iter().flatten())
    }

    /// The readable text ([`Dom::to_text`] / [`NodeRef::to_text`]): what
    /// leaf types such as `String` and `f64` are read from. An element that
    /// is hidden in a page's text, like `<title>` or a JSON-LD `<script>`,
    /// still reads as its own text here.
    pub fn text(&self) -> String {
        match *self {
            Scope::Document(dom) => dom.to_text(),
            Scope::Element(node) => node.to_text(),
        }
    }

    /// The HTML inside the element ([`NodeRef::inner_html`]), or the whole
    /// document's HTML.
    pub fn inner_html(&self) -> String {
        match *self {
            Scope::Document(dom) => dom.to_html(),
            Scope::Element(node) => node.inner_html(),
        }
    }

    /// The element's own HTML ([`NodeRef::outer_html`]), or the whole
    /// document's HTML.
    pub fn outer_html(&self) -> String {
        match *self {
            Scope::Document(dom) => dom.to_html(),
            Scope::Element(node) => node.outer_html(),
        }
    }

    /// The subtree (or document) as Markdown ([`Dom::to_markdown`]).
    #[cfg(feature = "markdown")]
    pub fn markdown(&self) -> String {
        match *self {
            Scope::Document(dom) => dom.to_markdown(),
            Scope::Element(node) => node.to_markdown(),
        }
    }

    /// The value of the element's attribute `name` (as written; HTML
    /// attribute names are lower-cased by the parser). Always `None` for a
    /// document, which has no attributes.
    pub fn attr(&self, name: &str) -> Option<&'a str> {
        match *self {
            Scope::Document(_) => None,
            Scope::Element(node) => node.attr(name),
        }
    }
}

// --- traits -------------------------------------------------------------------

/// A type that can be read out of HTML: a whole document or one element.
///
/// Usually derived — `#[derive(FromHtml)]` with the `derive` feature, where
/// each field says what to select and what to read (see the
/// [crate docs](crate#declarative-extraction)). Implemented here for `String`,
/// every integer and float type, `bool` and `char`, which read the
/// [readable text](Scope::text) of the scope and convert it with
/// [`FromHtmlValue`].
///
/// A hand-written impl reads the [`Scope`] directly:
///
/// ```
/// use domq::{ExtractError, ExtractErrorKind, FromHtml, Scope};
///
/// struct Heading {
///     level: u8,
///     text: String,
/// }
///
/// impl FromHtml for Heading {
///     const SELECTOR: Option<&'static str> = Some("h1, h2, h3");
///
///     fn from_html(scope: Scope<'_>) -> Result<Self, ExtractError> {
///         let node = scope.node().ok_or_else(|| ExtractError::new(ExtractErrorKind::NoMatch))?;
///         let level = node.tag_name().and_then(|t| t.get(1..)).and_then(|n| n.parse().ok());
///         Ok(Heading { level: level.unwrap_or(0), text: scope.text() })
///     }
/// }
///
/// let dom = domq::parse("<h1>Intro</h1><p>…</p><h2>Usage</h2>");
/// let toc: Vec<Heading> = dom.extract_all().unwrap();
/// assert_eq!(toc[1].level, 2);
/// assert_eq!(toc[1].text, "Usage");
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be extracted from HTML",
    label = "`{Self}` does not implement `FromHtml`",
    note = "derive it with `#[derive(FromHtml)]`, or read a string value with `attr`, `html`, `outer_html` or `markdown` (which need `FromHtmlValue`) or convert one with `with = \"…\"`"
)]
pub trait FromHtml: Sized {
    /// Where values of this type live in a document: the struct-level
    /// `#[html(select = "…")]`. [`Dom::extract_all`] reads one value per
    /// match and [`Dom::extract`] reads the first match; `None` (the
    /// default) means "the whole document".
    const SELECTOR: Option<&'static str> = None;

    /// Read a value from `scope`.
    fn from_html(scope: Scope<'_>) -> Result<Self, ExtractError>;
}

/// A type that can be parsed from one string value: an attribute value, an
/// element's text, its HTML or its Markdown.
///
/// Implemented for `String` (the value as is), every integer and float type
/// and `char` (the value with surrounding whitespace trimmed, through
/// [`FromStr`](core::str::FromStr)), and `bool` (`true` or `false` in any
/// ASCII case, trimmed). Implement it for your own types to use them with
/// `attr`/`html`/`outer_html`/`markdown` fields; the error is a short message
/// that [`ExtractError`] reports with the value and the type name.
///
/// ```
/// use domq::FromHtmlValue;
///
/// assert_eq!(f64::from_html_value(" 4.5 "), Ok(4.5));
/// assert_eq!(bool::from_html_value("TRUE"), Ok(true));
/// assert_eq!(u8::from_html_value("300").unwrap_err(), "number too large to fit in target type");
/// assert_eq!(String::from_html_value(" as is "), Ok(" as is ".to_string()));
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be parsed from an HTML string value",
    label = "`{Self}` does not implement `FromHtmlValue`",
    note = "implement `FromHtmlValue` for it, or convert the value with `with = \"path::to::fn\"`"
)]
pub trait FromHtmlValue: Sized {
    /// Parse `value`, or say briefly why it is not one (`"invalid digit
    /// found in string"`).
    fn from_html_value(value: &str) -> Result<Self, String>;
}

impl FromHtmlValue for String {
    fn from_html_value(value: &str) -> Result<Self, String> {
        Ok(value.into())
    }
}

impl FromHtml for String {
    fn from_html(scope: Scope<'_>) -> Result<Self, ExtractError> {
        Ok(scope.text())
    }
}

impl FromHtmlValue for bool {
    fn from_html_value(value: &str) -> Result<Self, String> {
        let value = value.trim();
        if value.eq_ignore_ascii_case("true") {
            Ok(true)
        } else if value.eq_ignore_ascii_case("false") {
            Ok(false)
        } else {
            Err(r#"expected "true" or "false""#.into())
        }
    }
}

/// `FromHtmlValue` through `FromStr` on the trimmed value, and `FromHtml`
/// through the readable text.
macro_rules! from_str_impls {
    ($($t:ty),* $(,)?) => {$(
        impl FromHtmlValue for $t {
            fn from_html_value(value: &str) -> Result<Self, String> {
                value.trim().parse().map_err(|e: <$t as core::str::FromStr>::Err| e.to_string())
            }
        }
    )*};
}

from_str_impls!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64, char
);

/// `FromHtml` for leaf types: the scope's readable text through
/// `FromHtmlValue`.
macro_rules! text_impls {
    ($($t:ty),* $(,)?) => {$(
        impl FromHtml for $t {
            fn from_html(scope: Scope<'_>) -> Result<Self, ExtractError> {
                parse_value(Cow::Owned(scope.text()), ValueSource::Text)
            }
        }
    )*};
}

text_impls!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64, bool, char
);

/// Convert `value` with `T::from_html_value`, as a `Parse` error on failure.
fn parse_value<T: FromHtmlValue>(
    value: Cow<'_, str>,
    source: ValueSource,
) -> Result<T, ExtractError> {
    T::from_html_value(&value).map_err(|message| {
        ExtractError::new(ExtractErrorKind::Parse {
            value: value.trim().into(),
            type_name: short_type_name::<T>(),
            message,
        })
        .with_source(source)
    })
}

/// `core::any::type_name` without module paths: `alloc::string::String` →
/// `String`, `Vec<my::Item>` → `Vec<Item>`.
fn short_type_name<T>() -> String {
    let full = core::any::type_name::<T>();
    let mut out = String::with_capacity(full.len());
    let mut chars = full.chars().peekable();
    while let Some(c) = chars.next() {
        if c == ':' && chars.peek() == Some(&':') {
            chars.next();
            // Drop the path segment just written.
            let keep = out
                .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
                .map_or(0, |i| i + 1);
            out.truncate(keep);
        } else {
            out.push(c);
        }
    }
    out
}

// --- Dom / NodeRef entry points ------------------------------------------------

impl Dom {
    /// Read a `T` out of this document.
    ///
    /// If `T` has a [`SELECTOR`](FromHtml::SELECTOR) (a struct-level
    /// `#[html(select = "…")]`), it is read from the first element matching
    /// it (a [`NoMatch`](ExtractErrorKind::NoMatch) error if none does);
    /// otherwise from the whole document.
    ///
    /// ```
    /// # #[cfg(feature = "derive")] {
    /// use domq::FromHtml;
    ///
    /// #[derive(FromHtml)]
    /// struct Page {
    ///     #[html(select = "h1")]
    ///     title: String,
    ///     #[html(select = "a.next", attr = "href")]
    ///     next: Option<String>,
    /// }
    ///
    /// let dom = domq::parse("<h1>Chapter 1</h1><a class=next href=/2>Next</a>");
    /// let page: Page = dom.extract().unwrap();
    /// assert_eq!(page.title, "Chapter 1");
    /// assert_eq!(page.next.as_deref(), Some("/2"));
    /// # }
    /// // Leaf types read the readable text:
    /// let dom = domq::parse("<p> 42 </p>");
    /// assert_eq!(dom.extract::<u32>(), Ok(42));
    /// ```
    pub fn extract<T: FromHtml>(&self) -> Result<T, ExtractError> {
        match T::SELECTOR {
            None => T::from_html(Scope::Document(self)),
            Some(css) => {
                let sel = type_selector::<T>(css)?;
                let node = self.select_first(&sel).ok_or_else(|| {
                    ExtractError::new(ExtractErrorKind::NoMatch)
                        .with_selector(css)
                        .in_type(short_type_name::<T>())
                })?;
                T::from_html(Scope::Element(node))
            }
        }
    }

    /// Read one `T` per element matching its [`SELECTOR`](FromHtml::SELECTOR)
    /// (the struct-level `#[html(select = "…")]`), in document order. No
    /// match is an empty `Vec`; a type without a selector is a
    /// [`MissingSelector`](ExtractErrorKind::MissingSelector) error. The
    /// first item that fails stops the extraction, and its error path names
    /// the item: `Card[2].price`.
    ///
    /// ```
    /// # #[cfg(feature = "derive")] {
    /// use domq::FromHtml;
    ///
    /// #[derive(FromHtml, Debug)]
    /// #[html(select = ".card")]
    /// struct Card {
    ///     #[html(select = "h2")]
    ///     title: String,
    ///     #[html(select = ".price", attr = "data-cents")]
    ///     cents: u32,
    /// }
    ///
    /// let dom = domq::parse(
    ///     "<div class=card><h2>Pen</h2><b class=price data-cents=150>$1.50</b></div>\
    ///      <div class=card><h2>Ink</h2><b class=price data-cents=180>$1.80</b></div>",
    /// );
    /// let cards: Vec<Card> = dom.extract_all().unwrap();
    /// assert_eq!((cards[1].title.as_str(), cards[1].cents), ("Ink", 180));
    ///
    /// let dom = domq::parse(
    ///     "<div class=card><h2>Pen</h2><b class=price data-cents=150>$1.50</b></div>\
    ///      <div class=card><h2>Ink</h2><b class=price data-cents=x>?</b></div>",
    /// );
    /// let err = dom.extract_all::<Card>().unwrap_err();
    /// assert_eq!(
    ///     err.to_string(),
    ///     r#"Card[1].cents (".price" @data-cents): cannot parse "x" as u32: invalid digit found in string"#,
    /// );
    /// # }
    /// ```
    pub fn extract_all<T: FromHtml>(&self) -> Result<Vec<T>, ExtractError> {
        let Some(css) = T::SELECTOR else {
            return Err(ExtractError::new(ExtractErrorKind::MissingSelector)
                .in_type(short_type_name::<T>()));
        };
        let sel = type_selector::<T>(css)?;
        self.select(&sel)
            .enumerate()
            .map(|(i, node)| T::from_html(Scope::Element(node)).map_err(|e| e.at_index(i)))
            .collect()
    }
}

/// Parse a type's `SELECTOR`, naming the type on failure.
fn type_selector<T>(css: &str) -> Result<Selector, ExtractError> {
    Selector::parse(css).map_err(|e| {
        ExtractError::new(ExtractErrorKind::InvalidSelector(e))
            .with_selector(css)
            .in_type(short_type_name::<T>())
    })
}

impl NodeRef<'_> {
    /// Read a `T` out of this element and its subtree. The type's
    /// [`SELECTOR`](FromHtml::SELECTOR) is not applied: this node is the
    /// value.
    ///
    /// ```
    /// let dom = domq::parse("<ul><li>3</li><li>4</li></ul>");
    /// let li = dom.select_first("li:last-child").unwrap();
    /// assert_eq!(li.extract::<i64>(), Ok(4));
    /// ```
    pub fn extract<T: FromHtml>(&self) -> Result<T, ExtractError> {
        T::from_html(Scope::Element(*self))
    }
}

// --- errors -------------------------------------------------------------------

/// Why extraction failed, where, and what it was reading.
///
/// Its [`Display`](fmt::Display) gives the field path, the selector and the
/// source, then the reason:
///
/// ```text
/// Product.title ("h1"): no element matches
/// Product.price (".price" @data-value): cannot parse "$5" as f64: invalid float literal
/// Product.price (".price" @data-value): <span> has no "data-value" attribute
/// Product.sku (@data-sku): a document has no attributes; give the field a selector or extract from an element
/// Cents.cents (".price"): cannot convert "$x" with parse_cents: no digits
/// Typo.title ("h1:hover"): invalid selector: unsupported pseudo-class ':hover' at byte 2
/// Card: extract_all needs a selector on the type: #[html(select = "…")]
/// ```
///
/// The path names the type and field. Nested structs prefix the outer field
/// (`Product.seller.name`), `Vec` items add their index
/// (`Product.reviews[1].rating`) and so do [`Dom::extract_all`] items
/// (`Product[1].title`). The parentheses hold the selector and the source
/// (`@attr`, `html`, `outer_html`, `markdown`); a text or nested field shows
/// only its selector, and one with neither has no parentheses. Values are
/// quoted and escaped like `{:?}`, cut short after 60 characters.
///
/// It is one pointer wide (the details are boxed), so `Result<T,
/// ExtractError>` costs little on the success path.
#[derive(Clone, PartialEq, Eq)]
pub struct ExtractError(Box<Inner>);

#[derive(Clone, PartialEq, Eq)]
struct Inner {
    kind: ExtractErrorKind,
    /// The type the path starts at (`Product`).
    root: Option<Cow<'static, str>>,
    /// Fields and indices, innermost first.
    segments: Vec<Segment>,
    selector: Option<String>,
    source: Option<ValueSource>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Segment {
    Field(&'static str),
    Index(usize),
}

/// What went wrong; see [`ExtractError::kind`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExtractErrorKind {
    /// A required field's selector matched nothing.
    NoMatch,
    /// The element has no such attribute, or the field read a document,
    /// which has none.
    MissingAttribute {
        /// The attribute's name.
        name: String,
        /// The tag name of the element that lacks it (`span`; `#text` and
        /// the like for other nodes), or `None` for a document.
        element: Option<String>,
    },
    /// The value was read but [`FromHtmlValue`] could not convert it.
    Parse {
        /// The value, without surrounding whitespace (an attribute value,
        /// the readable text, HTML, …).
        value: String,
        /// The target type, without module paths (`f64`, `Price`).
        type_name: String,
        /// The [`FromHtmlValue`] error (`invalid float literal`).
        message: String,
    },
    /// A `with = "…"` function rejected the value.
    Convert {
        /// The value the function was given.
        value: String,
        /// The function's name: the last segment of its path
        /// (`parse_cents`).
        function: String,
        /// The function's error, displayed.
        message: String,
    },
    /// A selector failed to parse.
    InvalidSelector(SelectorError),
    /// [`Dom::extract_all`] on a type without a struct-level
    /// `#[html(select = "…")]`.
    MissingSelector,
}

/// Which value of the selected element a field read.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ValueSource {
    /// The readable text (the default).
    Text,
    /// An attribute (`attr = "…"`).
    Attr(String),
    /// The inner HTML (`html`).
    InnerHtml,
    /// The outer HTML (`outer_html`).
    OuterHtml,
    /// The Markdown (`markdown`).
    Markdown,
}

impl ExtractError {
    /// A new error of `kind`, without a path, selector or source yet.
    pub fn new(kind: ExtractErrorKind) -> Self {
        ExtractError(Box::new(Inner {
            kind,
            root: None,
            segments: Vec::new(),
            selector: None,
            source: None,
        }))
    }

    /// What went wrong.
    pub fn kind(&self) -> &ExtractErrorKind {
        &self.0.kind
    }

    /// Where: the type and field path, like `Product.reviews[2].rating`
    /// (empty for a value read directly, like `dom.extract::<f64>()`).
    ///
    /// ```
    /// use domq::{ExtractError, ExtractErrorKind};
    ///
    /// let err = ExtractError::new(ExtractErrorKind::NoMatch)
    ///     .in_field("Review", "rating")
    ///     .at_index(2)
    ///     .in_field("Product", "reviews");
    /// assert_eq!(err.path(), "Product.reviews[2].rating");
    /// ```
    pub fn path(&self) -> String {
        use core::fmt::Write;
        let mut out = String::new();
        if let Some(root) = &self.0.root {
            out.push_str(root);
        }
        for segment in self.0.segments.iter().rev() {
            match segment {
                Segment::Field(name) => {
                    if !out.is_empty() {
                        out.push('.');
                    }
                    out.push_str(name);
                }
                Segment::Index(i) => {
                    let _ = write!(out, "[{i}]");
                }
            }
        }
        out
    }

    /// The selector being matched when it failed (the innermost one).
    pub fn selector(&self) -> Option<&str> {
        self.0.selector.as_deref()
    }

    /// The value being read when it failed, if it got that far.
    pub fn value_source(&self) -> Option<&ValueSource> {
        self.0.source.as_ref()
    }

    /// Record the selector being matched, unless the error already names one
    /// (the innermost selector is the most specific).
    pub fn with_selector(mut self, selector: &str) -> Self {
        if self.0.selector.is_none() {
            self.0.selector = Some(selector.into());
        }
        self
    }

    /// Record the value being read, unless the error already names one.
    pub fn with_source(mut self, source: ValueSource) -> Self {
        if self.0.source.is_none() {
            self.0.source = Some(source);
        }
        self
    }

    /// Put the error inside field `field` of type `type_name`: the path
    /// `Seller.name` becomes `Product.seller.name` after
    /// `.in_field("Product", "seller")`.
    pub fn in_field(mut self, type_name: &'static str, field: &'static str) -> Self {
        self.0.segments.push(Segment::Field(field));
        self.0.root = Some(Cow::Borrowed(type_name));
        self
    }

    /// Put the error inside item `index` of a list: `Review.rating` becomes
    /// `Review[2].rating`, and after [`in_field`](Self::in_field)
    /// `Product.reviews[2].rating`.
    pub fn at_index(mut self, index: usize) -> Self {
        self.0.segments.push(Segment::Index(index));
        self
    }

    /// Name the type an error without a path happened in.
    fn in_type(mut self, type_name: String) -> Self {
        if self.0.root.is_none() {
            self.0.root = Some(Cow::Owned(type_name));
        }
        self
    }
}

/// Values longer than this many characters are cut short in messages.
const MAX_VALUE_CHARS: usize = 60;

impl fmt::Display for ExtractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = self.path();
        f.write_str(&path)?;
        let source = match &self.0.source {
            None | Some(ValueSource::Text) => None,
            Some(source) => Some(source),
        };
        let mut lead = !path.is_empty();
        if self.0.selector.is_some() || source.is_some() {
            f.write_str(if lead { " (" } else { "(" })?;
            if let Some(selector) = &self.0.selector {
                write!(f, "{selector:?}")?;
                if source.is_some() {
                    f.write_str(" ")?;
                }
            }
            match source {
                Some(ValueSource::Attr(name)) => write!(f, "@{name}")?,
                Some(ValueSource::InnerHtml) => f.write_str("html")?,
                Some(ValueSource::OuterHtml) => f.write_str("outer_html")?,
                Some(ValueSource::Markdown) => f.write_str("markdown")?,
                Some(ValueSource::Text) | None => {}
            }
            f.write_str(")")?;
            lead = true;
        }
        if lead {
            f.write_str(": ")?;
        }
        match &self.0.kind {
            ExtractErrorKind::NoMatch => f.write_str("no element matches"),
            ExtractErrorKind::MissingAttribute {
                name,
                element: Some(element),
            } => write!(f, "<{element}> has no {name:?} attribute"),
            ExtractErrorKind::MissingAttribute { element: None, .. } => f.write_str(
                "a document has no attributes; give the field a selector or extract from an \
                 element",
            ),
            ExtractErrorKind::Parse {
                value,
                type_name,
                message,
            } => {
                f.write_str("cannot parse ")?;
                write_value(f, value)?;
                write!(f, " as {type_name}")?;
                write_message(f, message)
            }
            ExtractErrorKind::Convert {
                value,
                function,
                message,
            } => {
                f.write_str("cannot convert ")?;
                write_value(f, value)?;
                write!(f, " with {function}")?;
                write_message(f, message)
            }
            ExtractErrorKind::InvalidSelector(e) => write!(f, "invalid selector: {e}"),
            ExtractErrorKind::MissingSelector => {
                f.write_str(r#"extract_all needs a selector on the type: #[html(select = "…")]"#)
            }
        }
    }
}

/// A value in quotes (escaped like `{:?}`), cut short if long.
fn write_value(f: &mut fmt::Formatter<'_>, value: &str) -> fmt::Result {
    match value.char_indices().nth(MAX_VALUE_CHARS) {
        Some((cut, _)) => write!(f, "{:?}…", value.get(..cut).unwrap_or(value)),
        None => write!(f, "{value:?}"),
    }
}

/// `: message`, unless there is none.
fn write_message(f: &mut fmt::Formatter<'_>, message: &str) -> fmt::Result {
    if message.is_empty() {
        Ok(())
    } else {
        write!(f, ": {message}")
    }
}

impl fmt::Debug for ExtractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExtractError")
            .field("kind", &self.0.kind)
            .field("path", &self.path())
            .field("selector", &self.0.selector)
            .field("source", &self.0.source)
            .finish()
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ExtractError {}

// --- runtime for the derive macro ---------------------------------------------

/// Which value a field reads (the generated code's view of [`ValueSource`]).
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub enum Src {
    Text,
    Attr(&'static str),
    InnerHtml,
    OuterHtml,
    Markdown,
}

impl Src {
    fn value_source(self) -> ValueSource {
        match self {
            Src::Text => ValueSource::Text,
            Src::Attr(name) => ValueSource::Attr(name.into()),
            Src::InnerHtml => ValueSource::InnerHtml,
            Src::OuterHtml => ValueSource::OuterHtml,
            Src::Markdown => ValueSource::Markdown,
        }
    }

    /// Read this value from `scope`; `None` for a missing attribute.
    fn read<'a>(self, scope: Scope<'a>) -> Option<Cow<'a, str>> {
        Some(match self {
            Src::Text => Cow::Owned(scope.text()),
            Src::Attr(name) => Cow::Borrowed(scope.attr(name)?),
            Src::InnerHtml => Cow::Owned(scope.inner_html()),
            Src::OuterHtml => Cow::Owned(scope.outer_html()),
            #[cfg(feature = "markdown")]
            Src::Markdown => Cow::Owned(scope.markdown()),
            // Unreachable in compiling code: `require_markdown!` stops the
            // build first.
            #[cfg(not(feature = "markdown"))]
            Src::Markdown => Cow::Borrowed(""),
        })
    }
}

/// One field of a derived type: where it is (for error paths), what it
/// selects and what it reads (`None`: nested `FromHtml`).
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct Ctx {
    pub ty: &'static str,
    pub field: &'static str,
    pub select: Option<&'static str>,
    pub source: Option<Src>,
}

impl Ctx {
    /// Add this field's selector, source and path to an error from below.
    fn wrap(&self, mut e: ExtractError) -> ExtractError {
        if let Some(css) = self.select {
            e = e.with_selector(css);
        }
        if let Some(src) = self.source {
            e = e.with_source(src.value_source());
        }
        e.in_field(self.ty, self.field)
    }

    /// An error of `kind` about this field's selector itself.
    fn selector_error(&self, kind: ExtractErrorKind) -> ExtractError {
        let e = ExtractError::new(kind);
        let e = match self.select {
            Some(css) => e.with_selector(css),
            None => e,
        };
        e.in_field(self.ty, self.field)
    }

    /// The attribute this field reads is missing on `target` (not wrapped
    /// yet).
    fn missing_attribute(&self, target: Scope<'_>) -> ExtractError {
        let name = match self.source {
            Some(Src::Attr(name)) => name.into(),
            _ => String::new(),
        };
        let element = target.node().map(|node| match node.kind() {
            NodeKind::Element { tag } => tag.into(),
            NodeKind::Text(_) => "#text".into(),
            NodeKind::Comment(_) => "#comment".into(),
            NodeKind::Doctype(_) => "#doctype".into(),
        });
        ExtractError::new(ExtractErrorKind::MissingAttribute { name, element })
    }

    fn parse_selector(&self, css: &str) -> Result<Selector, ExtractError> {
        Selector::parse(css).map_err(|e| self.selector_error(ExtractErrorKind::InvalidSelector(e)))
    }

    /// The first match of the selector, or the scope itself without one.
    fn target<'a>(&self, scope: Scope<'a>) -> Result<Option<Scope<'a>>, ExtractError> {
        match self.select {
            None => Ok(Some(scope)),
            Some(css) => {
                let sel = self.parse_selector(css)?;
                Ok(scope.select_first(&sel).map(Scope::Element))
            }
        }
    }
}

/// The converter result: `Ok(None)` is a missing attribute.
type Converted<T> = Result<Option<T>, ExtractError>;

/// An exactly-one field: the first match, `NoMatch` without one.
#[doc(hidden)]
pub fn one<'a, T>(
    scope: Scope<'a>,
    ctx: &Ctx,
    convert: impl FnOnce(Scope<'a>, &Ctx) -> Converted<T>,
) -> Result<T, ExtractError> {
    let target = ctx
        .target(scope)?
        .ok_or_else(|| ctx.selector_error(ExtractErrorKind::NoMatch))?;
    convert(target, ctx)
        .map_err(|e| ctx.wrap(e))?
        .ok_or_else(|| ctx.wrap(ctx.missing_attribute(target)))
}

/// An `Option` (or `default`) field: `None` without a match or attribute.
#[doc(hidden)]
pub fn opt<'a, T>(
    scope: Scope<'a>,
    ctx: &Ctx,
    convert: impl FnOnce(Scope<'a>, &Ctx) -> Converted<T>,
) -> Result<Option<T>, ExtractError> {
    match ctx.target(scope)? {
        None => Ok(None),
        Some(target) => convert(target, ctx).map_err(|e| ctx.wrap(e)),
    }
}

/// A `Vec` field: one value per match, in document order.
#[doc(hidden)]
pub fn all<'a, T>(
    scope: Scope<'a>,
    ctx: &Ctx,
    mut convert: impl FnMut(Scope<'a>, &Ctx) -> Converted<T>,
) -> Result<Vec<T>, ExtractError> {
    let Some(css) = ctx.select else {
        // The macro requires `select` on a `Vec`; read the scope itself.
        return one(scope, ctx, convert).map(|v| alloc::vec![v]);
    };
    let sel = ctx.parse_selector(css)?;
    let mut out = Vec::new();
    for (i, node) in scope.select(&sel).enumerate() {
        match convert(Scope::Element(node), ctx) {
            Ok(Some(v)) => out.push(v),
            Ok(None) => {
                let e = ctx.missing_attribute(Scope::Element(node)).at_index(i);
                return Err(ctx.wrap(e));
            }
            Err(e) => return Err(ctx.wrap(e.at_index(i))),
        }
    }
    Ok(out)
}

/// Convert with `T`'s own `FromHtml` (nested structs and leaf types).
#[doc(hidden)]
pub fn nested<T: FromHtml>(scope: Scope<'_>, _: &Ctx) -> Converted<T> {
    T::from_html(scope).map(Some)
}

/// Read the field's source and convert it with `FromHtmlValue`.
#[doc(hidden)]
pub fn value<T: FromHtmlValue>(scope: Scope<'_>, ctx: &Ctx) -> Converted<T> {
    let src = ctx.source.unwrap_or(Src::Text);
    match src.read(scope) {
        None => Ok(None),
        Some(value) => parse_value(value, src.value_source()).map(Some),
    }
}

/// Read the field's source and convert it with the `with` function, whose
/// name is `function`.
#[doc(hidden)]
pub fn with<T, E: fmt::Display>(
    scope: Scope<'_>,
    ctx: &Ctx,
    function: &'static str,
    f: impl FnOnce(&str) -> Result<T, E>,
) -> Converted<T> {
    let src = ctx.source.unwrap_or(Src::Text);
    let Some(value) = src.read(scope) else {
        return Ok(None);
    };
    match f(&value) {
        Ok(v) => Ok(Some(v)),
        Err(e) => Err(ExtractError::new(ExtractErrorKind::Convert {
            value: value.into_owned(),
            function: function.into(),
            message: e.to_string(),
        })
        .with_source(src.value_source())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_type_names() {
        assert_eq!(short_type_name::<String>(), "String");
        assert_eq!(short_type_name::<f64>(), "f64");
        assert_eq!(short_type_name::<Vec<String>>(), "Vec<String>");
        assert_eq!(
            short_type_name::<Option<(u8, Vec<String>)>>(),
            "Option<(u8, Vec<String>)>"
        );
        assert_eq!(short_type_name::<Ctx>(), "Ctx");
    }

    #[test]
    fn display_without_path_or_selector() {
        let e = parse_value::<f64>(Cow::Borrowed("x"), ValueSource::Text).unwrap_err();
        assert_eq!(
            e.to_string(),
            r#"cannot parse "x" as f64: invalid float literal"#
        );
        let e = ExtractError::new(ExtractErrorKind::MissingAttribute {
            name: "href".into(),
            element: Some("a".into()),
        })
        .with_source(ValueSource::Attr("href".into()));
        assert_eq!(e.to_string(), r#"(@href): <a> has no "href" attribute"#);
    }

    #[test]
    fn long_values_are_cut_short() {
        let long = "x".repeat(100);
        let e = parse_value::<u8>(Cow::Borrowed(&long), ValueSource::Text).unwrap_err();
        let shown = alloc::format!("{:?}…", "x".repeat(MAX_VALUE_CHARS));
        assert_eq!(
            e.to_string(),
            alloc::format!("cannot parse {shown} as u8: invalid digit found in string")
        );
        match e.kind() {
            ExtractErrorKind::Parse { value, .. } => assert_eq!(value.len(), 100),
            other => panic!("unexpected {other:?}"),
        }
    }
}
