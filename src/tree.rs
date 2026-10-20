//! The parsed document ([`Dom`]) and the read handle into it ([`NodeRef`]).

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use core::ops::Range;

use crate::error::ParseError;
use crate::node::{Node, NodeId, NodeKind, RawKind, Span, SrcRange};

/// A parsed HTML document: an arena of nodes, a single string buffer they
/// reference, the top-level (root) nodes, and any recovery diagnostics.
///
/// Build one with [`crate::parse`] and traverse it with [`NodeRef`] handles
/// from [`Dom::root`], [`Dom::roots`], [`Dom::get`], or the `find_by_*`
/// helpers. The editing methods ([`Dom::remove`], [`Dom::set_text`],
/// [`Dom::append_html`], [`Dom::wrap`], …) rewrite it in place.
#[derive(Clone)]
pub struct Dom {
    pub(crate) nodes: Vec<Node>,
    pub(crate) strings: String,
    /// The first and last top-level node. Top-level nodes are linked as
    /// siblings, like the children of an implicit document node.
    pub(crate) first_root: Option<NodeId>,
    pub(crate) last_root: Option<NodeId>,
    /// Set by any edit that can put a node out of arena order. While it is
    /// clear, arena order *is* document order (the parser only ever appends)
    /// and [`Dom::nodes`] is a plain scan of the arena.
    pub(crate) reordered: bool,
    pub(crate) errors: Vec<ParseError>,
}

impl Dom {
    pub(crate) fn with_capacity(src_len: usize) -> Self {
        Dom {
            nodes: Vec::with_capacity(src_len / 16 + 8),
            strings: String::with_capacity(src_len),
            first_root: None,
            last_root: None,
            reordered: false,
            errors: Vec::new(),
        }
    }

    /// Append a string to the arena and return its [`Span`].
    pub(crate) fn intern(&mut self, s: &str) -> Span {
        if s.is_empty() {
            return Span::EMPTY;
        }
        let start = self.strings.len() as u32;
        self.strings.push_str(s);
        Span {
            start,
            len: s.len() as u32,
        }
    }

    #[inline]
    pub(crate) fn span_str(&self, span: Span) -> &str {
        self.strings.get(span.range()).unwrap_or("")
    }

    pub(crate) fn alloc(&mut self, raw: RawKind, src: SrcRange) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(Node::new(raw, src));
        id
    }

    #[inline]
    pub(crate) fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.index())
    }

    #[inline]
    pub(crate) fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id.index())
    }

    /// Append `child` under `parent` (or as a root when `parent` is `None`),
    /// maintaining all sibling/child links.
    #[inline]
    pub(crate) fn append(&mut self, parent: Option<NodeId>, child: NodeId) {
        self.link(parent, None, child);
    }

    /// Link the unlinked node `id` into `parent` (`None`: the top level),
    /// before its child `before` (`None`: as the last child). Touches links
    /// only; callers keep `detached` and `reordered` right.
    pub(crate) fn link(&mut self, parent: Option<NodeId>, before: Option<NodeId>, id: NodeId) {
        let prev = match (before, parent) {
            (Some(b), _) => self.node(b).and_then(|n| n.prev_sibling),
            (None, Some(p)) => self.node(p).and_then(|n| n.last_child),
            (None, None) => self.last_root,
        };
        if let Some(n) = self.node_mut(id) {
            n.parent = parent;
            n.prev_sibling = prev;
            n.next_sibling = before;
        }
        match (prev, parent) {
            (Some(p), _) => {
                if let Some(p) = self.node_mut(p) {
                    p.next_sibling = Some(id);
                }
            }
            (None, Some(p)) => {
                if let Some(p) = self.node_mut(p) {
                    p.first_child = Some(id);
                }
            }
            (None, None) => self.first_root = Some(id),
        }
        match (before, parent) {
            (Some(b), _) => {
                if let Some(b) = self.node_mut(b) {
                    b.prev_sibling = Some(id);
                }
            }
            (None, Some(p)) => {
                if let Some(p) = self.node_mut(p) {
                    p.last_child = Some(id);
                }
            }
            (None, None) => self.last_root = Some(id),
        }
    }

    /// Unlink `id` from its parent and siblings (or from the top level). Its
    /// subtree stays intact; `detached` flags are left to the caller.
    pub(crate) fn unlink(&mut self, id: NodeId) {
        let Some(node) = self.node(id) else { return };
        let (parent, prev, next) = (node.parent, node.prev_sibling, node.next_sibling);
        // A parentless node is top-level only while attached; a detached one
        // (removed or created) is an orphan with no siblings.
        let top = parent.is_none() && !node.detached;
        match (prev, parent) {
            (Some(p), _) => {
                if let Some(p) = self.node_mut(p) {
                    p.next_sibling = next;
                }
            }
            (None, Some(p)) => {
                if let Some(p) = self.node_mut(p) {
                    p.first_child = next;
                }
            }
            (None, None) if top => self.first_root = next,
            (None, None) => {}
        }
        match (next, parent) {
            (Some(n), _) => {
                if let Some(n) = self.node_mut(n) {
                    n.prev_sibling = prev;
                }
            }
            (None, Some(p)) => {
                if let Some(p) = self.node_mut(p) {
                    p.last_child = prev;
                }
            }
            (None, None) if top => self.last_root = prev,
            (None, None) => {}
        }
        if let Some(n) = self.node_mut(id) {
            n.parent = None;
            n.prev_sibling = None;
            n.next_sibling = None;
        }
    }

    /// The node after `cur` in document order, following links only.
    fn next_in_document(&self, cur: NodeId) -> Option<NodeId> {
        if let Some(child) = self.node(cur).and_then(|n| n.first_child) {
            return Some(child);
        }
        let mut n = cur;
        loop {
            let node = self.node(n)?;
            if let Some(sib) = node.next_sibling {
                return Some(sib);
            }
            n = node.parent?;
        }
    }

    // --- public read API -------------------------------------------------

    /// Number of nodes in the arena, including removed and detached ones.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the document contains no nodes.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Recovery diagnostics collected during parsing (empty for clean input).
    pub fn errors(&self) -> &[ParseError] {
        &self.errors
    }

    /// A handle to a node by id, or `None` if the id is out of range.
    pub fn get(&self, id: NodeId) -> Option<NodeRef<'_>> {
        if id.index() < self.nodes.len() {
            Some(NodeRef { dom: self, id })
        } else {
            None
        }
    }

    /// The first top-level node (the document element for a typical page).
    pub fn root(&self) -> Option<NodeRef<'_>> {
        self.first_root.map(|id| NodeRef { dom: self, id })
    }

    /// All top-level nodes, in document order.
    pub fn roots(&self) -> impl Iterator<Item = NodeRef<'_>> {
        core::iter::successors(self.first_root, move |&id| {
            self.node(id).and_then(|n| n.next_sibling)
        })
        .map(move |id| NodeRef { dom: self, id })
    }

    /// Every node in the document, in document (pre-order) order. Nodes
    /// removed with [`Dom::remove`] (and created nodes not yet attached) are
    /// skipped.
    ///
    /// ```
    /// let mut dom = domtree::parse("<p>a</p><p>c</p>");
    /// let c = dom.find_by_tag("p").nth(1).unwrap().id();
    /// dom.insert_html_before(c, "<p>b</p>");
    /// let text: Vec<_> = dom.nodes().filter_map(|n| n.text()).collect();
    /// assert_eq!(text, ["a", "b", "c"]);
    /// ```
    pub fn nodes(&self) -> impl Iterator<Item = NodeRef<'_>> {
        Nodes {
            dom: self,
            order: if self.reordered {
                Order::Links(self.first_root)
            } else {
                Order::Arena(0)
            },
        }
    }

    /// All elements whose tag name equals `name` (ASCII case-insensitive).
    pub fn find_by_tag<'a>(&'a self, name: &'a str) -> impl Iterator<Item = NodeRef<'a>> + 'a {
        self.nodes()
            .filter(move |n| n.tag_name().is_some_and(|t| t.eq_ignore_ascii_case(name)))
    }

    /// The first element with `id="…"` equal to `id`, if any.
    pub fn find_by_id<'a>(&'a self, id: &'a str) -> Option<NodeRef<'a>> {
        self.nodes().find(|n| n.attr("id") == Some(id))
    }

    /// All elements whose `class` attribute contains `class`.
    pub fn find_by_class<'a>(&'a self, class: &'a str) -> impl Iterator<Item = NodeRef<'a>> + 'a {
        self.nodes().filter(move |n| n.has_class(class))
    }
}

/// The iterator behind [`Dom::nodes`].
struct Nodes<'a> {
    dom: &'a Dom,
    order: Order,
}

enum Order {
    /// Arena order is document order: scan it, skipping detached nodes.
    Arena(usize),
    /// An edit moved nodes: follow the links from the first root (allocation-
    /// free, and only reaches attached nodes).
    Links(Option<NodeId>),
}

impl<'a> Iterator for Nodes<'a> {
    type Item = NodeRef<'a>;

    fn next(&mut self) -> Option<NodeRef<'a>> {
        let dom = self.dom;
        let id = match &mut self.order {
            Order::Arena(i) => loop {
                let node = dom.nodes.get(*i)?;
                let id = NodeId(*i as u32);
                *i += 1;
                if !node.detached {
                    break id;
                }
            },
            Order::Links(next) => {
                let id = (*next)?;
                *next = dom.next_in_document(id);
                id
            }
        };
        Some(NodeRef { dom, id })
    }
}

impl fmt::Debug for Dom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut list = f.debug_list();
        for r in self.roots() {
            list.entry(&r);
        }
        list.finish()
    }
}

/// A lightweight, `Copy` read handle to a single node, bound to its [`Dom`].
///
/// All traversal (`parent`, `children`, `descendants`, `ancestors`, siblings)
/// is O(1) per step thanks to the arena's links.
#[derive(Clone, Copy)]
pub struct NodeRef<'a> {
    dom: &'a Dom,
    id: NodeId,
}

impl<'a> NodeRef<'a> {
    #[inline]
    fn raw(&self) -> &'a Node {
        // Invariant: a `NodeRef` is only ever constructed from an id valid in
        // `dom` (via the `Dom` accessors or by following node links). The arena
        // is append-only and never shrinks, so this index is always in bounds.
        debug_assert!(self.id.index() < self.dom.nodes.len());
        &self.dom.nodes[self.id.index()]
    }

    /// This node's arena id.
    #[inline]
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// The byte range this node was parsed from, in the original input
    /// string (pair it with [`crate::line_col`] for line/column positions).
    ///
    /// - **Text, comments, doctypes, CDATA:** exactly their markup, raw —
    ///   `&amp;` in the source spans 5 bytes even though the text is `&`.
    /// - **Elements:** from the `<` of the start tag to the `>` of the end tag.
    ///   Void and self-closing elements span just their start tag. An element
    ///   with no end tag of its own ends where recovery closed it: at the
    ///   start of the tag that closed it (a start tag like the next `<li>`, or
    ///   a mismatched end tag like `</ul>`), or at the end of the input. So a
    ///   parent's range always contains its children's.
    /// - **`None`** for nodes created by editing ([`Dom::create_element`],
    ///   [`Dom::set_text`] on an element, …), including those inserted with
    ///   [`Dom::append_html`] and friends, whose source is a different string.
    ///
    /// Edits never move a range: a renamed element or rewritten text node
    /// still points at where it came from.
    ///
    /// ```
    /// let src = "<ul>\n  <li>Tom &amp; Jerry\n  <li>Spike</ul>";
    /// let dom = domtree::parse(src);
    /// let first = dom.find_by_tag("li").next().unwrap();
    /// // Closed implicitly by the second `<li>`:
    /// assert_eq!(&src[first.source_range().unwrap()], "<li>Tom &amp; Jerry\n  ");
    /// let text = first.first_child().unwrap();
    /// assert_eq!(&src[text.source_range().unwrap()], "Tom &amp; Jerry\n  ");
    /// assert_eq!(&src[dom.root().unwrap().source_range().unwrap()], src);
    /// ```
    pub fn source_range(&self) -> Option<Range<usize>> {
        self.raw().src.get()
    }

    /// Whether this node is *not* part of the document: it was removed (or
    /// sits inside a removed subtree), or was created by editing and not yet
    /// attached. Detached nodes are skipped by [`Dom::nodes`], selectors and
    /// serializers of the whole document. The inverse of the DOM's
    /// `isConnected`.
    ///
    /// ```
    /// let mut dom = domtree::parse("<p>a <b>b</b></p>");
    /// let p = dom.root().unwrap().id();
    /// let b = dom.find_by_tag("b").next().unwrap().id();
    /// dom.remove(b);
    /// assert!(dom.get(b).unwrap().is_detached());
    /// dom.append_child(p, b); // re-attached
    /// assert!(!dom.get(b).unwrap().is_detached());
    /// ```
    pub fn is_detached(&self) -> bool {
        self.raw().detached
    }

    /// The node's kind (element / text / comment / doctype) as a borrowed view.
    pub fn kind(&self) -> NodeKind<'a> {
        match &self.raw().raw {
            RawKind::Element { name, .. } => NodeKind::Element {
                tag: self.dom.span_str(*name),
            },
            RawKind::Text(s) => NodeKind::Text(self.dom.span_str(*s)),
            RawKind::Comment(s) => NodeKind::Comment(self.dom.span_str(*s)),
            RawKind::Doctype(s) => NodeKind::Doctype(self.dom.span_str(*s)),
        }
    }

    /// Whether this node is an element.
    pub fn is_element(&self) -> bool {
        matches!(self.raw().raw, RawKind::Element { .. })
    }

    /// Whether this node is a text node.
    pub fn is_text(&self) -> bool {
        matches!(self.raw().raw, RawKind::Text(_))
    }

    /// The tag name (lower-cased), if this node is an element.
    pub fn tag_name(&self) -> Option<&'a str> {
        match &self.raw().raw {
            RawKind::Element { name, .. } => Some(self.dom.span_str(*name)),
            _ => None,
        }
    }

    /// The value of attribute `name`, if this is an element with that attribute.
    pub fn attr(&self, name: &str) -> Option<&'a str> {
        let dom = self.dom;
        match &self.raw().raw {
            RawKind::Element { attrs, .. } => attrs
                .iter()
                .find(|&&(k, _)| dom.span_str(k) == name)
                .map(|&(_, v)| dom.span_str(v)),
            _ => None,
        }
    }

    /// Whether attribute `name` is present (including valueless boolean attrs).
    pub fn has_attr(&self, name: &str) -> bool {
        let dom = self.dom;
        match &self.raw().raw {
            RawKind::Element { attrs, .. } => attrs.iter().any(|&(k, _)| dom.span_str(k) == name),
            _ => false,
        }
    }

    /// All `(name, value)` attribute pairs (empty iterator for non-elements).
    pub fn attributes(&self) -> impl Iterator<Item = (&'a str, &'a str)> {
        let dom = self.dom;
        let attrs: &'a [(Span, Span)] = match &self.raw().raw {
            RawKind::Element { attrs, .. } => attrs.as_slice(),
            _ => &[],
        };
        attrs
            .iter()
            .map(move |&(k, v)| (dom.span_str(k), dom.span_str(v)))
    }

    /// The class names on this element.
    pub fn classes(&self) -> impl Iterator<Item = &'a str> {
        self.attr("class")
            .into_iter()
            .flat_map(str::split_whitespace)
    }

    /// Whether this element has the given class.
    pub fn has_class(&self, class: &str) -> bool {
        self.classes().any(|c| c == class)
    }

    /// This node's own text, if it is a text node (already entity-decoded).
    pub fn text(&self) -> Option<&'a str> {
        match &self.raw().raw {
            RawKind::Text(s) => Some(self.dom.span_str(*s)),
            _ => None,
        }
    }

    /// The body text of this comment node, if it is one.
    pub fn comment(&self) -> Option<&'a str> {
        match &self.raw().raw {
            RawKind::Comment(s) => Some(self.dom.span_str(*s)),
            _ => None,
        }
    }

    /// Concatenated text of this node and all its descendants, in document
    /// order (entity-decoded). Equivalent to the DOM `textContent`.
    pub fn text_content(&self) -> String {
        let mut out = String::new();
        if let RawKind::Text(s) = &self.raw().raw {
            out.push_str(self.dom.span_str(*s));
        }
        for d in self.descendants() {
            if let RawKind::Text(s) = &d.raw().raw {
                out.push_str(self.dom.span_str(*s));
            }
        }
        out
    }

    // --- traversal -------------------------------------------------------

    /// This node's parent, if any.
    pub fn parent(&self) -> Option<NodeRef<'a>> {
        self.raw().parent.map(|id| NodeRef { dom: self.dom, id })
    }

    /// The first child, if any.
    pub fn first_child(&self) -> Option<NodeRef<'a>> {
        self.raw()
            .first_child
            .map(|id| NodeRef { dom: self.dom, id })
    }

    /// The last child, if any.
    pub fn last_child(&self) -> Option<NodeRef<'a>> {
        self.raw()
            .last_child
            .map(|id| NodeRef { dom: self.dom, id })
    }

    /// The next sibling, if any.
    pub fn next_sibling(&self) -> Option<NodeRef<'a>> {
        self.raw()
            .next_sibling
            .map(|id| NodeRef { dom: self.dom, id })
    }

    /// The previous sibling, if any.
    pub fn prev_sibling(&self) -> Option<NodeRef<'a>> {
        self.raw()
            .prev_sibling
            .map(|id| NodeRef { dom: self.dom, id })
    }

    /// The next sibling that is an element (skips text, comments, doctypes).
    ///
    /// ```
    /// let dom = domtree::parse("<p><a>1</a> text <!-- c --><b>2</b></p>");
    /// let a = dom.find_by_tag("a").next().unwrap();
    /// assert_eq!(a.next_element_sibling().unwrap().tag_name(), Some("b"));
    /// assert!(a.prev_element_sibling().is_none());
    /// ```
    pub fn next_element_sibling(&self) -> Option<NodeRef<'a>> {
        core::iter::successors(self.next_sibling(), NodeRef::next_sibling).find(NodeRef::is_element)
    }

    /// The previous sibling that is an element (skips text, comments, doctypes).
    ///
    /// ```
    /// let dom = domtree::parse("<ul><li>a</li> <li>b</li></ul>");
    /// let b = dom.find_by_tag("li").nth(1).unwrap();
    /// assert_eq!(b.prev_element_sibling().unwrap().text_content(), "a");
    /// ```
    pub fn prev_element_sibling(&self) -> Option<NodeRef<'a>> {
        core::iter::successors(self.prev_sibling(), NodeRef::prev_sibling).find(NodeRef::is_element)
    }

    /// This node's direct children, in order.
    pub fn children(&self) -> impl Iterator<Item = NodeRef<'a>> {
        let dom = self.dom;
        core::iter::successors(self.raw().first_child, move |id| {
            dom.node(*id).and_then(|n| n.next_sibling)
        })
        .map(move |id| NodeRef { dom, id })
    }

    /// Child *elements* only (skips text/comment/doctype nodes).
    pub fn child_elements(&self) -> impl Iterator<Item = NodeRef<'a>> {
        self.children().filter(NodeRef::is_element)
    }

    /// This node's ancestors, from parent up to the root.
    pub fn ancestors(&self) -> impl Iterator<Item = NodeRef<'a>> {
        let dom = self.dom;
        core::iter::successors(self.raw().parent, move |id| {
            dom.node(*id).and_then(|n| n.parent)
        })
        .map(move |id| NodeRef { dom, id })
    }

    /// All descendants of this node, in document (pre-order) order. Does not
    /// include the node itself. Allocation-free.
    pub fn descendants(&self) -> Descendants<'a> {
        Descendants {
            dom: self.dom,
            root: self.id,
            next: self.raw().first_child,
        }
    }
}

impl<'a> fmt::Debug for NodeRef<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Dbg(*self, 0).fmt(f)
    }
}

/// `Debug` nests one formatter frame per tree level, so deeper levels are
/// elided to keep `{:?}` on pathological input from overflowing the stack.
const DEBUG_MAX_DEPTH: usize = 64;

struct Dbg<'a>(NodeRef<'a>, usize);
impl<'a> fmt::Debug for Dbg<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (node, depth) = (self.0, self.1);
        match node.kind() {
            NodeKind::Element { .. } if depth >= DEBUG_MAX_DEPTH => f.write_str("…"),
            NodeKind::Element { tag } => f
                .debug_struct("Element")
                .field("tag", &tag)
                .field("attrs", &Attrs(node))
                .field("children", &Kids(node, depth + 1))
                .finish(),
            NodeKind::Text(t) => f.debug_tuple("Text").field(&t).finish(),
            NodeKind::Comment(c) => f.debug_tuple("Comment").field(&c).finish(),
            NodeKind::Doctype(d) => f.debug_tuple("Doctype").field(&d).finish(),
        }
    }
}

struct Attrs<'a>(NodeRef<'a>);
impl<'a> fmt::Debug for Attrs<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut m = f.debug_map();
        for (k, v) in self.0.attributes() {
            m.entry(&k, &v);
        }
        m.finish()
    }
}

struct Kids<'a>(NodeRef<'a>, usize);
impl<'a> fmt::Debug for Kids<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let depth = self.1;
        f.debug_list()
            .entries(self.0.children().map(|c| Dbg(c, depth)))
            .finish()
    }
}

/// Pre-order descendant iterator (see [`NodeRef::descendants`]).
pub struct Descendants<'a> {
    dom: &'a Dom,
    root: NodeId,
    next: Option<NodeId>,
}

impl<'a> Iterator for Descendants<'a> {
    type Item = NodeRef<'a>;

    fn next(&mut self) -> Option<NodeRef<'a>> {
        let cur = self.next?;
        self.next = self.advance(cur);
        Some(NodeRef {
            dom: self.dom,
            id: cur,
        })
    }
}

impl<'a> Descendants<'a> {
    fn advance(&self, cur: NodeId) -> Option<NodeId> {
        if let Some(child) = self.dom.node(cur).and_then(|n| n.first_child) {
            return Some(child);
        }
        let mut n = cur;
        loop {
            if n == self.root {
                return None;
            }
            if let Some(sib) = self.dom.node(n).and_then(|nd| nd.next_sibling) {
                return Some(sib);
            }
            match self.dom.node(n).and_then(|nd| nd.parent) {
                Some(p) => n = p,
                None => return None,
            }
        }
    }
}
