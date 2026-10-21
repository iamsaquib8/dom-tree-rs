//! The tree builder: applies the recovery rules to a token stream.
//!
//! Parsing is infallible. The builder keeps an "open elements" stack and
//! recovers from the common real-world mistakes — omitted end tags, mis-nested
//! tags, stray end tags — recording each as a [`ParseError`] rather than
//! bailing out. See the crate docs for the exact recovery rules we implement.
//!
//! An end tag looks for the element it closes among the
//! [`MAX_END_TAG_SEARCH`] (256) innermost open elements, then among the
//! [`OUTER_END_TAG_SEARCH`] (8) outermost ones — where `</body>`, `</html>` or
//! a page wrapper's end tag finds its element under a pile of unclosed tags.
//! An element open only in between is out of reach: its end tag is treated as
//! stray ([`ErrorKind::StrayEndTag`]) and the element stays open until an end
//! tag that does reach it, or the end of the input. That makes every end tag
//! O(1) however deep the stack — an unbounded scan is quadratic on
//! `<div>`×N then `</x>`×N — with no per-name bookkeeping (which an attacker
//! could make collide), so the event stream's memory stays proportional to
//! the nesting depth.
//!
//! The builder does not know what it builds: it drives a [`Sink`]. The
//! [`DomSink`] turns what it hears into the [`Dom`] arena, and the streaming
//! API ([`crate::events`]) turns it into events — so both apply exactly the
//! same recovery, by construction. The builder is generic over the sink, so
//! the `Dom` path compiles to the same straight-line code it always did.
//!
//! The `Dom` also gets each node's source range (see
//! [`NodeRef::source_range`](crate::NodeRef::source_range)): leaf nodes get
//! their token's range, and an element's end is filled in when it is popped
//! off the open-element stack.

use alloc::borrow::Cow;
use alloc::vec::Vec;

use crate::error::{ErrorKind, ParseError};
use crate::node::{AttrRange, NodeId, RawKind, Span, SrcRange};
use crate::tags;
use crate::token::{AttrValue, Token};
use crate::tokenizer::Tokenizer;
use crate::tree::Dom;

/// Parse `src` into a best-effort [`Dom`]. Never panics; diagnostics land in
/// [`Dom::errors`].
pub(crate) fn parse_document(src: &str) -> Dom {
    let mut tk = Tokenizer::new(src);
    let mut builder = TreeBuilder::new(DomSink::new(src.len()), src);
    while let Some((tok, pos)) = tk.next_token() {
        let end = tk.offset();
        builder.token(tok, tk.attrs(), pos, end);
    }
    builder.finish(src.len(), &mut tk.errors);
    let mut dom = builder.sink.dom;
    dom.errors = builder.errors;
    dom.release_spare_capacity();
    dom
}

/// What the tree builder tells about the document it recovers, in document
/// order. Byte offsets are into the source; `start..end` is a token's range.
pub(crate) trait Sink<'a> {
    /// What the open-elements stack keeps for each open element.
    type Open;

    /// The lower-cased tag name of an open element.
    fn name<'s>(&'s self, open: &'s Self::Open) -> &'s str;

    /// One attribute of the start tag about to be reported by
    /// [`Sink::start`], as written: the name not yet lower-cased, the value
    /// not yet entity-decoded. Returns where decoding met the first
    /// reference-shaped `&…` it left verbatim: a byte offset into `value`
    /// (see [`crate::entities::decode_into`]).
    fn attr(&mut self, name: &'a str, value: &'a str) -> Option<usize>;

    /// An element starts, inside `parent` (`None`: at the top level). Its
    /// `attrs` attributes were just passed to [`Sink::attr`].
    fn start(
        &mut self,
        parent: Option<&Self::Open>,
        name: Cow<'a, str>,
        attrs: usize,
        start: usize,
        end: usize,
    ) -> Self::Open;

    /// An element ends: at its own end tag (`start..end`), or — `implied` —
    /// where recovery closed it (`start == end`; also right after a void or
    /// self-closing start tag).
    fn end(&mut self, open: Self::Open, start: usize, end: usize, implied: bool);

    /// Character data, entity-decoded if `decode`. Returns where decoding met
    /// the first reference-shaped `&…` it left verbatim: a byte offset into
    /// `text`.
    fn text(
        &mut self,
        parent: Option<&Self::Open>,
        text: &'a str,
        decode: bool,
        start: usize,
        end: usize,
    ) -> Option<usize>;

    /// A comment (or bogus comment).
    fn comment(&mut self, parent: Option<&Self::Open>, text: &'a str, start: usize, end: usize);

    /// A doctype, its text trimmed.
    fn doctype(&mut self, parent: Option<&Self::Open>, text: &'a str, start: usize, end: usize);
}

/// The recovery state machine: feed it tokens with [`TreeBuilder::token`],
/// then call [`TreeBuilder::finish`] once.
pub(crate) struct TreeBuilder<'a, S: Sink<'a>> {
    pub(crate) sink: S,
    open: OpenElements<S::Open>,
    /// Recovery diagnostics; complete and sorted after `finish`.
    pub(crate) errors: Vec<ParseError>,
    /// The source the tokens borrow from.
    src: &'a str,
}

impl<'a, S: Sink<'a>> TreeBuilder<'a, S> {
    /// A builder for the tokens of `src`.
    pub(crate) fn new(sink: S, src: &'a str) -> Self {
        TreeBuilder {
            sink,
            open: OpenElements::default(),
            errors: Vec::new(),
            src,
        }
    }

    /// The byte offset in the source of `s`, a slice of it — as every string
    /// in a token is (an attribute value, say, which the tokenizer hands over
    /// without its position). `None` if `s` is not inside the source.
    fn source_offset(&self, s: &str) -> Option<usize> {
        let offset = s.as_ptr().addr().checked_sub(self.src.as_ptr().addr())?;
        (offset.checked_add(s.len())? <= self.src.len()).then_some(offset)
    }

    /// Feed one token that spans `pos..end`. A start tag's attributes come
    /// separately (see [`Tokenizer::attrs`]).
    #[inline]
    pub(crate) fn token(
        &mut self,
        tok: Token<'a>,
        attrs: &[(&'a str, AttrValue<'a>)],
        pos: usize,
        end: usize,
    ) {
        match tok {
            Token::StartTag {
                name, self_closing, ..
            } => self.start_tag(name, attrs, self_closing, pos, end),
            Token::EndTag { name } => self.end_tag(name, pos, end),
            Token::Text(t) => self.text(t, pos, end),
            Token::Cdata(c) => {
                if !c.is_empty() {
                    self.sink.text(self.open.top(), c, false, pos, end);
                }
            }
            Token::Comment(c) => self.sink.comment(self.open.top(), c, pos, end),
            Token::Doctype(d) => self.sink.doctype(self.open.top(), d.trim(), pos, end),
        }
    }

    /// The input ended at byte `len`: everything still open runs to it. Folds
    /// in the tokenizer's `lexical` diagnostics and orders all by position.
    pub(crate) fn finish(&mut self, len: usize, lexical: &mut Vec<ParseError>) {
        if self.open.len() > 0 {
            self.error(len, ErrorKind::ImplicitlyClosed);
        }
        while self.open.len() > 0 {
            self.pop(len, len, true);
        }
        self.errors.append(lexical);
        self.errors.sort_by_key(|e| e.position);
    }

    #[inline]
    fn error(&mut self, pos: usize, kind: ErrorKind) {
        self.errors.push(ParseError::new(pos, kind));
    }

    /// Pop the current node: it ends at `start..end` (see [`Sink::end`]).
    #[inline]
    fn pop(&mut self, start: usize, end: usize, implied: bool) {
        if let Some(open) = self.open.pop() {
            self.sink.end(open, start, end, implied);
        }
    }

    fn start_tag(
        &mut self,
        name: &'a str,
        attrs: &[(&'a str, AttrValue<'a>)],
        self_closing: bool,
        pos: usize,
        end: usize,
    ) {
        let lname = lower(name);

        // Implicit close: pop open elements that this start tag closes.
        // They end where this tag starts.
        while self
            .open
            .top()
            .is_some_and(|top| tags::implicitly_closes(&lname, self.sink.name(top)))
        {
            self.pop(pos, pos, true);
            self.error(pos, ErrorKind::ImplicitlyClosed);
        }

        for &(k, v) in attrs {
            let value = v.as_str();
            if let Some(at) = self.sink.attr(k, value) {
                // At the `&`; the tag's start if the value can't be located.
                let at = self.source_offset(value).map_or(pos, |offset| offset + at);
                self.error(at, ErrorKind::UnknownEntity);
            }
        }
        let void = self_closing || tags::is_void(&lname);
        let open = self
            .sink
            .start(self.open.top(), lname, attrs.len(), pos, end);
        if void {
            self.sink.end(open, end, end, true);
        } else {
            self.open.push(open);
        }
    }

    fn end_tag(&mut self, name: &'a str, pos: usize, end: usize) {
        let lname = lower(name);
        if tags::is_void(&lname) {
            return; // void elements never have end tags
        }
        match self.open.find(&self.sink, &lname) {
            Some(idx) => {
                if idx + 1 != self.open.len() {
                    // Closing elements whose end tag is optional (`</ul>` over
                    // an open `<li>`) is valid HTML; anything else is
                    // mis-nesting.
                    let kind = if self.open.only_optional_above(&self.sink, idx) {
                        ErrorKind::ImplicitlyClosed
                    } else {
                        ErrorKind::MismatchedEndTag
                    };
                    self.error(pos, kind);
                }
                // Mis-nested elements above the match end where this end tag
                // starts; the match itself ends after it.
                while self.open.len() > idx + 1 {
                    self.pop(pos, pos, true);
                }
                self.pop(pos, end, false);
            }
            None => self.error(pos, ErrorKind::StrayEndTag),
        }
    }

    fn text(&mut self, t: &'a str, pos: usize, end: usize) {
        if t.is_empty() {
            return;
        }
        let (raw, closes) = {
            let top = self.open.top().map_or("", |o| self.sink.name(o));
            (tags::is_raw_text(top), tags::text_closes(top))
        };
        // Real text means an omitted `</head>` or `</colgroup>` (neither is raw
        // text, nor is what they sit in).
        if closes && t.bytes().any(|b| !b.is_ascii_whitespace()) {
            self.pop(pos, pos, true);
            self.error(pos, ErrorKind::ImplicitlyClosed);
        }
        // A text token's string starts at the token.
        if let Some(at) = self.sink.text(self.open.top(), t, !raw, pos, end) {
            self.error(pos + at, ErrorKind::UnknownEntity);
        }
    }
}

/// The [`Sink`] that builds a [`Dom`].
pub(crate) struct DomSink {
    pub(crate) dom: Dom,
    /// Source ranges are two `u32`s. Every offset is at most the input's
    /// length, so one check covers them all; an input too big for them gets
    /// no ranges (and `set_end` is then a no-op, so the casts never matter).
    fits: bool,
}

impl DomSink {
    pub(crate) fn new(src_len: usize) -> Self {
        DomSink {
            dom: Dom::with_capacity(src_len),
            fits: src_len < u32::MAX as usize,
        }
    }

    #[inline]
    fn range(&self, start: usize, end: usize) -> SrcRange {
        if self.fits {
            SrcRange::new(start as u32, end as u32)
        } else {
            SrcRange::NONE
        }
    }

    #[inline]
    fn leaf(&mut self, parent: Option<&(NodeId, Span)>, raw: RawKind, start: usize, end: usize) {
        let id = self.dom.alloc(raw, self.range(start, end));
        self.dom.append(parent.map(|&(p, _)| p), id);
    }
}

impl<'a> Sink<'a> for DomSink {
    /// The element and its interned name (saves an arena lookup per check).
    type Open = (NodeId, Span);

    #[inline]
    fn name<'s>(&'s self, open: &'s (NodeId, Span)) -> &'s str {
        self.dom.span_str(open.1)
    }

    #[inline]
    fn attr(&mut self, name: &'a str, value: &'a str) -> Option<usize> {
        let key = self.dom.intern_lower(name);
        let (value, unknown) = self.dom.intern_decoded(value, true);
        self.dom.attrs.push((key, value));
        unknown
    }

    #[inline]
    fn start(
        &mut self,
        parent: Option<&(NodeId, Span)>,
        name: Cow<'a, str>,
        attrs: usize,
        start: usize,
        end: usize,
    ) -> (NodeId, Span) {
        let name = self.dom.intern(&name);
        // The attributes are the last `attrs` pairs of the arena.
        let first = self.dom.attrs.len().saturating_sub(attrs);
        let attrs = AttrRange {
            start: first as u32,
            len: (self.dom.attrs.len() - first) as u32,
        };
        // Provisional range (just the start tag); `end` moves it when the
        // element is closed.
        let id = self
            .dom
            .alloc(RawKind::Element { name, attrs }, self.range(start, end));
        self.dom.append(parent.map(|&(p, _)| p), id);
        (id, name)
    }

    #[inline]
    fn end(&mut self, (id, _): (NodeId, Span), _start: usize, end: usize, _implied: bool) {
        if let Some(n) = self.dom.node_mut(id) {
            n.src.set_end(end as u32);
        }
    }

    #[inline]
    fn text(
        &mut self,
        parent: Option<&(NodeId, Span)>,
        text: &'a str,
        decode: bool,
        start: usize,
        end: usize,
    ) -> Option<usize> {
        let (span, unknown) = if decode {
            self.dom.intern_decoded(text, false)
        } else {
            (self.dom.intern(text), None)
        };
        self.leaf(parent, RawKind::Text(span), start, end);
        unknown
    }

    fn comment(
        &mut self,
        parent: Option<&(NodeId, Span)>,
        text: &'a str,
        start: usize,
        end: usize,
    ) {
        let span = self.dom.intern(text);
        self.leaf(parent, RawKind::Comment(span), start, end);
    }

    fn doctype(
        &mut self,
        parent: Option<&(NodeId, Span)>,
        text: &'a str,
        start: usize,
        end: usize,
    ) {
        let span = self.dom.intern(text);
        self.leaf(parent, RawKind::Doctype(span), start, end);
    }
}

/// How many of the innermost open elements an end tag searches for the one it
/// closes (see the module docs).
const MAX_END_TAG_SEARCH: usize = 256;

/// How many of the outermost open elements an end tag searches next (see the
/// module docs).
const OUTER_END_TAG_SEARCH: usize = 8;

/// The open-elements stack.
///
/// Finding the element an end tag closes scans down from the top. A match pops
/// everything it scanned past, so those scans are paid for by the pushes; an
/// end tag with *no* open match pops nothing, so on a deep stack
/// (`<div>`×N then `</x>`×N) unbounded scans would be quadratic. The scan is
/// therefore bounded: [`MAX_END_TAG_SEARCH`] elements from the top, then the
/// bottom [`OUTER_END_TAG_SEARCH`] — O(1) per end tag, whatever the names.
struct OpenElements<O> {
    /// Open elements, innermost last; their names come from the sink.
    stack: Vec<O>,
}

impl<O> Default for OpenElements<O> {
    fn default() -> Self {
        OpenElements { stack: Vec::new() }
    }
}

impl<O> OpenElements<O> {
    #[inline]
    fn top(&self) -> Option<&O> {
        self.stack.last()
    }

    #[inline]
    fn len(&self) -> usize {
        self.stack.len()
    }

    #[inline]
    fn push(&mut self, open: O) {
        self.stack.push(open);
    }

    #[inline]
    fn pop(&mut self) -> Option<O> {
        self.stack.pop()
    }

    /// Whether every element above stack index `idx` has an optional end tag.
    /// Only asked about a match, whose end tag pops all of them: the scan is
    /// paid for by their pushes.
    fn only_optional_above<'a, S: Sink<'a, Open = O>>(&self, sink: &S, idx: usize) -> bool {
        self.stack
            .get(idx + 1..)
            .unwrap_or(&[])
            .iter()
            .all(|open| tags::end_tag_optional(sink.name(open)))
    }

    /// The stack index of the open element named `name` that an end tag
    /// closes: the innermost among the [`MAX_END_TAG_SEARCH`] innermost, else
    /// the innermost among the [`OUTER_END_TAG_SEARCH`] outermost. `None`
    /// makes the end tag stray.
    fn find<'a, S: Sink<'a, Open = O>>(&self, sink: &S, name: &str) -> Option<usize> {
        let named = |open: &O| sink.name(open) == name;
        let inner = self.stack.len().saturating_sub(MAX_END_TAG_SEARCH);
        if let Some(i) = self
            .stack
            .get(inner..)
            .and_then(|s| s.iter().rposition(named))
        {
            return Some(inner + i);
        }
        let outer = inner.min(OUTER_END_TAG_SEARCH);
        self.stack.get(..outer)?.iter().rposition(named)
    }
}

/// Lower-case `name`, but borrow it unchanged when it's already lower-case
/// (the common case) — avoiding an allocation per tag/attribute.
pub(crate) fn lower(name: &str) -> Cow<'_, str> {
    if name.bytes().any(|b| b.is_ascii_uppercase()) {
        Cow::Owned(name.to_ascii_lowercase())
    } else {
        Cow::Borrowed(name)
    }
}
