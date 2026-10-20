//! Python bindings for [dom-tree-rs](https://github.com/iamsaquib8/dom-tree-rs):
//! `import domtree`.
//!
//! A thin layer over the core crate. Every `Element` holds a strong reference
//! to its `Document` plus a node id, so elements stay valid for as long as
//! Python keeps them around. Both classes are immutable (`frozen`), which makes
//! them safe to share between threads and lets the heavy work (parsing and
//! serializing large documents) run with the GIL released.

#![forbid(unsafe_code)]

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use domtree::{Dom, NodeId, NodeRef, Selector};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::marker::Ungil;
use pyo3::prelude::*;
use pyo3::pybacked::PyBackedStr;
use pyo3::types::{PyDict, PyString};

/// Inputs at least this many bytes long are parsed with the GIL released.
const DETACH_BYTES: usize = 16 * 1024;
/// Documents with at least this many nodes are serialized with the GIL released.
const DETACH_NODES: usize = 1024;

/// Run `f`, releasing the GIL first when the work is large enough to be worth it.
fn maybe_detach<T: Ungil>(py: Python<'_>, large: bool, f: impl Ungil + FnOnce() -> T) -> T {
    if large { py.detach(f) } else { f() }
}

/// Parse a selector, raising `ValueError` (with the position) when it is invalid.
fn compile(selector: &str) -> PyResult<Selector> {
    Selector::parse(selector)
        .map_err(|e| PyValueError::new_err(format!("invalid selector '{selector}': {e}")))
}

/// Parse an HTML document (or fragment) into a `Document`.
///
/// Never raises on malformed HTML: anything the parser had to recover from is
/// listed in `Document.errors`. Large inputs are parsed with the GIL released.
#[pyfunction]
fn parse(py: Python<'_>, html: PyBackedStr) -> Document {
    let dom = maybe_detach(py, html.len() >= DETACH_BYTES, || domtree::parse(&html));
    Document { dom }
}

/// A parsed HTML document. Create one with `domtree.parse(html)`.
#[pyclass(frozen, module = "domtree")]
struct Document {
    dom: Dom,
}

impl Document {
    fn large(&self) -> bool {
        self.dom.len() >= DETACH_NODES
    }
}

#[pymethods]
impl Document {
    /// Every element matching a CSS selector, in document order.
    ///
    /// Raises `ValueError` if the selector is invalid or unsupported.
    fn select(slf: &Bound<'_, Self>, selector: PyBackedStr) -> PyResult<Vec<Element>> {
        let sel = compile(&selector)?;
        let doc = slf.get();
        Ok(doc
            .dom
            .select(&sel)
            .map(|n| Element::new(slf, n.id()))
            .collect())
    }

    /// The first element matching a CSS selector, or `None`.
    ///
    /// Raises `ValueError` if the selector is invalid or unsupported.
    fn select_first(slf: &Bound<'_, Self>, selector: PyBackedStr) -> PyResult<Option<Element>> {
        let sel = compile(&selector)?;
        Ok(slf
            .get()
            .dom
            .select_first(&sel)
            .map(|n| Element::new(slf, n.id())))
    }

    /// Readable plain text: no `<script>`/`<style>`/`<head>`, whitespace
    /// collapsed, a line per block and a blank line between paragraphs.
    fn to_text(&self, py: Python<'_>) -> String {
        maybe_detach(py, self.large(), || self.dom.to_text())
    }

    /// CommonMark + GFM Markdown (headings, lists, links, images, code
    /// blocks, tables, quotes) — handy for LLM prompts and RAG indexes.
    fn to_markdown(&self, py: Python<'_>) -> String {
        maybe_detach(py, self.large(), || self.dom.to_markdown())
    }

    /// The document re-serialized as HTML.
    fn to_html(&self, py: Python<'_>) -> String {
        maybe_detach(py, self.large(), || self.dom.to_html())
    }

    /// The whole tree as a JSON string (`json.loads` it for nested dicts).
    fn to_json(&self, py: Python<'_>) -> String {
        maybe_detach(py, self.large(), || self.dom.to_json())
    }

    /// Recovery diagnostics as `(byte_offset, message)` tuples; empty for
    /// clean input. Offsets index the UTF-8 encoding of the source.
    #[getter]
    fn errors(&self) -> Vec<(usize, &'static str)> {
        self.dom
            .errors()
            .iter()
            .map(|e| (e.position, e.message()))
            .collect()
    }

    /// The first top-level element (`<html>` for a typical page), or `None`.
    #[getter]
    fn root(slf: &Bound<'_, Self>) -> Option<Element> {
        slf.get()
            .dom
            .roots()
            .find(NodeRef::is_element)
            .map(|n| Element::new(slf, n.id()))
    }

    /// All top-level elements, in document order (a fragment can have several).
    #[getter]
    fn roots(slf: &Bound<'_, Self>) -> Vec<Element> {
        slf.get()
            .dom
            .roots()
            .filter(NodeRef::is_element)
            .map(|n| Element::new(slf, n.id()))
            .collect()
    }

    fn __str__(&self, py: Python<'_>) -> String {
        self.to_html(py)
    }

    fn __repr__(&self) -> String {
        let roots: Vec<String> = self
            .dom
            .roots()
            .filter_map(|n| n.tag_name().map(|t| format!("<{t}>")))
            .collect();
        format!(
            "<Document roots=[{}] nodes={} errors={}>",
            roots.join(", "),
            self.dom.len(),
            self.dom.errors().len()
        )
    }
}

/// An HTML element inside a `Document`. Keeps its document alive.
#[pyclass(frozen, module = "domtree")]
struct Element {
    doc: Py<Document>,
    id: NodeId,
}

impl Element {
    fn new(doc: &Bound<'_, Document>, id: NodeId) -> Element {
        Element {
            doc: doc.clone().unbind(),
            id,
        }
    }

    fn node(&self) -> PyResult<NodeRef<'_>> {
        // Ids come from the same (immutable) document, so this always succeeds.
        self.doc
            .get()
            .dom
            .get(self.id)
            .ok_or_else(|| PyRuntimeError::new_err("element no longer exists"))
    }

    fn wrap(&self, py: Python<'_>, node: NodeRef<'_>) -> Element {
        Element {
            doc: self.doc.clone_ref(py),
            id: node.id(),
        }
    }
}

#[pymethods]
impl Element {
    /// The lower-cased tag name, e.g. `"div"`.
    #[getter]
    fn tag(&self) -> PyResult<&str> {
        Ok(self.node()?.tag_name().unwrap_or(""))
    }

    /// The attributes as a new `dict` (for a duplicated attribute, the first
    /// occurrence wins — the same value `attr()` returns).
    #[getter]
    fn attrs<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        for (name, value) in self.node()?.attributes() {
            if !dict.contains(name)? {
                dict.set_item(name, value)?;
            }
        }
        Ok(dict)
    }

    /// The value of attribute `name`, or `default` if it is absent.
    #[pyo3(signature = (name, default = None))]
    fn attr<'py>(
        &self,
        py: Python<'py>,
        name: PyBackedStr,
        default: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        Ok(match self.node()?.attr(&name) {
            Some(value) => PyString::new(py, value).into_any(),
            None => default.unwrap_or_else(|| py.None().into_bound(py)),
        })
    }

    /// Readable text of this element (like `Document.to_text()`).
    #[getter]
    fn text(&self) -> PyResult<String> {
        Ok(self.node()?.to_text())
    }

    /// All descendant text concatenated, verbatim (DOM `textContent`).
    #[getter]
    fn text_content(&self) -> PyResult<String> {
        Ok(self.node()?.text_content())
    }

    /// This element as HTML, including its own tags (DOM `outerHTML`).
    #[getter]
    fn html(&self) -> PyResult<String> {
        Ok(self.node()?.outer_html())
    }

    /// The HTML of this element's children (DOM `innerHTML`).
    #[getter]
    fn inner_html(&self) -> PyResult<String> {
        Ok(self.node()?.inner_html())
    }

    /// Descendants matching a CSS selector, in document order. As with the
    /// DOM's `querySelectorAll`, the selector is matched against the whole
    /// document; use `:scope` to anchor it here (`":scope > li"`).
    fn select(&self, py: Python<'_>, selector: PyBackedStr) -> PyResult<Vec<Element>> {
        let sel = compile(&selector)?;
        Ok(self
            .node()?
            .select(&sel)
            .map(|n| self.wrap(py, n))
            .collect())
    }

    /// The first descendant matching a CSS selector, or `None`.
    fn select_first(&self, py: Python<'_>, selector: PyBackedStr) -> PyResult<Option<Element>> {
        let sel = compile(&selector)?;
        Ok(self.node()?.select_first(&sel).map(|n| self.wrap(py, n)))
    }

    /// The nearest element matching `selector`, starting with this element
    /// itself and walking up its ancestors (DOM `Element.closest`).
    fn closest(&self, py: Python<'_>, selector: PyBackedStr) -> PyResult<Option<Element>> {
        let sel = compile(&selector)?;
        Ok(self.node()?.closest(&sel).map(|n| self.wrap(py, n)))
    }

    /// Whether this element matches a CSS selector (DOM `Element.matches`).
    fn matches(&self, selector: PyBackedStr) -> PyResult<bool> {
        let sel = compile(&selector)?;
        Ok(self.node()?.matches(&sel))
    }

    /// The parent element, or `None` for a top-level element.
    #[getter]
    fn parent(&self, py: Python<'_>) -> PyResult<Option<Element>> {
        Ok(self
            .node()?
            .parent()
            .filter(NodeRef::is_element)
            .map(|n| self.wrap(py, n)))
    }

    /// The child elements (text and comment nodes are skipped).
    #[getter]
    fn children(&self, py: Python<'_>) -> PyResult<Vec<Element>> {
        Ok(self
            .node()?
            .child_elements()
            .map(|n| self.wrap(py, n))
            .collect())
    }

    /// The `Document` this element belongs to.
    #[getter]
    fn document(&self, py: Python<'_>) -> Py<Document> {
        self.doc.clone_ref(py)
    }

    /// This element as Markdown.
    fn to_markdown(&self) -> PyResult<String> {
        Ok(self.node()?.to_markdown())
    }

    /// This element's subtree as a JSON string.
    fn to_json(&self) -> PyResult<String> {
        Ok(self.node()?.to_json())
    }

    fn __str__(&self) -> PyResult<String> {
        self.html()
    }

    fn __repr__(&self) -> PyResult<String> {
        const MAX: usize = 80;
        let node = self.node()?;
        let mut tag = format!("<{}", node.tag_name().unwrap_or(""));
        for (name, value) in node.attributes() {
            if value.is_empty() {
                tag.push_str(&format!(" {name}"));
            } else {
                tag.push_str(&format!(" {name}=\"{value}\""));
            }
        }
        if tag.chars().count() > MAX {
            tag = tag.chars().take(MAX).collect();
            tag.push('…');
        }
        Ok(format!("<Element {tag}>>"))
    }

    fn __eq__(&self, other: &Bound<'_, Element>) -> bool {
        let other = other.get();
        self.doc.as_ptr() == other.doc.as_ptr() && self.id == other.id
    }

    fn __hash__(&self) -> u64 {
        let mut h = DefaultHasher::new();
        (self.doc.as_ptr() as usize).hash(&mut h);
        self.id.hash(&mut h);
        h.finish()
    }
}

/// The native half of the `domtree` package; import `domtree` instead (its
/// `__init__.py` re-exports everything here, next to the type stubs).
#[pymodule(name = "_domtree")]
mod py_domtree {
    #[pymodule_export]
    use super::{Document, Element, parse};

    /// The package version.
    #[pymodule_export]
    #[allow(non_upper_case_globals)]
    const __version__: &str = env!("CARGO_PKG_VERSION");
}
