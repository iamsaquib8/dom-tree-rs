//! Streaming parse events: the tree builder's recovery without the tree.
//!
//! [`events`](crate::events) runs the same tokenizer and the same tree
//! builder as [`parse`](crate::parse), but the builder drives an event queue
//! instead of the [`Dom`](crate::Dom) arena (see `src/builder.rs`), so the
//! two can never disagree: replaying the events into a tree gives exactly the
//! tree `parse` builds. Memory stays proportional to the nesting depth (plus
//! the events of one token and the collected diagnostics), not to the
//! document.

use alloc::borrow::Cow;
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::iter::FusedIterator;
use core::ops::Range;

use crate::builder::{Sink, TreeBuilder, lower};
use crate::entities;
use crate::error::ParseError;
use crate::tokenizer::Tokenizer;

/// One step of a document as the tree builder sees it, after recovery. See
/// [`events`](crate::events).
///
/// Every `Start` is matched by exactly one later `End` with the same name,
/// properly nested — including elements whose end tag was omitted or
/// misplaced, and void and self-closing elements, whose `End` follows
/// immediately.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event<'a> {
    /// An element starts.
    Start {
        /// The lower-cased tag name.
        name: Cow<'a, str>,
        /// The attributes in source order, duplicates kept: names
        /// lower-cased, values entity-decoded (`""` for a valueless one).
        attrs: Vec<(Cow<'a, str>, Cow<'a, str>)>,
    },
    /// An element ends.
    End {
        /// The lower-cased tag name (that of the matching `Start`).
        name: Cow<'a, str>,
        /// `true` when the element has no end tag of its own: recovery closed
        /// it (an implicitly closed `<p>` or `<li>`, an element left open
        /// inside a mismatched end tag, anything open at the end of the
        /// input), or it is a void or self-closing element.
        implied: bool,
    },
    /// Character data: entity-decoded, except inside `<script>`/`<style>`.
    /// CDATA sections are text too. Adjacent text can arrive in several
    /// pieces (a stray `<` is its own piece), exactly like the tree's text
    /// nodes.
    Text(Cow<'a, str>),
    /// The body of a comment (or of a bogus comment like `<?xml …>`).
    Comment(&'a str),
    /// The body of a `<!doctype …>`, trimmed.
    Doctype(&'a str),
}

/// The iterator returned by [`events`](crate::events). Yields each
/// [`Event`] with its byte range in the source.
///
/// The range is the event's markup: a start tag, an end tag, a text run (raw,
/// before decoding), a comment or doctype. An implied [`Event::End`] has an
/// empty range at the position where recovery closed the element — the start
/// of the token that closed it, the end of a void element's start tag, or the
/// end of the input. So an element spans from its `Start`'s range start to
/// its `End`'s range end, the same as
/// [`NodeRef::source_range`](crate::NodeRef::source_range).
pub struct Events<'a> {
    tokenizer: Tokenizer<'a>,
    builder: TreeBuilder<'a, Queue<'a>>,
    len: usize,
    done: bool,
}

impl<'a> Events<'a> {
    pub(crate) fn new(src: &'a str) -> Self {
        Events {
            tokenizer: Tokenizer::new(src),
            builder: TreeBuilder::new(
                Queue {
                    events: VecDeque::new(),
                    attrs: Vec::new(),
                },
                src,
            ),
            len: src.len(),
            done: false,
        }
    }

    /// The recovery diagnostics, the same as
    /// [`Dom::errors`](crate::Dom::errors) for the same input. Complete and
    /// sorted by position once the iterator has returned `None`; before that,
    /// those found so far.
    ///
    /// ```
    /// let mut events = domq::events("<p>a</b>");
    /// let n = events.by_ref().count();
    /// assert_eq!(n, 3); // Start p, Text, End p (implied, at the end of input)
    /// assert_eq!(events.errors(), domq::parse("<p>a</b>").errors());
    /// ```
    pub fn errors(&self) -> &[ParseError] {
        &self.builder.errors
    }
}

impl<'a> Iterator for Events<'a> {
    type Item = (Event<'a>, Range<usize>);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(event) = self.builder.sink.events.pop_front() {
                return Some(event);
            }
            if self.done {
                return None;
            }
            match self.tokenizer.next_token() {
                Some((tok, pos)) => {
                    let end = self.tokenizer.offset();
                    self.builder.token(tok, self.tokenizer.attrs(), pos, end);
                }
                None => {
                    self.builder.finish(self.len, &mut self.tokenizer.errors);
                    self.done = true;
                }
            }
        }
    }
}

impl FusedIterator for Events<'_> {}

/// The [`Sink`] behind [`Events`]: queues what the builder reports for one
/// token (an implicit close, then a start tag, say).
struct Queue<'a> {
    events: VecDeque<(Event<'a>, Range<usize>)>,
    /// The attributes of the start tag being reported.
    attrs: Vec<(Cow<'a, str>, Cow<'a, str>)>,
}

impl<'a> Sink<'a> for Queue<'a> {
    /// The open element's lower-cased name.
    type Open = Cow<'a, str>;

    fn name<'s>(&'s self, open: &'s Cow<'a, str>) -> &'s str {
        open
    }

    fn attr(&mut self, name: &'a str, value: &'a str) -> Option<usize> {
        let (value, unknown) = entities::decode(value, true);
        self.attrs.push((lower(name), value));
        unknown
    }

    fn start(
        &mut self,
        _parent: Option<&Cow<'a, str>>,
        name: Cow<'a, str>,
        _attrs: usize,
        start: usize,
        end: usize,
    ) -> Cow<'a, str> {
        let attrs = core::mem::take(&mut self.attrs);
        let event = Event::Start {
            name: name.clone(),
            attrs,
        };
        self.events.push_back((event, start..end));
        name
    }

    fn end(&mut self, name: Cow<'a, str>, start: usize, end: usize, implied: bool) {
        self.events
            .push_back((Event::End { name, implied }, start..end));
    }

    fn text(
        &mut self,
        _parent: Option<&Cow<'a, str>>,
        text: &'a str,
        decode: bool,
        start: usize,
        end: usize,
    ) -> Option<usize> {
        let (text, unknown) = if decode {
            entities::decode(text, false)
        } else {
            (Cow::Borrowed(text), None)
        };
        self.events.push_back((Event::Text(text), start..end));
        unknown
    }

    fn comment(&mut self, _parent: Option<&Cow<'a, str>>, text: &'a str, start: usize, end: usize) {
        self.events.push_back((Event::Comment(text), start..end));
    }

    fn doctype(&mut self, _parent: Option<&Cow<'a, str>>, text: &'a str, start: usize, end: usize) {
        self.events.push_back((Event::Doctype(text), start..end));
    }
}
