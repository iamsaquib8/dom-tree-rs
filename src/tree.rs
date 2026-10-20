//! The parsed document ([`Dom`]) and the read handle into it ([`NodeRef`]).

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use crate::error::ParseError;
use crate::node::{Node, NodeId, NodeKind, RawKind, Span};

/// A parsed HTML document: an arena of nodes, a single string buffer they
/// reference, the top-level (root) nodes, and any recovery diagnostics.
///
/// Build one with [`crate::parse`]. It is immutable once built; traverse it with
/// [`NodeRef`] handles from [`Dom::root`], [`Dom::roots`], [`Dom::get`], or the
/// `find_by_*` helpers.
#[derive(Clone)]
pub struct Dom {
    pub(crate) nodes: Vec<Node>,
    pub(crate) strings: String,
    pub(crate) roots: Vec<NodeId>,
    pub(crate) errors: Vec<ParseError>,
}

impl Dom {
    pub(crate) fn with_capacity(src_len: usize) -> Self {
        Dom {
            nodes: Vec::with_capacity(src_len / 16 + 8),
            strings: String::with_capacity(src_len),
            roots: Vec::new(),
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

    pub(crate) fn alloc(&mut self, raw: RawKind) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(Node::new(raw));
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
    pub(crate) fn append(&mut self, parent: Option<NodeId>, child: NodeId) {
        let parent = match parent {
            None => {
                // Top-level nodes are siblings of each other, like children of
                // an implicit document node (so `li + li` works on fragments).
                if let Some(&last) = self.roots.last() {
                    if let Some(l) = self.node_mut(last) {
                        l.next_sibling = Some(child);
                    }
                    if let Some(c) = self.node_mut(child) {
                        c.prev_sibling = Some(last);
                    }
                }
                self.roots.push(child);
                return;
            }
            Some(p) => p,
        };
        if let Some(c) = self.node_mut(child) {
            c.parent = Some(parent);
        }
        let last = self.node(parent).and_then(|n| n.last_child);
        match last {
            None => {
                if let Some(p) = self.node_mut(parent) {
                    p.first_child = Some(child);
                    p.last_child = Some(child);
                }
            }
            Some(last) => {
                if let Some(l) = self.node_mut(last) {
                    l.next_sibling = Some(child);
                }
                if let Some(c) = self.node_mut(child) {
                    c.prev_sibling = Some(last);
                }
                if let Some(p) = self.node_mut(parent) {
                    p.last_child = Some(child);
                }
            }
        }
    }

    // --- editing ---------------------------------------------------------

    /// Remove a node (and its subtree) from the document.
    ///
    /// The node stays valid — handles to it and its descendants still work and
    /// can still be serialized — but it is no longer reachable from the
    /// document: [`Dom::nodes`], `find_by_*`, `select`, and every serializer
    /// skip it. Removing an already-removed node is a no-op.
    ///
    /// ```
    /// let mut dom = domtree::parse("<p>Hi <script>track()</script>there</p>");
    /// let script = dom.find_by_tag("script").next().unwrap().id();
    /// dom.remove(script);
    /// assert_eq!(dom.to_html(), "<p>Hi there</p>");
    /// assert_eq!(dom.find_by_tag("script").count(), 0);
    /// ```
    pub fn remove(&mut self, id: NodeId) {
        let Some(node) = self.node(id) else { return };
        let (parent, prev, next) = (node.parent, node.prev_sibling, node.next_sibling);

        match prev {
            Some(p) => {
                if let Some(p) = self.node_mut(p) {
                    p.next_sibling = next;
                }
            }
            None => {
                if let Some(par) = parent.and_then(|p| self.node_mut(p)) {
                    par.first_child = next;
                }
            }
        }
        match next {
            Some(n) => {
                if let Some(n) = self.node_mut(n) {
                    n.prev_sibling = prev;
                }
            }
            None => {
                if let Some(par) = parent.and_then(|p| self.node_mut(p)) {
                    par.last_child = prev;
                }
            }
        }
        if parent.is_none() {
            self.roots.retain(|&r| r != id);
        }
        if let Some(n) = self.node_mut(id) {
            n.parent = None;
            n.prev_sibling = None;
            n.next_sibling = None;
        }

        let subtree: Vec<NodeId> = match self.get(id) {
            Some(n) => core::iter::once(id)
                .chain(n.descendants().map(|d| d.id()))
                .collect(),
            None => Vec::new(),
        };
        for d in subtree {
            if let Some(n) = self.node_mut(d) {
                n.detached = true;
            }
        }
    }

    /// Set attribute `name` (ASCII-lower-cased) to `value` on an element,
    /// replacing any existing value. Does nothing for non-element nodes.
    ///
    /// ```
    /// let mut dom = domtree::parse("<a href=/x>x</a>");
    /// let a = dom.find_by_tag("a").next().unwrap().id();
    /// dom.set_attr(a, "href", "https://example.com/x");
    /// dom.set_attr(a, "rel", "nofollow");
    /// assert_eq!(dom.to_html(), r#"<a href="https://example.com/x" rel="nofollow">x</a>"#);
    /// ```
    pub fn set_attr(&mut self, id: NodeId, name: &str, value: &str) {
        let name = name.to_ascii_lowercase();
        let existing = match self.node(id).map(|n| &n.raw) {
            Some(RawKind::Element { attrs, .. }) => {
                attrs.iter().position(|&(k, _)| self.span_str(k) == name)
            }
            _ => return,
        };
        let value = self.intern(value);
        let key = match existing {
            Some(_) => Span::EMPTY,
            None => self.intern(&name),
        };
        if let Some(Node {
            raw: RawKind::Element { attrs, .. },
            ..
        }) = self.node_mut(id)
        {
            match existing.and_then(|i| attrs.get_mut(i)) {
                Some(slot) => slot.1 = value,
                None => attrs.push((key, value)),
            }
        }
    }

    /// Remove attribute `name` (ASCII case-insensitive) from an element, if
    /// present.
    ///
    /// ```
    /// let mut dom = domtree::parse(r#"<p style="color:red" class=x>hi</p>"#);
    /// let p = dom.find_by_tag("p").next().unwrap().id();
    /// dom.remove_attr(p, "style");
    /// assert_eq!(dom.to_html(), r#"<p class="x">hi</p>"#);
    /// ```
    pub fn remove_attr(&mut self, id: NodeId, name: &str) {
        let strings = &self.strings;
        if let Some(Node {
            raw: RawKind::Element { attrs, .. },
            ..
        }) = self.nodes.get_mut(id.index())
        {
            attrs.retain(|&(k, _)| {
                !strings
                    .get(k.range())
                    .is_some_and(|k| k.eq_ignore_ascii_case(name))
            });
        }
    }

    // --- public read API -------------------------------------------------

    /// Number of nodes in the arena.
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
        self.roots.first().map(|&id| NodeRef { dom: self, id })
    }

    /// All top-level nodes, in document order.
    pub fn roots(&self) -> impl Iterator<Item = NodeRef<'_>> {
        self.roots.iter().map(move |&id| NodeRef { dom: self, id })
    }

    /// Every node in the document, in document (pre-order) order. Nodes
    /// removed with [`Dom::remove`] are skipped.
    pub fn nodes(&self) -> impl Iterator<Item = NodeRef<'_>> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| !n.detached)
            .map(move |(i, _)| NodeRef {
                dom: self,
                id: NodeId(i as u32),
            })
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

    /// The class names on this element. Like HTML, splits on ASCII whitespace
    /// only, so a non-breaking space stays part of a class name.
    pub fn classes(&self) -> impl Iterator<Item = &'a str> {
        self.attr("class")
            .into_iter()
            .flat_map(str::split_ascii_whitespace)
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
        match self.kind() {
            NodeKind::Element { tag } => f
                .debug_struct("Element")
                .field("tag", &tag)
                .field("attrs", &Attrs(*self))
                .field("children", &Kids(*self))
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

struct Kids<'a>(NodeRef<'a>);
impl<'a> fmt::Debug for Kids<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.0.children()).finish()
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
