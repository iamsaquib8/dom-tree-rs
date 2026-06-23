//! Node types stored in the [`Dom`](crate::Dom) arena.

use alloc::boxed::Box;
use alloc::vec::Vec;

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

/// What a node *is*.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeKind {
    /// An element, e.g. `<div class="x">…</div>`.
    Element(Element),
    /// A run of text. Stored already entity-decoded (except inside
    /// `<script>`/`<style>`, whose contents are kept raw).
    Text(Box<str>),
    /// The body of an HTML comment.
    Comment(Box<str>),
    /// The body of a `<!doctype …>` declaration.
    Doctype(Box<str>),
}

/// An HTML element: a lower-cased tag name and its attributes in source order.
///
/// Attributes are stored in an insertion-ordered `Vec` rather than a `HashMap`:
/// real elements carry only a handful of attributes, so linear lookup is faster
/// in practice *and* dramatically smaller in a WASM build (no `HashMap`/SipHash
/// machinery), which is the whole point of this crate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element {
    pub(crate) name: Box<str>,
    pub(crate) attrs: Vec<(Box<str>, Box<str>)>,
}

impl Element {
    /// The tag name, lower-cased (e.g. `"div"`).
    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The value of attribute `name`, if present. First match wins.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k.as_ref() == name)
            .map(|(_, v)| v.as_ref())
    }

    /// Whether attribute `name` is present (including valueless boolean attrs).
    pub fn has_attr(&self, name: &str) -> bool {
        self.attrs.iter().any(|(k, _)| k.as_ref() == name)
    }

    /// All attributes as `(name, value)` pairs, in source order.
    pub fn attributes(&self) -> impl Iterator<Item = (&str, &str)> {
        self.attrs.iter().map(|(k, v)| (k.as_ref(), v.as_ref()))
    }

    /// The `id` attribute, if any.
    pub fn id(&self) -> Option<&str> {
        self.attr("id")
    }

    /// The `class` attribute split into individual class names.
    pub fn classes(&self) -> impl Iterator<Item = &str> {
        self.attr("class").into_iter().flat_map(str::split_whitespace)
    }

    /// Whether the element's `class` attribute contains `class`.
    pub fn has_class(&self, class: &str) -> bool {
        self.classes().any(|c| c == class)
    }
}

/// The internal arena record: tree links plus the node's kind.
#[derive(Clone, Debug)]
pub(crate) struct Node {
    pub(crate) parent: Option<NodeId>,
    pub(crate) first_child: Option<NodeId>,
    pub(crate) last_child: Option<NodeId>,
    pub(crate) next_sibling: Option<NodeId>,
    pub(crate) prev_sibling: Option<NodeId>,
    pub(crate) kind: NodeKind,
}

impl Node {
    pub(crate) fn new(kind: NodeKind) -> Self {
        Node {
            parent: None,
            first_child: None,
            last_child: None,
            next_sibling: None,
            prev_sibling: None,
            kind,
        }
    }
}
