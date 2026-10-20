//! Node types stored in the [`Dom`](crate::Dom) arena.
//!
//! Strings (tag names, attribute names/values, text) are **not** stored inline
//! on each node. They live in a single growable buffer on the [`Dom`](crate::Dom)
//! and nodes reference them by [`Span`] (a byte range). This replaces thousands
//! of tiny per-node allocations with appends to one buffer — the main reason
//! parsing is fast despite producing an owned tree.

use core::fmt;
use core::num::NonZeroU32;
use core::ops::Range;

/// A handle to a node inside a [`Dom`](crate::Dom). Cheap (`Copy`) and stable
/// for the lifetime of the tree.
//
// Stored as index + 1 so that `Option<NodeId>` is 4 bytes: every node holds
// five tree links, so this keeps a node at 52 bytes.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(NonZeroU32);

impl NodeId {
    /// The id of arena slot `index`. (Saturates past `u32::MAX - 1` nodes,
    /// which no arena that fits in memory reaches.)
    #[inline]
    pub(crate) fn from_index(index: usize) -> Self {
        let index = u32::try_from(index).unwrap_or(u32::MAX);
        NodeId(NonZeroU32::MIN.saturating_add(index))
    }

    /// The node's position in the arena.
    #[inline]
    pub fn index(self) -> usize {
        (self.0.get() - 1) as usize
    }
}

impl fmt::Debug for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("NodeId").field(&self.index()).finish()
    }
}

/// A byte range into the [`Dom`](crate::Dom)'s string arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// An element's attributes: a run of `len` consecutive `(name, value)` pairs
/// starting at `start` in the [`Dom`](crate::Dom)'s attribute arena.
///
/// Storing every element's attributes in one shared `Vec` (instead of a `Vec`
/// per element) saves a heap allocation per element with attributes and
/// shrinks a node from 64 to 52 bytes. The parser lays the runs out in
/// document order; editing may move an element's run to the end of the arena
/// (see `Dom::set_attr`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AttrRange {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

impl AttrRange {
    #[inline]
    pub(crate) fn range(self) -> Range<usize> {
        let start = self.start as usize;
        start..start + self.len as usize
    }
}

/// The internal, span-based node kind (not exposed; see [`NodeKind`]).
#[derive(Clone, Copy)]
pub(crate) enum RawKind {
    Element { name: Span, attrs: AttrRange },
    Text(Span),
    Comment(Span),
    Doctype(Span),
}

/// Where a node came from: a byte range into the *original source* (not the
/// string arena), or [`SrcRange::NONE`] for nodes created by editing.
///
/// Two `u32`s keep the per-node overhead at 8 bytes. The builder records
/// ranges only when the whole input fits in them (under 4 GiB); for a larger
/// input every node is `NONE`, never a wrapped offset.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct SrcRange {
    start: u32,
    end: u32,
}

impl SrcRange {
    /// No range. `start == u32::MAX` marks it: real offsets are below that.
    pub(crate) const NONE: SrcRange = SrcRange {
        start: u32::MAX,
        end: u32::MAX,
    };

    /// A range the caller knows is valid (`start <= end < u32::MAX`).
    #[inline]
    pub(crate) fn new(start: u32, end: u32) -> Self {
        SrcRange { start, end }
    }

    /// Move the end (an element's range grows when it is closed). A no-op on
    /// [`SrcRange::NONE`].
    #[inline]
    pub(crate) fn set_end(&mut self, end: u32) {
        if self.start != u32::MAX {
            self.end = end;
        }
    }

    pub(crate) fn get(self) -> Option<Range<usize>> {
        (self.start != u32::MAX).then_some(self.start as usize..self.end.max(self.start) as usize)
    }
}

/// The internal arena record: tree links plus the node's kind.
#[derive(Clone)]
pub(crate) struct Node {
    pub(crate) parent: Option<NodeId>,
    pub(crate) first_child: Option<NodeId>,
    pub(crate) last_child: Option<NodeId>,
    pub(crate) next_sibling: Option<NodeId>,
    pub(crate) prev_sibling: Option<NodeId>,
    /// Exactly when the node is **not** reachable from the top level: it was
    /// removed with [`Dom::remove`](crate::Dom::remove) (or is inside a
    /// removed subtree), or was created by editing and not yet attached. A
    /// subtree is always uniformly flagged. It stays in the arena either way.
    pub(crate) detached: bool,
    pub(crate) src: SrcRange,
    pub(crate) raw: RawKind,
}

impl Node {
    pub(crate) fn new(raw: RawKind, src: SrcRange) -> Self {
        Node {
            parent: None,
            first_child: None,
            last_child: None,
            next_sibling: None,
            prev_sibling: None,
            detached: false,
            src,
            raw,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nodes_stay_small() {
        // Parse speed is mostly memory traffic: keep nodes small. Five links
        // (20 bytes), the source range (8), the kind with its two spans (20)
        // and the detached flag.
        assert_eq!(core::mem::size_of::<Option<NodeId>>(), 4);
        assert_eq!(core::mem::size_of::<RawKind>(), 20);
        assert!(core::mem::size_of::<Node>() <= 52);
    }

    #[test]
    fn node_id_round_trips_its_index() {
        for i in [0, 1, 41, u32::MAX as usize - 1] {
            assert_eq!(NodeId::from_index(i).index(), i);
        }
        assert_eq!(alloc::format!("{:?}", NodeId::from_index(7)), "NodeId(7)");
    }
}
