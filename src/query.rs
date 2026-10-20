//! A small, hand-rolled CSS selector engine (behind the `query` feature).
//!
//! Zero dependencies (no `selectors`/`cssparser`), `no_std + alloc`, and it
//! never panics: any selector string against any document simply matches or
//! doesn't. Pass a `&str` to [`Dom::select`] / [`NodeRef::select`], or parse
//! once with [`Selector::parse`] to reuse it and to get a [`SelectorError`]
//! (with a byte position) for invalid input.
//!
//! ## Supported grammar
//!
//! - **Simple selectors:** type `div`, universal `*`, `#id`, `.class`, and any
//!   compound of them (`div.card.active#main`).
//! - **Selector lists** `a, b` and all four **combinators**: descendant
//!   `a b`, child `a > b`, next-sibling `a + b`, subsequent-sibling `a ~ b`.
//! - **Attributes:** `[a]`, `[a=v]`, `[a~=v]`, `[a|=v]`, `[a^=v]`, `[a$=v]`,
//!   `[a*=v]`, with quoted or unquoted values and the ` i` / ` s` case flags.
//!   As in CSS, an empty value never matches `^=`, `$=`, `*=` or `~=`.
//!   Unquoted values may contain anything but whitespace, quotes and brackets
//!   (a lenient superset of CSS, so `a[href=/about]` works).
//! - **Structural pseudo-classes:** `:first-child`, `:last-child`,
//!   `:only-child`, `:nth-child(An+B [of S])`, `:nth-last-child(An+B [of S])`,
//!   `:first-of-type`, `:last-of-type`, `:only-of-type`, `:nth-of-type(An+B)`,
//!   `:nth-last-of-type(An+B)`, `:empty`, `:root`, `:scope`. `An+B` accepts
//!   `odd`, `even`, `n`, `-n+3`, `2n + 1`, `5`, ….
//! - **Logical pseudo-classes:** `:not(list)`, `:is(list)`, `:where(list)`,
//!   and `:has(relative list)` (`:has(img)`, `:has(> img)`, `:has(+ p)`,
//!   `:has(~ p)`).
//! - **Form state** (from attributes, as written in the markup): `:checked`
//!   (`<input checked>`, `<option selected>`), `:disabled`, `:enabled`.
//! - **`:contains("text")`** — *non-standard*, jQuery/cheerio-style scraping
//!   extension: the element's [`text_content`](NodeRef::text_content) contains
//!   the given text (case-sensitive substring; quotes optional).
//! - **CSS escapes** in names and strings: `.md\:flex`, `#a\.b`, `.\31 0`
//!   (hex), so Tailwind-style class names work.
//!
//! ## Matching rules
//!
//! - Tag names, attribute names and pseudo-class names are ASCII
//!   case-insensitive; attribute values, ids and classes are case-sensitive
//!   (unless the attribute selector has the ` i` flag).
//! - Structural pseudo-classes and sibling combinators count **elements only**;
//!   text and comment nodes are skipped.
//! - Top-level nodes have no parent and are not linked as siblings in this
//!   crate's tree, so every top-level element is `:root` and is its own
//!   first/last/only child.
//! - `:scope` is the element a query runs on ([`NodeRef::select`],
//!   [`NodeRef::matches`], [`NodeRef::closest`]); for [`Dom::select`] and
//!   [`Selector::matches`] it is the same as `:root`.
//!
//! Matching never recurses on document depth and never backtracks
//! exponentially: a descendant chain like `div div div span` costs time
//! linear in the candidate's depth, so 100k-deep documents are fine.
//! `:has()` and `:contains()` scan each candidate's subtree (iteratively), and
//! each level of argument nesting multiplies the work, so prefer flat
//! selectors on very large documents.
//!
//! ## Deliberately missing
//!
//! Pseudo-elements (`::before`), dynamic and UI pseudo-classes (`:hover`,
//! `:focus`, `:visited`, …), `:lang()`, `:dir()`, namespaces (`svg|rect`), the
//! column combinator `||`, `/* comments */`, and `:has()` nested inside
//! `:has()` (also invalid CSS). These are parse **errors** — they match nothing
//! rather than being silently ignored. Arguments nest at most 32 levels deep.

use alloc::borrow::Cow;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use core::str::FromStr;

use crate::node::NodeId;
use crate::tree::{Dom, NodeRef};

/// How deep `:not(:is(...))`-style arguments may nest before it's an error.
const MAX_DEPTH: u32 = 32;

// --- public API ----------------------------------------------------------

/// A parsed, reusable CSS selector list.
///
/// Parse once, match many times: pass `&selector` anywhere a selector string
/// is accepted ([`Dom::select`], [`NodeRef::closest`], …).
///
/// Supported: type, `*`, `#id`, `.class`, attribute selectors (`=`, `~=`,
/// `|=`, `^=`, `$=`, `*=`, with ` i`/` s` flags), selector lists, all four
/// combinators (descendant, `>`, `+`, `~`), `:first-child`, `:last-child`,
/// `:only-child`, `:nth-child(An+B [of S])`, `:nth-last-child()`, the
/// `*-of-type` variants, `:empty`, `:root`, `:scope`, `:not()`, `:is()`, `:where()`,
/// `:has()` (relative: `:has(> img)`, `:has(+ p)`), `:checked`, `:disabled`,
/// `:enabled`, CSS escapes (`.md\:flex`), and the non-standard jQuery-style
/// `:contains("text")` (case-sensitive substring of the text content).
/// Tag, attribute and pseudo-class names are ASCII case-insensitive; ids,
/// classes and attribute values are case-sensitive. Anything else (`:hover`,
/// `::before`, namespaces, …) is a parse error, never silently ignored.
///
/// Sibling combinators and positional pseudo-classes count elements only, and
/// only among children of the same parent: the top-level nodes of a fragment
/// are not linked as siblings, so each top-level element is `:root` and its
/// own `:first-child`.
///
/// ```
/// use domtree::Selector;
///
/// let dom = domtree::parse("<ul><li class='on'>a</li><li>b</li></ul>");
/// let sel = Selector::parse("li:not(.on)").unwrap();
/// let hits: Vec<_> = dom.select(&sel).map(|n| n.text_content()).collect();
/// assert_eq!(hits, ["b"]);
///
/// // `FromStr` works too.
/// let sel: Selector = "ul > li".parse().unwrap();
/// assert_eq!(dom.select(&sel).count(), 2);
/// ```
#[derive(Clone)]
pub struct Selector {
    list: Vec<Complex>,
    source: String,
}

impl Selector {
    /// Parse a selector list such as `"ul > li.active, a[href^='https:']"`.
    ///
    /// Anything outside the supported grammar is an error — including
    /// pseudo-classes this engine doesn't implement (`:hover`, `::before`, …),
    /// which are rejected rather than silently ignored.
    ///
    /// ```
    /// use domtree::Selector;
    ///
    /// assert!(Selector::parse("div > p:first-child").is_ok());
    ///
    /// let err = Selector::parse("div > p)").unwrap_err();
    /// assert_eq!(err.position(), 7);
    /// assert_eq!(err.to_string(), "unexpected ')' at byte 7");
    /// ```
    pub fn parse(selector: &str) -> Result<Selector, SelectorError> {
        let mut p = Parser {
            src: selector,
            pos: 0,
            depth: 0,
            in_has: false,
        };
        let list = p.list(false)?;
        match p.peek() {
            None => Ok(Selector {
                list,
                source: selector.into(),
            }),
            Some(_) => p.unexpected(),
        }
    }

    /// Whether `node` is an element matching this selector, evaluated against
    /// the whole document `node` belongs to.
    ///
    /// ```
    /// use domtree::Selector;
    ///
    /// let dom = domtree::parse("<div class='card'><h2>Title</h2></div>");
    /// let h2 = dom.find_by_tag("h2").next().unwrap();
    /// assert!(Selector::parse(".card > h2").unwrap().matches(h2));
    /// assert!(!Selector::parse("h2:empty").unwrap().matches(h2));
    /// ```
    pub fn matches(&self, node: NodeRef<'_>) -> bool {
        self.matches_in(node, Ctx::default())
    }

    fn matches_in(&self, node: NodeRef<'_>, cx: Ctx) -> bool {
        any_matches(&self.list, node, cx)
    }
}

/// Same as [`Selector::parse`].
impl FromStr for Selector {
    type Err = SelectorError;

    fn from_str(s: &str) -> Result<Selector, SelectorError> {
        Selector::parse(s)
    }
}

impl fmt::Debug for Selector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Selector").field(&self.source).finish()
    }
}

/// Why [`Selector::parse`] rejected a selector, and where.
///
/// ```
/// let err = domtree::Selector::parse("a:hover").unwrap_err();
/// assert_eq!(err.position(), 1);
/// assert_eq!(err.message(), "unsupported pseudo-class ':hover'");
/// assert_eq!(err.to_string(), "unsupported pseudo-class ':hover' at byte 1");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorError {
    position: usize,
    message: String,
}

impl SelectorError {
    /// Byte offset into the selector string where the problem was found.
    ///
    /// ```
    /// let err = domtree::Selector::parse("ul >").unwrap_err();
    /// assert_eq!(err.position(), 4);
    /// ```
    pub fn position(&self) -> usize {
        self.position
    }

    /// A short, human-readable description of the problem.
    ///
    /// ```
    /// let err = domtree::Selector::parse("p::before").unwrap_err();
    /// assert_eq!(err.message(), "pseudo-elements are not supported");
    /// ```
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for SelectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.message, self.position)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SelectorError {}

/// Anything the selector methods accept: a `&str` or `&String` (parsed on the
/// spot) or an already-parsed `&`[`Selector`].
///
/// A string that fails to parse matches nothing; use [`Selector::parse`] when
/// you want the error.
///
/// ```
/// use domtree::{AsSelector, Selector};
///
/// let dom = domtree::parse("<p class='x'>hi</p>");
/// let owned = String::from("p.x");
/// let parsed = Selector::parse("p.x").unwrap();
/// assert_eq!(dom.select("p.x").count(), 1);
/// assert_eq!(dom.select(&owned).count(), 1);
/// assert_eq!(dom.select(&parsed).count(), 1);
///
/// assert!("p:hover".as_selector().is_none()); // invalid: matches nothing
/// assert_eq!(dom.select("p:hover").count(), 0);
/// ```
pub trait AsSelector {
    /// The parsed selector, or `None` if this is not a valid selector.
    fn as_selector(&self) -> Option<Cow<'_, Selector>>;
}

impl AsSelector for str {
    fn as_selector(&self) -> Option<Cow<'_, Selector>> {
        Selector::parse(self).ok().map(Cow::Owned)
    }
}

impl AsSelector for String {
    fn as_selector(&self) -> Option<Cow<'_, Selector>> {
        self.as_str().as_selector()
    }
}

impl AsSelector for Selector {
    fn as_selector(&self) -> Option<Cow<'_, Selector>> {
        Some(Cow::Borrowed(self))
    }
}

impl Dom {
    /// Every element matching a CSS selector, in document order.
    ///
    /// `selector` may be a `&str`, a `&String` or a pre-parsed `&`[`Selector`]
    /// (see [`AsSelector`]). It is parsed once per call; an invalid selector
    /// string matches nothing (use [`Selector::parse`] to see the error).
    ///
    /// ```
    /// let dom = domtree::parse(
    ///     "<div><ul><li class='x'>a</li><li>b</li></ul><ol><li>c</li></ol></div>",
    /// );
    /// assert_eq!(dom.select("ul li.x").count(), 1); // descendant + class
    /// assert_eq!(dom.select("ol > li").count(), 1); // child combinator
    /// assert_eq!(dom.select("ul, ol").count(), 2); // selector list
    /// assert_eq!(dom.select("li:nth-child(2)").count(), 1);
    /// assert_eq!(dom.select("ul:has(.x) + ol").count(), 1);
    /// ```
    pub fn select<'a, S: AsSelector + ?Sized>(
        &'a self,
        selector: &S,
    ) -> impl Iterator<Item = NodeRef<'a>> {
        let sel = selector.as_selector();
        self.nodes()
            .filter(move |n| sel.as_ref().is_some_and(|s| s.matches(*n)))
    }

    /// The first element (in document order) matching a CSS selector.
    ///
    /// ```
    /// let dom = domtree::parse("<p>a</p><p class='x'>b</p><p class='x'>c</p>");
    /// let p = dom.select_first("p.x").unwrap();
    /// assert_eq!(p.text_content(), "b");
    /// assert!(dom.select_first("table").is_none());
    /// ```
    pub fn select_first<S: AsSelector + ?Sized>(&self, selector: &S) -> Option<NodeRef<'_>> {
        self.select(selector).next()
    }
}

impl<'a> NodeRef<'a> {
    /// Descendants of this node (not the node itself) matching `selector`, in
    /// document order.
    ///
    /// Like the DOM's `querySelectorAll`, the selector is matched against the
    /// whole document — only the *results* are limited to this subtree. So
    /// `"div p"` also finds a `<p>` whose `<div>` is an ancestor of this node.
    /// Use `:scope` to anchor at this node: `":scope > li"` is its `<li>`
    /// children.
    ///
    /// ```
    /// let dom = domtree::parse("<div><ul><li>a<ul><li>b</li></ul></li></ul></div>");
    /// let ul = dom.select_first("ul").unwrap();
    /// assert_eq!(ul.select("li").count(), 2);
    /// assert_eq!(ul.select(":scope > li").count(), 1);
    /// assert_eq!(ul.select("div li").count(), 2); // the <div> is outside `ul`
    /// ```
    pub fn select<'s, S: AsSelector + ?Sized>(
        &self,
        selector: &'s S,
    ) -> impl Iterator<Item = NodeRef<'a>> + use<'a, 's, S> {
        let sel = selector.as_selector();
        let cx = Ctx::scoped(self.id());
        self.descendants()
            .filter(move |n| sel.as_ref().is_some_and(|s| s.matches_in(*n, cx)))
    }

    /// The first descendant (in document order) matching `selector`; see
    /// [`NodeRef::select`].
    ///
    /// ```
    /// let dom = domtree::parse("<table><tr><td>1</td><td>2</td></tr></table>");
    /// let row = dom.select_first("tr").unwrap();
    /// assert_eq!(row.select_first("td:last-child").unwrap().text_content(), "2");
    /// ```
    pub fn select_first<S: AsSelector + ?Sized>(&self, selector: &S) -> Option<NodeRef<'a>> {
        self.select(selector).next()
    }

    /// Whether this node is an element matching `selector` (DOM
    /// `Element.matches`). An invalid selector matches nothing.
    ///
    /// ```
    /// let dom = domtree::parse("<ul><li class='a'>x</li></ul>");
    /// let li = dom.select_first("li").unwrap();
    /// assert!(li.matches("ul > li.a"));
    /// assert!(!li.matches("ol li"));
    /// ```
    pub fn matches<S: AsSelector + ?Sized>(&self, selector: &S) -> bool {
        selector
            .as_selector()
            .is_some_and(|s| s.matches_in(*self, Ctx::scoped(self.id())))
    }

    /// The nearest element matching `selector`, starting with this node itself
    /// and walking up its ancestors (DOM `Element.closest`).
    ///
    /// ```
    /// let dom = domtree::parse("<div class='card'><p><b>hi</b></p></div>");
    /// let b = dom.select_first("b").unwrap();
    /// assert_eq!(b.closest(".card").unwrap().tag_name(), Some("div"));
    /// assert_eq!(b.closest("b").unwrap().id(), b.id()); // self-inclusive
    /// assert!(b.closest("table").is_none());
    /// ```
    pub fn closest<S: AsSelector + ?Sized>(&self, selector: &S) -> Option<NodeRef<'a>> {
        let sel = selector.as_selector()?;
        let cx = Ctx::scoped(self.id());
        core::iter::once(*self)
            .chain(self.ancestors())
            .find(|n| sel.matches_in(*n, cx))
    }
}

// --- representation ------------------------------------------------------

/// One complex selector (`ul > li a`), stored **right to left**:
/// `compounds[0]` is the subject, and `combs[i]` links `compounds[i]` to the
/// compound on its left, `compounds[i + 1]`.
#[derive(Clone)]
struct Complex {
    compounds: Vec<Compound>,
    combs: Vec<Comb>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Comb {
    Descendant,
    Child,
    Next,
    Later,
}

#[derive(Clone)]
struct Compound {
    /// Lower-cased type selector; `None` for `*` or no type.
    tag: Option<String>,
    /// Ordered cheapest-first, so `tag.class` rejections short-circuit before
    /// anything expensive (`:has`, `:contains`, …) runs.
    simples: Vec<Simple>,
}

#[derive(Clone)]
enum Simple {
    Id(String),
    Class(String),
    Attr {
        name: String,
        op: AttrOp,
        value: String,
        ci: bool,
    },
    Nth(Nth),
    /// `:only-child` (`false`) / `:only-of-type` (`true`).
    Only(bool),
    Empty,
    Root,
    Scope,
    /// The element a `:has()` argument is relative to (internal only).
    Anchor,
    Checked,
    Disabled,
    Enabled,
    Not(Vec<Complex>),
    Is(Vec<Complex>),
    Has(Vec<Complex>),
    Contains(String),
}

#[derive(Clone, Copy)]
enum AttrOp {
    Exists,
    Equals,
    Includes,
    Dash,
    Prefix,
    Suffix,
    Substring,
}

/// `:nth-*(An+B)` and the `:first-*` / `:last-*` shorthands (`0n+1`).
#[derive(Clone)]
struct Nth {
    a: i32,
    b: i32,
    last: bool,
    of_type: bool,
    /// `:nth-child(An+B of S)`; empty when absent.
    of: Vec<Complex>,
}

impl Simple {
    fn cost(&self) -> u8 {
        match self {
            Simple::Id(_) | Simple::Class(_) => 0,
            Simple::Not(_) | Simple::Is(_) => 2,
            Simple::Has(_) | Simple::Contains(_) => 3,
            _ => 1,
        }
    }
}

// --- parsing -------------------------------------------------------------

/// A recursive-descent parser over the selector string. Recursion only
/// happens for nested arguments (`:not(...)`), bounded by [`MAX_DEPTH`].
struct Parser<'s> {
    src: &'s str,
    pos: usize,
    depth: u32,
    in_has: bool,
}

impl<'s> Parser<'s> {
    fn rest(&self) -> &'s str {
        self.src.get(self.pos..).unwrap_or("")
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn eat(&mut self, c: char) -> bool {
        let hit = self.peek() == Some(c);
        if hit {
            self.pos += c.len_utf8();
        }
        hit
    }

    /// Skip CSS whitespace; returns whether there was any.
    fn ws(&mut self) -> bool {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
            self.pos += 1;
        }
        self.pos > start
    }

    fn error<T>(&self, at: usize, message: impl Into<String>) -> Result<T, SelectorError> {
        Err(SelectorError {
            position: at,
            message: message.into(),
        })
    }

    fn unexpected<T>(&self) -> Result<T, SelectorError> {
        match self.peek() {
            Some(c) => self.error(self.pos, format!("unexpected '{c}'")),
            None => self.error(self.pos, "unexpected end of selector"),
        }
    }

    /// `complex (',' complex)*`, stopping before a `)` or the end.
    fn list(&mut self, relative: bool) -> Result<Vec<Complex>, SelectorError> {
        let mut list = Vec::new();
        loop {
            list.push(self.complex(relative)?);
            if !self.eat(',') {
                return Ok(list);
            }
        }
    }

    /// A list nested in a pseudo-class argument, with the depth bounded.
    fn nested(&mut self, relative: bool) -> Result<Vec<Complex>, SelectorError> {
        if self.depth >= MAX_DEPTH {
            return self.error(self.pos, "selectors nested too deeply");
        }
        self.depth += 1;
        let list = self.list(relative);
        self.depth -= 1;
        list
    }

    fn combinator(&mut self) -> Option<Comb> {
        let comb = match self.peek()? {
            '>' => Comb::Child,
            '+' => Comb::Next,
            '~' => Comb::Later,
            _ => return None,
        };
        self.pos += 1;
        self.ws();
        Some(comb)
    }

    fn complex(&mut self, relative: bool) -> Result<Complex, SelectorError> {
        self.ws();
        let mut compounds = Vec::new();
        let mut combs = Vec::new();
        if relative {
            // `:has(> img)` is `<anchor> > img`; a bare `:has(img)` is a
            // descendant of the anchor.
            compounds.push(Compound {
                tag: None,
                simples: alloc::vec![Simple::Anchor],
            });
            combs.push(self.combinator().unwrap_or(Comb::Descendant));
        }
        loop {
            compounds.push(self.compound()?);
            let spaced = self.ws();
            let comb = match self.combinator() {
                Some(comb) => comb,
                None => match self.peek() {
                    None | Some(',' | ')') => break,
                    Some(_) if spaced => Comb::Descendant,
                    Some(_) => return self.unexpected(),
                },
            };
            combs.push(comb);
        }
        compounds.reverse();
        combs.reverse();
        Ok(Complex { compounds, combs })
    }

    fn compound(&mut self) -> Result<Compound, SelectorError> {
        let start = self.pos;
        let mut tag = None;
        if !self.eat('*') && self.peek().is_some_and(|c| c == '\\' || is_name_char(c)) {
            tag = Some(self.name()?.to_ascii_lowercase());
        }
        let mut simples: Vec<Simple> = Vec::new();
        loop {
            let at = self.pos;
            let simple = match self.peek() {
                Some('#') => {
                    self.pos += 1;
                    Simple::Id(self.name()?)
                }
                Some('.') => {
                    self.pos += 1;
                    Simple::Class(self.name()?)
                }
                Some('[') => {
                    self.pos += 1;
                    self.attribute()?
                }
                Some(':') => {
                    self.pos += 1;
                    self.pseudo(at)?
                }
                _ => break,
            };
            // Insertion sort by cost (keeps the code tiny; compounds are short).
            let i = simples
                .iter()
                .position(|s| s.cost() > simple.cost())
                .unwrap_or(simples.len());
            simples.insert(i, simple);
        }
        if self.pos == start {
            return match self.peek() {
                None => self.error(start, "expected a selector"),
                Some(_) => self.unexpected(),
            };
        }
        Ok(Compound { tag, simples })
    }

    /// An identifier-ish name: name characters plus CSS escapes.
    fn name(&mut self) -> Result<String, SelectorError> {
        let mut out = String::new();
        while let Some(c) = self.peek() {
            if c == '\\' {
                self.pos += 1;
                out.push(self.escape());
            } else if is_name_char(c) {
                self.pos += c.len_utf8();
                out.push(c);
            } else {
                break;
            }
        }
        if out.is_empty() {
            return self.unexpected();
        }
        Ok(out)
    }

    /// Decode a CSS escape whose `\` was just consumed: 1–6 hex digits (plus
    /// one optional trailing space), or any other character taken literally.
    fn escape(&mut self) -> char {
        let start = self.pos;
        let mut code = 0u32;
        while self.pos - start < 6 {
            match self.peek().and_then(|c| c.to_digit(16)) {
                Some(d) => {
                    code = code * 16 + d;
                    self.pos += 1;
                }
                None => break,
            }
        }
        if self.pos > start {
            if self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
                self.pos += 1;
            }
            return char::from_u32(code)
                .filter(|&c| c != '\0')
                .unwrap_or('\u{FFFD}');
        }
        self.bump().unwrap_or('\u{FFFD}')
    }

    /// A `"…"` or `'…'` string starting at the current position.
    fn string(&mut self) -> Result<String, SelectorError> {
        let start = self.pos;
        let quote = self.bump();
        let mut out = String::new();
        loop {
            match self.bump() {
                None => return self.error(start, "unterminated string"),
                Some('\\') => match self.peek() {
                    Some('\n') => self.pos += 1, // line continuation
                    Some(_) => out.push(self.escape()),
                    None => {}
                },
                Some(c) if Some(c) == quote => return Ok(out),
                Some(c) => out.push(c),
            }
        }
    }

    /// `[name]`, `[name op value flag?]`; the `[` was just consumed.
    fn attribute(&mut self) -> Result<Simple, SelectorError> {
        const OPS: [(&str, AttrOp); 6] = [
            ("=", AttrOp::Equals),
            ("~=", AttrOp::Includes),
            ("|=", AttrOp::Dash),
            ("^=", AttrOp::Prefix),
            ("$=", AttrOp::Suffix),
            ("*=", AttrOp::Substring),
        ];
        self.ws();
        let name = self.name()?.to_ascii_lowercase();
        self.ws();
        if self.eat(']') {
            return Ok(Simple::Attr {
                name,
                op: AttrOp::Exists,
                value: String::new(),
                ci: false,
            });
        }
        let rest = self.rest();
        let Some(&(token, op)) = OPS.iter().find(|(t, _)| rest.starts_with(t)) else {
            return self.unexpected();
        };
        self.pos += token.len();
        self.ws();
        let value = match self.peek() {
            Some('"' | '\'') => self.string()?,
            _ => self.unquoted()?,
        };
        self.ws();
        let mut ci = false;
        if let Some(flag @ ('i' | 'I' | 's' | 'S')) = self.peek() {
            self.pos += 1;
            ci = flag.eq_ignore_ascii_case(&'i');
            self.ws();
        }
        if !self.eat(']') {
            return self.unexpected();
        }
        Ok(Simple::Attr {
            name,
            op,
            value,
            ci,
        })
    }

    /// An unquoted attribute value: anything up to whitespace, a quote or a
    /// bracket (more lenient than CSS identifiers, e.g. `[href=/a/b]`).
    fn unquoted(&mut self) -> Result<String, SelectorError> {
        let mut out = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() || matches!(c, '[' | ']' | '"' | '\'') {
                break;
            }
            self.pos += c.len_utf8();
            if c == '\\' {
                out.push(self.escape());
            } else {
                out.push(c);
            }
        }
        if out.is_empty() {
            return self.unexpected();
        }
        Ok(out)
    }

    /// A pseudo-class; `at` is the byte offset of its `:` (already consumed).
    fn pseudo(&mut self, at: usize) -> Result<Simple, SelectorError> {
        if self.peek() == Some(':') {
            return self.error(at, "pseudo-elements are not supported");
        }
        let name = self.name()?.to_ascii_lowercase();
        if !self.eat('(') {
            let nth = |last, of_type| {
                Simple::Nth(Nth {
                    a: 0,
                    b: 1,
                    last,
                    of_type,
                    of: Vec::new(),
                })
            };
            return Ok(match name.as_str() {
                "first-child" => nth(false, false),
                "last-child" => nth(true, false),
                "first-of-type" => nth(false, true),
                "last-of-type" => nth(true, true),
                "only-child" => Simple::Only(false),
                "only-of-type" => Simple::Only(true),
                "empty" => Simple::Empty,
                "root" => Simple::Root,
                "scope" => Simple::Scope,
                "checked" => Simple::Checked,
                "disabled" => Simple::Disabled,
                "enabled" => Simple::Enabled,
                _ => return self.error(at, format!("unsupported pseudo-class ':{name}'")),
            });
        }
        self.ws();
        let simple = match name.as_str() {
            "not" => Simple::Not(self.nested(false)?),
            "is" | "where" => Simple::Is(self.nested(false)?),
            "has" if self.in_has => return self.error(at, ":has() cannot be nested"),
            "has" => {
                self.in_has = true;
                let list = self.nested(true);
                self.in_has = false;
                Simple::Has(list?)
            }
            "contains" => Simple::Contains(self.contains_arg()?),
            "nth-child" | "nth-last-child" | "nth-of-type" | "nth-last-of-type" => {
                self.nth(&name)?
            }
            _ => return self.error(at, format!("unsupported pseudo-class ':{name}()'")),
        };
        self.ws();
        if !self.eat(')') {
            return self.unexpected();
        }
        Ok(simple)
    }

    /// `:contains("text")`, `:contains('text')` or `:contains(text)`.
    fn contains_arg(&mut self) -> Result<String, SelectorError> {
        if let Some('"' | '\'') = self.peek() {
            return self.string();
        }
        let rest = self.rest();
        let end = rest.find(')').unwrap_or(rest.len());
        let text = rest.get(..end).unwrap_or("").trim_end();
        if text.is_empty() {
            return self.unexpected();
        }
        self.pos += text.len();
        Ok(text.into())
    }

    fn nth(&mut self, name: &str) -> Result<Simple, SelectorError> {
        let (a, b) = self.an_plus_b()?;
        let of_type = name.ends_with("of-type");
        let mut of = Vec::new();
        if !of_type
            && self.ws()
            && self
                .rest()
                .get(..2)
                .is_some_and(|k| k.eq_ignore_ascii_case("of"))
        {
            self.pos += 2;
            if !self.ws() {
                return self.unexpected();
            }
            of = self.nested(false)?;
        }
        Ok(Simple::Nth(Nth {
            a,
            b,
            last: name.starts_with("nth-last"),
            of_type,
            of,
        }))
    }

    /// `odd`, `even`, `B`, `An`, `An+B` (signs and whitespace as in CSS).
    fn an_plus_b(&mut self) -> Result<(i32, i32), SelectorError> {
        let start = self.pos;
        for (word, ab) in [("odd", (2, 1)), ("even", (2, 0))] {
            if self
                .rest()
                .get(..word.len())
                .is_some_and(|w| w.eq_ignore_ascii_case(word))
            {
                self.pos += word.len();
                return Ok(ab);
            }
        }
        let neg = self.eat('-');
        if !neg {
            self.eat('+');
        }
        let digits = self.integer()?;
        if !(self.eat('n') || self.eat('N')) {
            return match digits {
                Some(b) => Ok((0, if neg { -b } else { b })),
                None => self.error(start, "invalid :nth-*() argument"),
            };
        }
        let a = digits.unwrap_or(1);
        let a = if neg { -a } else { a };
        let before = self.pos;
        self.ws();
        let b = match self.peek() {
            Some(sign @ ('+' | '-')) => {
                self.pos += 1;
                self.ws();
                match self.integer()? {
                    Some(b) if sign == '-' => -b,
                    Some(b) => b,
                    None => return self.unexpected(),
                }
            }
            _ => {
                self.pos = before; // leave the space for a following `of S`
                0
            }
        };
        Ok((a, b))
    }

    fn integer(&mut self) -> Result<Option<i32>, SelectorError> {
        let start = self.pos;
        let mut n: i32 = 0;
        while let Some(d) = self.peek().and_then(|c| c.to_digit(10)) {
            n = match n.checked_mul(10).and_then(|n| n.checked_add(d as i32)) {
                Some(n) => n,
                None => return self.error(start, "number too large"),
            };
            self.pos += 1;
        }
        Ok((self.pos > start).then_some(n))
    }
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_' || !c.is_ascii()
}

// --- matching ------------------------------------------------------------

/// Per-query context.
#[derive(Clone, Copy, Default)]
struct Ctx {
    /// What `:scope` matches (`None`: the document, i.e. top-level elements).
    scope: Option<NodeId>,
    /// The subject of the `:has()` currently being evaluated.
    anchor: Option<NodeId>,
}

impl Ctx {
    fn scoped(scope: NodeId) -> Ctx {
        Ctx {
            scope: Some(scope),
            anchor: None,
        }
    }
}

fn any_matches(list: &[Complex], el: NodeRef<'_>, cx: Ctx) -> bool {
    list.iter().any(|c| c.matches(el, cx))
}

// Matching runs right to left without backtracking across a descendant (or
// `~`) combinator: the selector is cut at descendant combinators into chunks,
// and each chunk at `~` into runs of `>`/`+` (which leave no choice). For each
// chunk (run) it suffices to take the *nearest* ancestor (preceding sibling)
// where it matches: a farther one only leaves fewer candidates for the rest
// (a chunk always ends the same number of `>` levels above where it started).
// So a match costs at most ancestors × siblings × selector length instead of
// exponential time, and it loops instead of recursing on the document.
impl Complex {
    fn matches(&self, el: NodeRef<'_>, cx: Ctx) -> bool {
        let Some((mut el, mut i)) = self.chunk(0, el, cx) else {
            return false;
        };
        // Any remaining combinator is a descendant one.
        while self.combs.get(i).is_some() {
            let mut up = el.parent();
            (el, i) = loop {
                let Some(a) = up else { return false };
                if let Some(hit) = self.chunk(i + 1, a, cx) {
                    break hit;
                }
                up = a.parent();
            };
        }
        true
    }

    /// Match compounds `i..` linked by `>`, `+` and `~`, anchored at `el`.
    /// Returns the element the last of them matched, and its index.
    fn chunk<'a>(&self, i: usize, el: NodeRef<'a>, cx: Ctx) -> Option<(NodeRef<'a>, usize)> {
        let (mut el, mut i) = self.run(i, el, cx)?;
        while self.combs.get(i) == Some(&Comb::Later) {
            let mut prev = prev_element(el);
            (el, i) = loop {
                let p = prev?;
                if let Some(hit) = self.run(i + 1, p, cx) {
                    break hit;
                }
                prev = prev_element(p);
            };
        }
        Some((el, i))
    }

    /// Match compounds `i..` linked by `>` and `+`, anchored at `el`.
    fn run<'a>(&self, mut i: usize, mut el: NodeRef<'a>, cx: Ctx) -> Option<(NodeRef<'a>, usize)> {
        loop {
            if !self.compounds.get(i)?.matches(el, cx) {
                return None;
            }
            el = match self.combs.get(i) {
                Some(Comb::Child) => el.parent()?,
                Some(Comb::Next) => prev_element(el)?,
                _ => return Some((el, i)),
            };
            i += 1;
        }
    }
}

impl Compound {
    fn matches(&self, el: NodeRef<'_>, cx: Ctx) -> bool {
        let Some(tag) = el.tag_name() else {
            return false;
        };
        self.tag
            .as_deref()
            .is_none_or(|t| t.eq_ignore_ascii_case(tag))
            && self.simples.iter().all(|s| s.matches(el, cx))
    }
}

impl Simple {
    fn matches(&self, el: NodeRef<'_>, cx: Ctx) -> bool {
        match self {
            Simple::Id(id) => el.attr("id") == Some(id.as_str()),
            Simple::Class(class) => el.has_class(class),
            Simple::Attr {
                name,
                op,
                value,
                ci,
            } => el
                .attr(name)
                .is_some_and(|have| attr_matches(*op, have, value, *ci)),
            Simple::Nth(nth) => nth.matches(el, cx),
            Simple::Only(of_type) => {
                let same = |s: NodeRef<'_>| !of_type || s.tag_name() == el.tag_name();
                position(el, false, 1, same) == 1 && position(el, true, 1, same) == 1
            }
            Simple::Empty => el
                .children()
                .all(|c| c.comment().is_some() || c.text() == Some("")),
            Simple::Root => el.parent().is_none(),
            Simple::Scope => match cx.scope {
                Some(id) => el.id() == id,
                None => el.parent().is_none(),
            },
            Simple::Anchor => cx.anchor == Some(el.id()),
            Simple::Checked => match el.tag_name() {
                Some("input") => el.has_attr("checked"),
                Some("option") => el.has_attr("selected"),
                _ => false,
            },
            Simple::Disabled => is_form_control(el) && el.has_attr("disabled"),
            Simple::Enabled => is_form_control(el) && !el.has_attr("disabled"),
            Simple::Not(list) => !any_matches(list, el, cx),
            Simple::Is(list) => any_matches(list, el, cx),
            Simple::Has(list) => has(list, el, cx),
            Simple::Contains(text) => el.text_content().contains(text.as_str()),
        }
    }
}

impl Nth {
    fn matches(&self, el: NodeRef<'_>, cx: Ctx) -> bool {
        if !self.of.is_empty() && !any_matches(&self.of, el, cx) {
            return false;
        }
        let counts = |s: NodeRef<'_>| {
            (!self.of_type || s.tag_name() == el.tag_name())
                && (self.of.is_empty() || any_matches(&self.of, s, cx))
        };
        let (a, b) = (i64::from(self.a), i64::from(self.b));
        // With a <= 0 no position past b can match, so stop counting there.
        let cap = if a <= 0 { b } else { i64::MAX };
        let d = position(el, self.last, cap, counts) - b;
        if a == 0 {
            d == 0
        } else {
            d % a == 0 && d / a >= 0
        }
    }
}

/// 1-based position of `el` among its element siblings that pass `counts`,
/// from the end if `last`. Stops counting once the position exceeds `cap`.
fn position<'a>(
    el: NodeRef<'a>,
    last: bool,
    cap: i64,
    counts: impl Fn(NodeRef<'a>) -> bool,
) -> i64 {
    let mut pos = 1;
    let mut sib = if last {
        el.next_sibling()
    } else {
        el.prev_sibling()
    };
    while let Some(s) = sib {
        if pos > cap {
            break;
        }
        if s.is_element() && counts(s) {
            pos += 1;
        }
        sib = if last {
            s.next_sibling()
        } else {
            s.prev_sibling()
        };
    }
    pos
}

fn attr_matches(op: AttrOp, have: &str, want: &str, ci: bool) -> bool {
    let (have, want) = (have.as_bytes(), want.as_bytes());
    let eq = |s: &[u8]| {
        if ci {
            s.eq_ignore_ascii_case(want)
        } else {
            s == want
        }
    };
    match op {
        AttrOp::Exists => true,
        AttrOp::Equals => eq(have),
        AttrOp::Dash => {
            eq(have)
                || (have.get(want.len()) == Some(&b'-') && have.get(..want.len()).is_some_and(eq))
        }
        // An empty value never matches the remaining operators (and must not
        // reach `windows(0)`).
        _ if want.is_empty() => false,
        AttrOp::Includes => have.split(u8::is_ascii_whitespace).any(eq),
        AttrOp::Prefix => have.get(..want.len()).is_some_and(eq),
        AttrOp::Suffix => have
            .len()
            .checked_sub(want.len())
            .and_then(|i| have.get(i..))
            .is_some_and(eq),
        AttrOp::Substring => have.windows(want.len()).any(eq),
    }
}

/// `:has(rel, …)`: some element relative to `el` matches one of `rels`, each
/// anchored at `el`. The candidates are scanned iteratively.
fn has(rels: &[Complex], el: NodeRef<'_>, cx: Ctx) -> bool {
    let cx = Ctx {
        anchor: Some(el.id()),
        ..cx
    };
    rels.iter().any(|rel| match rel.combs.last() {
        // `:has(+ x)` / `:has(~ x)`: later siblings, plus their subtrees when
        // the selector steps back down (`:has(+ ul > li)`).
        Some(Comb::Next | Comb::Later) => {
            let deep = rel
                .combs
                .iter()
                .any(|c| matches!(c, Comb::Child | Comb::Descendant));
            let reach = if rel.combs.iter().all(|c| *c == Comb::Next) {
                rel.combs.len()
            } else {
                usize::MAX
            };
            core::iter::successors(el.next_sibling(), NodeRef::next_sibling)
                .filter(NodeRef::is_element)
                .take(reach)
                .any(|s| {
                    rel.matches(s, cx) || (deep && s.descendants().any(|d| rel.matches(d, cx)))
                })
        }
        _ => el.descendants().any(|d| rel.matches(d, cx)),
    })
}

fn prev_element(el: NodeRef<'_>) -> Option<NodeRef<'_>> {
    core::iter::successors(el.prev_sibling(), NodeRef::prev_sibling).find(NodeRef::is_element)
}

fn is_form_control(el: NodeRef<'_>) -> bool {
    matches!(
        el.tag_name(),
        Some("button" | "input" | "select" | "textarea" | "optgroup" | "option" | "fieldset")
    )
}
