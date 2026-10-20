//! Node types stored in the [`Dom`](crate::Dom) arena.
//!
//! Strings (tag names, attribute names/values, text) are **not** stored inline
//! on each node. They live in a single growable buffer on the [`Dom`](crate::Dom)
//! and nodes reference them by [`Span`] (a byte range). This replaces thousands
//! of tiny per-node allocations with appends to one buffer — the main reason
//! parsing is fast despite producing an owned tree.

use alloc::vec::Vec;
use core::ops::Range;

/// A handle to a node inside a [`Dom`](crate::Dom). Cheap (`Copy`) and stable
/// for the lifetime of the tree.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NodeId(pub(crate) u32);

impl NodeId {
    /// The node's position in the arena.
    #[inline]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A byte range into the [`Dom`](crate::Dom)'s string arena.
#[derive(Clone, Copy)]
pub(crate) struct Span {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

impl Span {
    pub(crate) const EMPTY: Span = Span { start: 0, len: 0 };

    #[inline]
    pub(crate) fn range(self) -> Range<usize> {
        let start = self.start as usize;
        start..start + self.len as usize
    }
}

/// What a node is — a borrowed, `Copy` view returned by
/// [`NodeRef::kind`](crate::NodeRef::kind).
///
/// For elements, the tag name is included here; read attributes via the
/// [`NodeRef`](crate::NodeRef) accessors (`attr`, `attributes`, `classes`, …).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind<'a> {
    /// An element, with its lower-cased tag name.
    Element {
        /// The lower-cased tag name (e.g. `"div"`).
        tag: &'a str,
    },
    /// A run of text (already entity-decoded, except inside `<script>`/`<style>`).
    Text(&'a str),
    /// The body of an HTML comment.
    Comment(&'a str),
    /// The body of a `<!doctype …>` declaration.
    Doctype(&'a str),
}

/// The internal, span-based node kind (not exposed; see [`NodeKind`]).
#[derive(Clone)]
pub(crate) enum RawKind {
    Element {
        name: Span,
        attrs: Vec<(Span, Span)>,
    },
    Text(Span),
    Comment(Span),
    Doctype(Span),
}

/// The internal arena record: tree links plus the node's kind.
#[derive(Clone)]
pub(crate) struct Node {
    pub(crate) parent: Option<NodeId>,
    pub(crate) first_child: Option<NodeId>,
    pub(crate) last_child: Option<NodeId>,
    pub(crate) next_sibling: Option<NodeId>,
    pub(crate) prev_sibling: Option<NodeId>,
    /// Set once the node (or an ancestor) is removed with
    /// [`Dom::remove`](crate::Dom::remove): it stays in the arena but is no
    /// longer part of the document.
    pub(crate) detached: bool,
    pub(crate) raw: RawKind,
}

impl Node {
    pub(crate) fn new(raw: RawKind) -> Self {
        Node {
            parent: None,
            first_child: None,
            last_child: None,
            next_sibling: None,
            prev_sibling: None,
            detached: false,
            raw,
        }
    }
}
