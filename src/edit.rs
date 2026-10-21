//! The editing API: rewrite a parsed [`Dom`] in place.
//!
//! Nodes are never freed. Removing or replacing a node *detaches* it — its
//! [`NodeId`] stays valid and its subtree can still be read and serialized —
//! and created or inserted nodes are appended to the arena. Every structural
//! edit keeps the parent/child/sibling links consistent (top-level nodes stay
//! linked as siblings) and keeps the `detached` flag set on exactly the nodes
//! that are not reachable from the top level, so iteration, selectors and
//! serializers only ever see the live document.
//!
//! **Document order.** The parser only appends, so arena order is document
//! order and [`Dom::nodes`] (which `find_by_*` and `select` build on) is a plain
//! scan of the arena. An edit that can place a node out of arena order (an
//! insertion or a move) sets a flag, and from then on `nodes()` follows the
//! links from the first root instead — still allocation-free and linear.
//! Removing, unwrapping, renaming and attribute edits keep the fast path.
//!
//! No edit recurses: subtree walks follow the links, so they are as safe on
//! 100,000-deep trees as the parser is.

use alloc::vec::Vec;

use crate::builder;
use crate::node::{AttrRange, Node, NodeId, RawKind, Span, SrcRange};
use crate::tags;
use crate::tree::Dom;

/// What an edit needs to know about a node's kind.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// An element that may have children.
    Container,
    /// A void element (`<br>`, `<img>`, …): never has children.
    Void,
    Text,
    /// A comment or doctype.
    Other,
}

impl Dom {
    // --- helpers ---------------------------------------------------------

    fn shape(&self, id: NodeId) -> Option<Shape> {
        Some(match &self.node(id)?.raw {
            RawKind::Element { name, .. } if tags::is_void(self.span_str(*name)) => Shape::Void,
            RawKind::Element { .. } => Shape::Container,
            RawKind::Text(_) => Shape::Text,
            RawKind::Comment(_) | RawKind::Doctype(_) => Shape::Other,
        })
    }

    /// Where `id` sits: `Some(Some(parent))`, `Some(None)` for a top-level
    /// node, or `None` for a detached node with no parent (removed or created)
    /// and for unknown ids.
    fn container(&self, id: NodeId) -> Option<Option<NodeId>> {
        let n = self.node(id)?;
        match n.parent {
            Some(p) => Some(Some(p)),
            None if !n.detached => Some(None),
            None => None,
        }
    }

    /// Whether `node` is `id` itself or one of its ancestors.
    fn is_inclusive_ancestor(&self, node: NodeId, id: NodeId) -> bool {
        let mut cur = Some(id);
        while let Some(c) = cur {
            if c == node {
                return true;
            }
            cur = self.node(c).and_then(|n| n.parent);
        }
        false
    }

    /// Flag `root` and its whole subtree as detached (or attached).
    fn set_detached(&mut self, root: NodeId, detached: bool) {
        let mut cur = root;
        loop {
            let Some(n) = self.node_mut(cur) else { return };
            n.detached = detached;
            if let Some(child) = n.first_child {
                cur = child;
                continue;
            }
            // Leaf: move to the next node of the subtree, climbing as needed.
            loop {
                if cur == root {
                    return;
                }
                let Some(n) = self.node(cur) else { return };
                if let Some(sib) = n.next_sibling {
                    cur = sib;
                    break;
                }
                match n.parent {
                    Some(p) => cur = p,
                    None => return,
                }
            }
        }
    }

    /// A new parentless, detached node.
    fn alloc_detached(&mut self, raw: RawKind) -> NodeId {
        let id = self.alloc(raw, SrcRange::NONE);
        if let Some(n) = self.node_mut(id) {
            n.detached = true;
        }
        id
    }

    /// Move `node` (with its subtree) into `parent` (`None`: the top level),
    /// before its child `before` (`None`: at the end). Refuses — returning
    /// `false` — when `parent` is inside `node`, which would make a cycle.
    fn place(&mut self, node: NodeId, parent: Option<NodeId>, before: Option<NodeId>) -> bool {
        if self.node(node).is_none() || before == Some(node) {
            return false;
        }
        if parent.is_some_and(|p| self.is_inclusive_ancestor(node, p)) {
            return false;
        }
        let detached = match parent {
            Some(p) => self.node(p).is_none_or(|n| n.detached),
            None => false,
        };
        self.unlink(node);
        self.link(parent, before, node);
        // Subtrees are uniformly flagged, so the root's flag speaks for all.
        if self.node(node).is_some_and(|n| n.detached != detached) {
            self.set_detached(node, detached);
        }
        self.reordered = true;
        true
    }

    /// Parse `html` and insert its top-level nodes into `parent` before
    /// `before`, in order.
    ///
    /// Not one `place` per root: that climbs `parent`'s ancestors for the
    /// cycle check every time (roots × depth). The grafted roots are fresh
    /// orphans, so they cannot contain `parent` and need no check, and their
    /// subtrees are all flagged detached, so only an attached `parent` needs
    /// them re-flagged.
    fn insert_fragment(
        &mut self,
        parent: Option<NodeId>,
        before: Option<NodeId>,
        html: &str,
    ) -> Vec<NodeId> {
        let roots = self.graft(&builder::parse_document(html));
        let attached = match parent {
            Some(p) => self.node(p).is_some_and(|n| !n.detached),
            None => true,
        };
        for &r in &roots {
            self.link(parent, before, r);
            if attached {
                self.set_detached(r, false);
            }
        }
        if !roots.is_empty() {
            self.reordered = true;
        }
        roots
    }

    /// Copy every node of the freshly parsed `frag` into this arena —
    /// detached, with no source range — and return the copies of its
    /// top-level nodes. `frag` was only ever appended to, so its arenas hold
    /// exactly its document: they are appended wholesale, and node ids, string
    /// spans and attribute runs all shift by a constant.
    fn graft(&mut self, frag: &Dom) -> Vec<NodeId> {
        let base = self.nodes.len();
        let shift = |id: Option<NodeId>| id.map(|i| NodeId::from_index(base + i.index()));
        // `as u32` like `intern`: an arena past 4 GiB is out of reach anyway.
        let str_base = self.strings.len() as u32;
        let attr_base = self.attrs.len() as u32;
        let span = |s: Span| Span {
            start: s.start.wrapping_add(str_base),
            len: s.len,
        };
        self.strings.push_str(&frag.strings);
        self.attrs
            .extend(frag.attrs.iter().map(|&(k, v)| (span(k), span(v))));
        self.nodes.reserve(frag.nodes.len());
        for node in &frag.nodes {
            let raw = match node.raw {
                RawKind::Element { name, attrs } => RawKind::Element {
                    name: span(name),
                    attrs: AttrRange {
                        start: attrs.start.wrapping_add(attr_base),
                        len: attrs.len,
                    },
                },
                RawKind::Text(s) => RawKind::Text(span(s)),
                RawKind::Comment(s) => RawKind::Comment(span(s)),
                RawKind::Doctype(s) => RawKind::Doctype(span(s)),
            };
            self.nodes.push(Node {
                parent: shift(node.parent),
                first_child: shift(node.first_child),
                last_child: shift(node.last_child),
                next_sibling: shift(node.next_sibling),
                prev_sibling: shift(node.prev_sibling),
                detached: true,
                src: SrcRange::NONE,
                raw,
            });
        }
        let roots: Vec<NodeId> = frag.roots().filter_map(|r| shift(Some(r.id()))).collect();
        // The copies are still linked to each other as top-level siblings;
        // make each a clean orphan before it is placed.
        for &r in &roots {
            if let Some(n) = self.node_mut(r) {
                n.prev_sibling = None;
                n.next_sibling = None;
            }
        }
        roots
    }

    // --- removing and attributes -----------------------------------------

    /// Remove a node (and its subtree) from the document.
    ///
    /// The node stays valid — handles to it and its descendants still work and
    /// can still be serialized, and it can be re-inserted with
    /// [`Dom::append_child`] or [`Dom::insert_before`] — but it is no longer
    /// reachable from the document: [`Dom::nodes`], `find_by_*`, `select`, and
    /// every serializer skip it. Removing an already-removed node is a no-op.
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
        // Inside an already-removed subtree every descendant is flagged too, so
        // only the links need fixing (keeps `-r div` on deep nesting linear).
        let already_detached = node.detached;
        self.unlink(id);
        if !already_detached {
            self.set_detached(id, true);
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
        let Some(attrs) = self.attr_range(id) else {
            return;
        };
        let name = name.to_ascii_lowercase();
        let existing = self
            .attr_slice(attrs)
            .iter()
            .position(|&(k, _)| self.span_str(k) == name);
        let value = self.intern(value);
        if let Some(i) = existing {
            if let Some(slot) = self.attrs.get_mut(attrs.start as usize + i) {
                slot.1 = value;
            }
            return;
        }
        let key = self.intern(&name);
        // Appending needs the run to end where the arena does: move it there
        // unless it already does (it always does for the element parsed or
        // created last, and after the first move).
        let start = if attrs.range().end == self.attrs.len() {
            attrs.start
        } else {
            let start = self.attrs.len() as u32;
            for i in attrs.range() {
                if let Some(&pair) = self.attrs.get(i) {
                    self.attrs.push(pair);
                }
            }
            self.attr_waste += attrs.len as usize;
            start
        };
        self.attrs.push((key, value));
        self.set_attr_range(
            id,
            AttrRange {
                start,
                len: attrs.len + 1,
            },
        );
        self.maybe_compact_attrs();
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
        let Some(attrs) = self.attr_range(id) else {
            return;
        };
        // Pack the attributes that stay at the front of the run, in order.
        let mut kept = 0;
        for i in attrs.range() {
            let Some(&(k, v)) = self.attrs.get(i) else {
                break;
            };
            if self.span_str(k).eq_ignore_ascii_case(name) {
                continue;
            }
            if let Some(slot) = self.attrs.get_mut(attrs.start as usize + kept) {
                *slot = (k, v);
            }
            kept += 1;
        }
        let removed = attrs.len as usize - kept;
        if removed == 0 {
            return;
        }
        if attrs.range().end == self.attrs.len() {
            self.attrs.truncate(attrs.start as usize + kept);
        } else {
            self.attr_waste += removed;
        }
        self.set_attr_range(
            id,
            AttrRange {
                start: attrs.start,
                len: kept as u32,
            },
        );
        self.maybe_compact_attrs();
    }

    /// The attribute run of element `id` (`None` for other nodes).
    fn attr_range(&self, id: NodeId) -> Option<AttrRange> {
        match self.node(id)?.raw {
            RawKind::Element { attrs, .. } => Some(attrs),
            _ => None,
        }
    }

    fn set_attr_range(&mut self, id: NodeId, range: AttrRange) {
        if let Some(Node {
            raw: RawKind::Element { attrs, .. },
            ..
        }) = self.node_mut(id)
        {
            *attrs = range;
        }
    }

    /// Rebuild the attribute arena without the slots edits left behind, once
    /// they outnumber the live ones plus the nodes. That bounds the arena at
    /// about twice what the document needs however edits move runs around,
    /// and the rebuild (linear in nodes + slots) is paid for by the moves that
    /// made the waste.
    fn maybe_compact_attrs(&mut self) {
        let live = self.attrs.len() - self.attr_waste;
        if self.attr_waste <= live + self.nodes.len() {
            return;
        }
        let mut packed = Vec::with_capacity(live);
        for node in &mut self.nodes {
            if let RawKind::Element { attrs, .. } = &mut node.raw {
                let start = packed.len();
                packed.extend_from_slice(self.attrs.get(attrs.range()).unwrap_or(&[]));
                *attrs = AttrRange {
                    start: start as u32,
                    len: (packed.len() - start) as u32,
                };
            }
        }
        self.attrs = packed;
        self.attr_waste = 0;
    }

    // --- text and names --------------------------------------------------

    /// Replace a node's text.
    ///
    /// On a text node, replaces its content. On an element, removes all of
    /// its children and puts `text` in a single new text node (none if `text`
    /// is empty) — the DOM's `textContent = text`. Does nothing on void
    /// elements, comments and doctypes.
    ///
    /// `text` is literal, never parsed (use [`Dom::append_html`] for markup),
    /// and [`Dom::to_html`] escapes `&`, `<` and `>` in it — except inside
    /// `<script>` and `<style>`, whose content HTML has no escapes for. There
    /// it is written as is, apart from any `</script` (or `</style`) that
    /// would end the element early, which is written `<\/script` — the same
    /// string inside a JavaScript or CSS string literal — so the output still
    /// parses back to a single element.
    ///
    /// ```
    /// let mut dom = domtree::parse("<h1>Old <em>title</em></h1><p>x</p>");
    /// let h1 = dom.find_by_tag("h1").next().unwrap().id();
    /// dom.set_text(h1, "New & <improved>");
    /// assert_eq!(dom.to_html(), "<h1>New &amp; &lt;improved&gt;</h1><p>x</p>");
    ///
    /// let x = dom.find_by_tag("p").next().unwrap().first_child().unwrap().id();
    /// dom.set_text(x, "y");
    /// assert_eq!(dom.to_html(), "<h1>New &amp; &lt;improved&gt;</h1><p>y</p>");
    ///
    /// let mut dom = domtree::parse("<script>old()</script>");
    /// let script = dom.root().unwrap().id();
    /// dom.set_text(script, r#"if (a < b) log("</script>")"#);
    /// assert_eq!(dom.to_html(), r#"<script>if (a < b) log("<\/script>")</script>"#);
    /// ```
    pub fn set_text(&mut self, id: NodeId, text: &str) {
        match self.shape(id) {
            Some(Shape::Text) => {
                let span = self.intern(text);
                if let Some(n) = self.node_mut(id) {
                    n.raw = RawKind::Text(span);
                }
            }
            Some(Shape::Container) => {
                while let Some(child) = self.node(id).and_then(|n| n.first_child) {
                    self.remove(child);
                }
                if !text.is_empty() {
                    let t = self.create_text(text);
                    self.place(t, Some(id), None);
                }
            }
            _ => {}
        }
    }

    /// Change an element's tag name (ASCII-lower-cased, not validated).
    /// Attributes and children are kept; does nothing for non-elements.
    ///
    /// Renaming to a void element (`br`, `img`, …) keeps any children in the
    /// tree, but [`Dom::to_html`] — like the HTML syntax — cannot express them.
    ///
    /// ```
    /// let mut dom = domtree::parse("<b class=x>bold</b>");
    /// let b = dom.root().unwrap().id();
    /// dom.rename(b, "STRONG");
    /// assert_eq!(dom.to_html(), r#"<strong class="x">bold</strong>"#);
    /// ```
    pub fn rename(&mut self, id: NodeId, tag: &str) {
        if !matches!(self.shape(id), Some(Shape::Container | Shape::Void)) {
            return;
        }
        let span = self.intern(&tag.to_ascii_lowercase());
        if let Some(Node {
            raw: RawKind::Element { name, .. },
            ..
        }) = self.node_mut(id)
        {
            *name = span;
        }
    }

    // --- creating and moving ---------------------------------------------

    /// Create a new, empty element (tag ASCII-lower-cased, not validated).
    ///
    /// It starts *detached*: not part of the document until you attach it
    /// with [`Dom::append_child`] or [`Dom::insert_before`]. Created nodes have
    /// no [`source_range`](crate::NodeRef::source_range).
    ///
    /// ```
    /// let mut dom = domtree::parse("<ul><li>a</li></ul>");
    /// let ul = dom.root().unwrap().id();
    /// let li = dom.create_element("li");
    /// let text = dom.create_text("b");
    /// dom.append_child(li, text);
    /// dom.set_attr(li, "class", "new");
    /// dom.append_child(ul, li);
    /// assert_eq!(dom.to_html(), r#"<ul><li>a</li><li class="new">b</li></ul>"#);
    /// ```
    pub fn create_element(&mut self, tag: &str) -> NodeId {
        let name = self.intern(&tag.to_ascii_lowercase());
        // An empty run at the end of the arena: the first `set_attr` appends
        // in place.
        let attrs = AttrRange {
            start: self.attrs.len() as u32,
            len: 0,
        };
        self.alloc_detached(RawKind::Element { name, attrs })
    }

    /// Create a new, detached text node holding `text` literally (see
    /// [`Dom::create_element`] for how detached nodes behave).
    ///
    /// ```
    /// let mut dom = domtree::parse("<p></p>");
    /// let p = dom.root().unwrap().id();
    /// let t = dom.create_text("1 < 2");
    /// dom.append_child(p, t);
    /// assert_eq!(dom.to_html(), "<p>1 &lt; 2</p>");
    /// ```
    pub fn create_text(&mut self, text: &str) -> NodeId {
        let span = self.intern(text);
        self.alloc_detached(RawKind::Text(span))
    }

    /// Move `child` (with its subtree) to the end of `parent`'s children.
    ///
    /// `child` may be a created node, a removed one, or a node elsewhere in the
    /// document (it is detached from its old place first). If `parent` is
    /// itself detached, `child` ends up detached with it. Returns `false` and
    /// changes nothing if `parent` cannot have children (it is not an element,
    /// or is a void element) or if `parent` is `child` or inside it — moving a
    /// node into its own subtree would make a cycle.
    ///
    /// ```
    /// let mut dom = domtree::parse("<div id=a><b>x</b></div><div id=c></div>");
    /// let b = dom.find_by_tag("b").next().unwrap().id();
    /// let c = dom.find_by_id("c").unwrap().id();
    /// assert!(dom.append_child(c, b)); // moves <b> from #a to #c
    /// assert_eq!(dom.to_html(), r#"<div id="a"></div><div id="c"><b>x</b></div>"#);
    ///
    /// assert!(!dom.append_child(b, c)); // #c contains <b>: refused, no cycle
    /// let x = dom.get(b).unwrap().first_child().unwrap().id();
    /// assert!(!dom.append_child(x, c)); // text nodes have no children
    /// ```
    pub fn append_child(&mut self, parent: NodeId, child: NodeId) -> bool {
        self.shape(parent) == Some(Shape::Container) && self.place(child, Some(parent), None)
    }

    /// Move `node` (with its subtree) to just before `reference`, as its
    /// sibling — at the top level if `reference` is a top-level node.
    ///
    /// Like [`Dom::append_child`], `node` may come from anywhere. Returns
    /// `false` and changes nothing if `reference` has no place to insert at (it
    /// is detached with no parent), if `node` is `reference`, or if `node`
    /// contains `reference` (a cycle).
    ///
    /// ```
    /// let mut dom = domtree::parse("<li>b</li><li>c</li><li>a</li>");
    /// let items: Vec<_> = dom.find_by_tag("li").map(|n| n.id()).collect();
    /// assert!(dom.insert_before(items[0], items[2]));
    /// assert_eq!(dom.to_html(), "<li>a</li><li>b</li><li>c</li>");
    /// ```
    pub fn insert_before(&mut self, reference: NodeId, node: NodeId) -> bool {
        if node == reference {
            return false;
        }
        match self.container(reference) {
            Some(parent) => self.place(node, parent, Some(reference)),
            None => false,
        }
    }

    /// Wrap a node in a new `tag` element, which takes the node's place.
    /// Returns the wrapper's id, or `None` if `id` is unknown or `tag` is a
    /// void element. Wrapping a detached node gives a detached wrapper.
    ///
    /// ```
    /// let mut dom = domtree::parse("<p>Hello <b>world</b></p>");
    /// let b = dom.find_by_tag("b").next().unwrap().id();
    /// let a = dom.wrap(b, "a").unwrap();
    /// dom.set_attr(a, "href", "/world");
    /// assert_eq!(dom.to_html(), r#"<p>Hello <a href="/world"><b>world</b></a></p>"#);
    /// ```
    pub fn wrap(&mut self, id: NodeId, tag: &str) -> Option<NodeId> {
        self.node(id)?;
        if tags::is_void(&tag.to_ascii_lowercase()) {
            return None;
        }
        let wrapper = self.create_element(tag);
        if let Some(parent) = self.container(id) {
            self.place(wrapper, parent, Some(id));
        }
        self.place(id, Some(wrapper), None);
        Some(wrapper)
    }

    /// Replace an element by its children: they move up into its place, in
    /// order, and the element itself is removed (detached, now childless).
    /// Returns `false` and changes nothing for non-elements and for detached
    /// elements with no parent (there is no place to move the children to).
    ///
    /// ```
    /// let mut dom = domtree::parse(r#"<p>a <span class="x">b <i>c</i></span> d</p>"#);
    /// let span = dom.find_by_tag("span").next().unwrap().id();
    /// assert!(dom.unwrap(span));
    /// assert_eq!(dom.to_html(), "<p>a b <i>c</i> d</p>");
    /// ```
    pub fn unwrap(&mut self, id: NodeId) -> bool {
        if !matches!(self.shape(id), Some(Shape::Container | Shape::Void)) {
            return false;
        }
        let Some(parent) = self.container(id) else {
            return false;
        };
        // The children keep their relative order, so if arena order was
        // document order it still is: `reordered` is left alone.
        while let Some(child) = self.node(id).and_then(|n| n.first_child) {
            self.unlink(child);
            self.link(parent, Some(id), child);
        }
        self.remove(id);
        true
    }

    // --- parsed fragments ------------------------------------------------

    /// Parse `html` as a fragment and append its nodes to the end of
    /// `parent`'s children. Returns the ids of the inserted top-level nodes,
    /// in order — empty if `html` produced no nodes or `parent` cannot have
    /// children (not an element, or a void element).
    ///
    /// The fragment is parsed on its own, with the normal parser, not in the
    /// context of `parent` (so `<li>` closes `<li>` inside it, but text
    /// appended to a `<script>` is still parsed as HTML). Its parse errors are
    /// **not** added to [`Dom::errors`] — those positions refer to the original
    /// input; check [`crate::parse_fragment`]`(html).errors()` if you need
    /// them. Inserted nodes have no
    /// [`source_range`](crate::NodeRef::source_range).
    ///
    /// ```
    /// let mut dom = domtree::parse("<ul><li>a</li></ul>");
    /// let ul = dom.root().unwrap().id();
    /// let ids = dom.append_html(ul, "<li>b<li>c");
    /// assert_eq!(ids.len(), 2);
    /// assert_eq!(dom.to_html(), "<ul><li>a</li><li>b</li><li>c</li></ul>");
    /// ```
    pub fn append_html(&mut self, parent: NodeId, html: &str) -> Vec<NodeId> {
        if self.shape(parent) != Some(Shape::Container) {
            return Vec::new();
        }
        self.insert_fragment(Some(parent), None, html)
    }

    /// Parse `html` and insert its nodes at the start of `parent`'s children.
    /// See [`Dom::append_html`] for how the fragment is parsed and what is
    /// returned.
    ///
    /// ```
    /// let mut dom = domtree::parse("<p>world</p>");
    /// let p = dom.root().unwrap().id();
    /// dom.prepend_html(p, "<b>Hello</b>, ");
    /// assert_eq!(dom.to_html(), "<p><b>Hello</b>, world</p>");
    /// ```
    pub fn prepend_html(&mut self, parent: NodeId, html: &str) -> Vec<NodeId> {
        if self.shape(parent) != Some(Shape::Container) {
            return Vec::new();
        }
        let first = self.node(parent).and_then(|n| n.first_child);
        self.insert_fragment(Some(parent), first, html)
    }

    /// Parse `html` and insert its nodes just before `id`, as its siblings (at
    /// the top level if `id` is a top-level node). Returns nothing inserted if
    /// `id` is detached with no parent. See [`Dom::append_html`] for how the
    /// fragment is parsed.
    ///
    /// ```
    /// let mut dom = domtree::parse("<h1>Title</h1><p>Body</p>");
    /// let h1 = dom.root().unwrap().id();
    /// dom.insert_html_before(h1, "<!doctype html>");
    /// assert_eq!(dom.to_html(), "<!doctype html><h1>Title</h1><p>Body</p>");
    /// ```
    pub fn insert_html_before(&mut self, id: NodeId, html: &str) -> Vec<NodeId> {
        match self.container(id) {
            Some(parent) => self.insert_fragment(parent, Some(id), html),
            None => Vec::new(),
        }
    }

    /// Parse `html` and insert its nodes just after `id`, as its siblings. See
    /// [`Dom::insert_html_before`].
    ///
    /// ```
    /// let mut dom = domtree::parse("<p>See <a href=/docs>docs</a>.</p>");
    /// let a = dom.find_by_tag("a").next().unwrap().id();
    /// dom.insert_html_after(a, " <sup>[1]</sup>");
    /// assert_eq!(dom.to_html(), r#"<p>See <a href="/docs">docs</a> <sup>[1]</sup>.</p>"#);
    /// ```
    pub fn insert_html_after(&mut self, id: NodeId, html: &str) -> Vec<NodeId> {
        match self.container(id) {
            Some(parent) => {
                let next = self.node(id).and_then(|n| n.next_sibling);
                self.insert_fragment(parent, next, html)
            }
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::format;
    use alloc::string::ToString;
    use alloc::vec::Vec;

    /// Appending to two elements in turn moves one run to the end of the arena
    /// on every call. Without compaction that would be quadratic (~250k
    /// slots here); with it the arena stays within about twice what is live.
    #[test]
    fn moved_attribute_runs_are_compacted() {
        let mut dom = crate::parse("<a x=1></a><b y=2 z=3></b><i></i>");
        let ids: Vec<_> = dom.nodes().map(|n| n.id()).collect();
        let (a, b) = (ids[0], ids[1]);
        for i in 0..500 {
            dom.set_attr(a, &format!("a{i}"), "v");
            dom.set_attr(b, &format!("b{i}"), &i.to_string());
            let live = 3 + 2 * (i + 1);
            assert!(dom.attrs.len() <= 2 * (live + dom.nodes.len()) + 2);
        }
        let a_attrs: Vec<_> = dom.get(a).unwrap().attributes().collect();
        assert_eq!(a_attrs.len(), 501);
        assert_eq!(a_attrs[0], ("x", "1"));
        assert_eq!(a_attrs[500], ("a499", "v"));
        let b = dom.get(b).unwrap();
        assert_eq!(b.attributes().count(), 502);
        assert_eq!(b.attr("z"), Some("3"));
        assert_eq!(b.attr("b250"), Some("250"));
        assert_eq!(dom.get(ids[2]).unwrap().attributes().count(), 0);

        // Removing from a run that is not at the end leaves waste behind,
        // and compaction keeps that bounded too.
        for i in 0..500 {
            dom.remove_attr(a, &format!("a{i}"));
        }
        assert_eq!(
            dom.get(a).unwrap().attributes().collect::<Vec<_>>(),
            [("x", "1")]
        );
        assert!(dom.attrs.len() <= 2 * (504 + dom.nodes.len()) + 2);
        assert_eq!(dom.get(ids[1]).unwrap().attr("b499"), Some("499"));
    }
}
