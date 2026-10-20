//! The tree builder: turns a token stream into the [`Dom`] arena.
//!
//! Parsing is infallible. The builder keeps an "open elements" stack and
//! recovers from the common real-world mistakes — omitted end tags, mis-nested
//! tags, stray end tags — recording each as a [`ParseError`] rather than
//! bailing out. See the crate docs for the exact recovery rules we implement.
//!
//! It also records each node's source range (see
//! [`NodeRef::source_range`](crate::NodeRef::source_range)): leaf nodes get
//! their token's range, and an element's end is filled in when it is popped
//! off the open-element stack.

use alloc::borrow::Cow;
use alloc::vec::Vec;

use crate::entities;
use crate::error::{ErrorKind, ParseError};
use crate::node::{NodeId, RawKind, Span, SrcRange};
use crate::tags;
use crate::token::Token;
use crate::tokenizer::Tokenizer;
use crate::tree::Dom;

/// Parse `src` into a best-effort [`Dom`]. Never panics; diagnostics land in
/// [`Dom::errors`].
pub(crate) fn parse_document(src: &str) -> Dom {
    let mut tk = Tokenizer::new(src);
    let mut dom = Dom::with_capacity(src.len());
    let mut open = OpenElements::default();
    // Source ranges are two `u32`s. Every offset is at most `src.len()`, so one
    // check here covers them all; an input too big for them gets no ranges
    // (and `set_end` is then a no-op, so the casts below never matter).
    let fits = src.len() < u32::MAX as usize;
    let range = |start: usize, end: usize| {
        if fits {
            SrcRange::new(start as u32, end as u32)
        } else {
            SrcRange::NONE
        }
    };

    while let Some((tok, pos)) = tk.next_token() {
        let end = tk.offset();
        match tok {
            Token::StartTag {
                name,
                attrs,
                self_closing,
            } => {
                let lname = lower(name);

                // Implicit close: pop open elements that this start tag closes.
                // They end where this tag starts.
                while open.top().is_some() && tags::implicitly_closes(&lname, open.top_name(&dom)) {
                    open.pop(&mut dom, pos);
                    dom.errors
                        .push(ParseError::new(pos, ErrorKind::ImplicitlyClosed));
                }

                let parent = open.top();

                let name_span = dom.intern(&lname);
                let mut built: Vec<(Span, Span)> = Vec::with_capacity(attrs.len());
                for (k, v) in attrs {
                    let key_span = dom.intern(&lower(k));
                    let (decoded, unknown) = entities::decode(v.as_str());
                    if unknown {
                        dom.errors
                            .push(ParseError::new(pos, ErrorKind::UnknownEntity));
                    }
                    let val_span = dom.intern(&decoded);
                    built.push((key_span, val_span));
                }

                // Provisional range (just the start tag); a non-void element's
                // end is moved when it is closed.
                let id = dom.alloc(
                    RawKind::Element {
                        name: name_span,
                        attrs: built,
                    },
                    range(pos, end),
                );
                dom.append(parent, id);

                if !self_closing && !tags::is_void(&lname) {
                    open.push(&dom, id, name_span);
                }
            }

            Token::EndTag { name } => {
                let lname = lower(name);
                if tags::is_void(&lname) {
                    continue; // void elements never have end tags
                }
                match open.find(&dom, &lname) {
                    Some(idx) => {
                        if idx + 1 != open.len() {
                            dom.errors
                                .push(ParseError::new(pos, ErrorKind::MismatchedEndTag));
                        }
                        // Mis-nested elements above the match end where this
                        // end tag starts; the match itself ends after it.
                        while open.len() > idx + 1 {
                            open.pop(&mut dom, pos);
                        }
                        open.pop(&mut dom, end);
                    }
                    None => dom
                        .errors
                        .push(ParseError::new(pos, ErrorKind::StrayEndTag)),
                }
            }

            Token::Text(t) => {
                if t.is_empty() {
                    continue;
                }
                let (raw, closes) = {
                    let top = open.top_name(&dom);
                    (tags::is_raw_text(top), tags::text_closes(top))
                };
                // Real text means an omitted `</head>` or `</colgroup>` (neither
                // is raw text, nor is what they sit in).
                if closes && t.bytes().any(|b| !b.is_ascii_whitespace()) {
                    open.pop(&mut dom, pos);
                    dom.errors
                        .push(ParseError::new(pos, ErrorKind::ImplicitlyClosed));
                }
                let parent = open.top();
                let span = if raw {
                    dom.intern(t)
                } else {
                    let (decoded, unknown) = entities::decode(t);
                    if unknown {
                        dom.errors
                            .push(ParseError::new(pos, ErrorKind::UnknownEntity));
                    }
                    dom.intern(&decoded)
                };
                let id = dom.alloc(RawKind::Text(span), range(pos, end));
                dom.append(parent, id);
            }

            Token::Cdata(c) => {
                if c.is_empty() {
                    continue;
                }
                let parent = open.top();
                let span = dom.intern(c);
                let id = dom.alloc(RawKind::Text(span), range(pos, end));
                dom.append(parent, id);
            }

            Token::Comment(c) => {
                let parent = open.top();
                let span = dom.intern(c);
                let id = dom.alloc(RawKind::Comment(span), range(pos, end));
                dom.append(parent, id);
            }

            Token::Doctype(d) => {
                let parent = open.top();
                let span = dom.intern(d.trim());
                let id = dom.alloc(RawKind::Doctype(span), range(pos, end));
                dom.append(parent, id);
            }
        }
    }

    if !open.stack.is_empty() {
        dom.errors
            .push(ParseError::new(src.len(), ErrorKind::ImplicitlyClosed));
        // Everything still open runs to the end of the input.
        for &(id, _) in &open.stack {
            if let Some(n) = dom.node_mut(id) {
                n.src.set_end(src.len() as u32);
            }
        }
    }

    // Fold in the tokenizer's lexical diagnostics, then order by position.
    dom.errors.append(&mut tk.errors);
    dom.errors.sort_by_key(|e| e.position);
    dom
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
/// tag name and rejects such end tags in O(log n). Shallow stacks — nearly every
/// real document — skip the bookkeeping, since their scans are short anyway.
#[derive(Default)]
struct OpenElements {
    /// Open elements with their interned (lower-cased) tag names, innermost
    /// last. Keeping the name here saves an arena lookup per check.
    stack: Vec<(NodeId, Span)>,
    counts: Option<NameCounts>,
}

impl OpenElements {
    #[inline]
    fn top(&self) -> Option<NodeId> {
        self.stack.last().map(|&(id, _)| id)
    }

    /// The current node's tag name (`""` when nothing is open).
    #[inline]
    fn top_name<'d>(&self, dom: &'d Dom) -> &'d str {
        self.stack
            .last()
            .map_or("", |&(_, name)| dom.span_str(name))
    }

    #[inline]
    fn len(&self) -> usize {
        self.stack.len()
    }

    #[inline]
    fn push(&mut self, dom: &Dom, id: NodeId, name: Span) {
        self.stack.push((id, name));
        if self.counts.is_some() || self.stack.len() > DEEP {
            self.count_push(dom, name);
        }
    }

    /// Pop the current node, recording that its source range ends at `at`.
    #[inline]
    fn pop(&mut self, dom: &mut Dom, at: usize) {
        let Some((id, name)) = self.stack.pop() else {
            return;
        };
        if let Some(n) = dom.node_mut(id) {
            n.src.set_end(at as u32);
        }
        if self.counts.is_some() {
            self.count_pop(dom, name);
        }
    }

    /// The deep-stack bookkeeping behind `push`: start counting, or count.
    #[cold]
    fn count_push(&mut self, dom: &Dom, name: Span) {
        match &mut self.counts {
            Some(counts) => counts.up(dom, name),
            None => {
                let mut counts = NameCounts::default();
                for &(_, open) in &self.stack {
                    counts.up(dom, open);
                }
                self.counts = Some(counts);
            }
        }
    }

    /// The deep-stack bookkeeping behind `pop`: uncount, or stop counting.
    #[cold]
    fn count_pop(&mut self, dom: &Dom, name: Span) {
        if self.stack.len() < DEEP / 2 {
            self.counts = None;
        } else if let Some(counts) = &mut self.counts {
            counts.down(dom, dom.span_str(name));
        }
    }

    /// The stack index of the innermost open element named `name`.
    fn find(&self, dom: &Dom, name: &str) -> Option<usize> {
        if self.counts.as_ref().is_some_and(|c| !c.contains(dom, name)) {
            return None;
        }
        self.stack
            .iter()
            .rposition(|&(_, open)| dom.span_str(open) == name)
    }
}

/// Open-element counts per tag name: a minimal open-addressing hash table
/// (FNV-1a, linear probing, at most half full). Hand-rolled rather than a
/// `BTreeMap` because the generic tree code alone cost ~3 KB of wasm. Names
/// are never removed — a count just drops to zero — so no tombstones needed.
#[derive(Default)]
struct NameCounts {
    /// `(hash, name, count)`; `None` is an empty slot. Length is 0 or a power
    /// of two.
    slots: Vec<Option<(u32, Span, u32)>>,
    used: usize,
}

impl NameCounts {
    /// The slot holding `name`, or the empty slot where it would go.
    fn slot(&self, dom: &Dom, hash: u32, name: &str) -> usize {
        let mask = self.slots.len().wrapping_sub(1);
        let mut i = hash as usize & mask;
        // Terminates: the table is never more than half full.
        while let Some(Some((h, span, _))) = self.slots.get(i) {
            if *h == hash && dom.span_str(*span) == name {
                break;
            }
            i = (i + 1) & mask;
        }
        i
    }

    fn up(&mut self, dom: &Dom, name: Span) {
        if (self.used + 1) * 2 > self.slots.len() {
            self.grow(dom);
        }
        let s = dom.span_str(name);
        let hash = fnv1a(s);
        let i = self.slot(dom, hash, s);
        match self.slots.get_mut(i) {
            Some(Some((_, _, count))) => *count = count.saturating_add(1),
            Some(empty) => {
                *empty = Some((hash, name, 1));
                self.used += 1;
            }
            None => {}
        }
    }

    fn down(&mut self, dom: &Dom, name: &str) {
        let i = self.slot(dom, fnv1a(name), name);
        if let Some(Some((_, _, count))) = self.slots.get_mut(i) {
            *count = count.saturating_sub(1);
        }
    }

    fn contains(&self, dom: &Dom, name: &str) -> bool {
        let i = self.slot(dom, fnv1a(name), name);
        matches!(self.slots.get(i), Some(Some((_, _, count))) if *count > 0)
    }

    fn grow(&mut self, dom: &Dom) {
        let cap = (self.slots.len() * 2).max(16);
        let old = core::mem::replace(&mut self.slots, alloc::vec![None; cap]);
        for (hash, span, count) in old.into_iter().flatten() {
            let i = self.slot(dom, hash, dom.span_str(span));
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
fn lower(name: &str) -> Cow<'_, str> {
    if name.bytes().any(|b| b.is_ascii_uppercase()) {
        Cow::Owned(name.to_ascii_lowercase())
    } else {
        Cow::Borrowed(name)
    }
}
