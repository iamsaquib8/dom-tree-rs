//! The tree builder: applies the recovery rules to a token stream.
//!
//! Parsing is infallible. The builder keeps an "open elements" stack and
//! recovers from the common real-world mistakes — omitted end tags, mis-nested
//! tags, stray end tags — recording each as a [`ParseError`] rather than
//! bailing out. See the crate docs for the exact recovery rules we implement.
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
use alloc::string::String;
use alloc::vec::Vec;
use core::marker::PhantomData;

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
    let mut builder = TreeBuilder::new(DomSink::new(src.len()));
    while let Some((tok, pos)) = tk.next_token() {
        let end = tk.offset();
        builder.token(tok, tk.attrs(), pos, end);
    }
    builder.finish(src.len(), &mut tk.errors);
    let mut dom = builder.sink.dom;
    dom.errors = builder.errors;
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
    /// not yet entity-decoded. Returns whether decoding met an unknown named
    /// reference.
    fn attr(&mut self, name: &'a str, value: &'a str) -> bool;

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

    /// Character data, entity-decoded if `decode`. Returns whether decoding
    /// met an unknown named reference.
    fn text(
        &mut self,
        parent: Option<&Self::Open>,
        text: &'a str,
        decode: bool,
        start: usize,
        end: usize,
    ) -> bool;

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
    _src: PhantomData<&'a str>,
}

impl<'a, S: Sink<'a>> TreeBuilder<'a, S> {
    pub(crate) fn new(sink: S) -> Self {
        TreeBuilder {
            sink,
            open: OpenElements::default(),
            errors: Vec::new(),
            _src: PhantomData,
        }
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
        if let Some(open) = self.open.pop(&self.sink) {
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
            if self.sink.attr(k, v.as_str()) {
                self.error(pos, ErrorKind::UnknownEntity);
            }
        }
        let void = self_closing || tags::is_void(&lname);
        let open = self
            .sink
            .start(self.open.top(), lname, attrs.len(), pos, end);
        if void {
            self.sink.end(open, end, end, true);
        } else {
            self.open.push(&self.sink, open);
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
                    self.error(pos, ErrorKind::MismatchedEndTag);
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
        if self.sink.text(self.open.top(), t, !raw, pos, end) {
            self.error(pos, ErrorKind::UnknownEntity);
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
    fn attr(&mut self, name: &'a str, value: &'a str) -> bool {
        let key = self.dom.intern_lower(name);
        let (value, unknown) = self.dom.intern_decoded(value);
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
    ) -> bool {
        let (span, unknown) = if decode {
            self.dom.intern_decoded(text)
        } else {
            (self.dom.intern(text), false)
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

/// Above this many open elements the stack starts counting them per name (and
/// stops again below half of it).
const DEEP: usize = 64;

/// The open-elements stack.
///
/// Finding the element an end tag closes scans down from the top. A match pops
/// everything it scanned past, so those scans are paid for by the pushes; an
/// end tag with *no* open match pops nothing, so on a deep stack
/// (`<div>`×N then `</x>`×N) the scans would be quadratic. Once the stack is
/// deeper than [`DEEP`] it therefore keeps an exact count of open elements per
/// tag name and rejects such end tags in O(1). Shallow stacks — nearly every
/// real document — skip the bookkeeping, since their scans are short anyway.
struct OpenElements<O> {
    /// Open elements, innermost last; their names come from the sink.
    stack: Vec<O>,
    counts: Option<NameCounts>,
}

impl<O> Default for OpenElements<O> {
    fn default() -> Self {
        OpenElements {
            stack: Vec::new(),
            counts: None,
        }
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
    fn push<'a, S: Sink<'a, Open = O>>(&mut self, sink: &S, open: O) {
        self.stack.push(open);
        if self.counts.is_some() || self.stack.len() > DEEP {
            self.count_push(sink);
        }
    }

    #[inline]
    fn pop<'a, S: Sink<'a, Open = O>>(&mut self, sink: &S) -> Option<O> {
        let open = self.stack.pop()?;
        if self.counts.is_some() {
            self.count_pop(sink, &open);
        }
        Some(open)
    }

    /// The deep-stack bookkeeping behind `push` (the new element is already
    /// on the stack): start counting, or count.
    #[cold]
    fn count_push<'a, S: Sink<'a, Open = O>>(&mut self, sink: &S) {
        match &mut self.counts {
            Some(counts) => {
                if let Some(top) = self.stack.last() {
                    counts.up(sink.name(top));
                }
            }
            None => {
                let mut counts = NameCounts::default();
                for open in &self.stack {
                    counts.up(sink.name(open));
                }
                self.counts = Some(counts);
            }
        }
    }

    /// The deep-stack bookkeeping behind `pop`: uncount, or stop counting.
    #[cold]
    fn count_pop<'a, S: Sink<'a, Open = O>>(&mut self, sink: &S, popped: &O) {
        if self.stack.len() < DEEP / 2 {
            self.counts = None;
        } else if let Some(counts) = &mut self.counts {
            counts.down(sink.name(popped));
        }
    }

    /// The stack index of the innermost open element named `name`.
    fn find<'a, S: Sink<'a, Open = O>>(&self, sink: &S, name: &str) -> Option<usize> {
        if self.counts.as_ref().is_some_and(|c| !c.contains(name)) {
            return None;
        }
        self.stack.iter().rposition(|open| sink.name(open) == name)
    }
}

/// Open-element counts per tag name: a minimal open-addressing hash table
/// (FNV-1a, linear probing, at most half full). Hand-rolled rather than a
/// `BTreeMap` because the generic tree code alone cost ~3 KB of wasm. Names
/// are never removed — a count just drops to zero — so no tombstones needed.
/// It keeps its own copy of each name (only while deep, so off the hot path):
/// a sink's names need not outlive their elements.
#[derive(Default)]
struct NameCounts {
    /// `(hash, name in keys, count)`; `None` is an empty slot. Length is 0 or
    /// a power of two.
    slots: Vec<Option<(u32, Span, u32)>>,
    keys: String,
    used: usize,
}

impl NameCounts {
    fn key(&self, span: Span) -> &str {
        self.keys.get(span.range()).unwrap_or("")
    }

    /// The slot holding `name`, or the empty slot where it would go.
    fn slot(&self, hash: u32, name: &str) -> usize {
        let mask = self.slots.len().wrapping_sub(1);
        let mut i = hash as usize & mask;
        // Terminates: the table is never more than half full.
        while let Some(Some((h, span, _))) = self.slots.get(i) {
            if *h == hash && self.key(*span) == name {
                break;
            }
            i = (i + 1) & mask;
        }
        i
    }

    fn up(&mut self, name: &str) {
        if (self.used + 1) * 2 > self.slots.len() {
            self.grow();
        }
        let hash = fnv1a(name);
        let i = self.slot(hash, name);
        let new_key = Span {
            start: self.keys.len() as u32,
            len: name.len() as u32,
        };
        match self.slots.get_mut(i) {
            Some(Some((_, _, count))) => *count = count.saturating_add(1),
            Some(empty) => {
                *empty = Some((hash, new_key, 1));
                self.keys.push_str(name);
                self.used += 1;
            }
            None => {}
        }
    }

    fn down(&mut self, name: &str) {
        let i = self.slot(fnv1a(name), name);
        if let Some(Some((_, _, count))) = self.slots.get_mut(i) {
            *count = count.saturating_sub(1);
        }
    }

    fn contains(&self, name: &str) -> bool {
        let i = self.slot(fnv1a(name), name);
        matches!(self.slots.get(i), Some(Some((_, _, count))) if *count > 0)
    }

    fn grow(&mut self) {
        let cap = (self.slots.len() * 2).max(16);
        let old = core::mem::replace(&mut self.slots, alloc::vec![None; cap]);
        for (hash, span, count) in old.into_iter().flatten() {
            let i = self.slot(hash, self.key(span));
            if let Some(slot) = self.slots.get_mut(i) {
                *slot = Some((hash, span, count));
            }
        }
    }
}

fn fnv1a(s: &str) -> u32 {
    s.bytes().fold(0x811c_9dc5, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    })
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
