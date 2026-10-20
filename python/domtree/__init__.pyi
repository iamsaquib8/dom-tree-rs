"""Type stubs for ``domtree`` (the ``dom-tree-rs`` package on PyPI).

Parse messy real-world HTML, query it with CSS selectors, and get readable
text, Markdown, HTML or JSON back. Powered by the dom-tree-rs Rust crate.
"""

from typing import Optional, TypeVar, Union, final, overload

__all__ = ["Document", "Element", "__version__", "parse"]

__version__: str

_T = TypeVar("_T")

def parse(html: str) -> Document:
    """Parse an HTML document (or fragment) into a :class:`Document`.

    Never raises on malformed HTML: anything the parser had to recover from is
    listed in :attr:`Document.errors`. Large inputs are parsed with the GIL
    released.
    """

@final
class Document:
    """A parsed HTML document. Create one with :func:`domtree.parse`.

    Immutable and safe to share between threads.
    """

    def select(self, selector: str) -> list[Element]:
        """Every element matching a CSS selector, in document order.

        Raises :class:`ValueError` if the selector is invalid or unsupported.
        """
    def select_first(self, selector: str) -> Optional[Element]:
        """The first element matching a CSS selector, or ``None``.

        Raises :class:`ValueError` if the selector is invalid or unsupported.
        """
    def to_text(self) -> str:
        """Readable plain text: no ``<script>``/``<style>``/``<head>``,
        whitespace collapsed, a line per block, a blank line between
        paragraphs."""
    def to_markdown(self) -> str:
        """CommonMark + GFM Markdown (headings, lists, links, images, code
        blocks, tables, quotes) — handy for LLM prompts and RAG indexes."""
    def to_html(self) -> str:
        """The document re-serialized as HTML."""
    def to_json(self) -> str:
        """The whole tree as a JSON string (``json.loads`` it for nested
        dicts)."""
    @property
    def errors(self) -> list[tuple[int, str]]:
        """Recovery diagnostics as ``(byte_offset, message)`` tuples; empty for
        clean input. Offsets index the UTF-8 encoding of the source."""
    @property
    def root(self) -> Optional[Element]:
        """The first top-level element (``<html>`` for a typical page), or
        ``None``."""
    @property
    def roots(self) -> list[Element]:
        """All top-level elements, in document order."""
    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...

@final
class Element:
    """An HTML element inside a :class:`Document`. Keeps its document alive.

    Elements compare equal (and hash equally) when they are the same node of
    the same document.
    """

    @property
    def tag(self) -> str:
        """The lower-cased tag name, e.g. ``"div"``."""
    @property
    def attrs(self) -> dict[str, str]:
        """The attributes as a new ``dict`` (valueless attributes map to
        ``""``)."""
    @overload
    def attr(self, name: str) -> Optional[str]:
        """The value of attribute ``name``, or ``None`` if it is absent."""
    @overload
    def attr(self, name: str, default: _T) -> Union[str, _T]:
        """The value of attribute ``name``, or ``default`` if it is absent."""
    @property
    def text(self) -> str:
        """Readable text of this element (like :meth:`Document.to_text`)."""
    @property
    def text_content(self) -> str:
        """All descendant text concatenated, verbatim (DOM ``textContent``)."""
    @property
    def html(self) -> str:
        """This element as HTML, including its own tags (DOM ``outerHTML``)."""
    @property
    def inner_html(self) -> str:
        """The HTML of this element's children (DOM ``innerHTML``)."""
    def select(self, selector: str) -> list[Element]:
        """Descendants matching a CSS selector, in document order.

        As with the DOM's ``querySelectorAll``, the selector is matched against
        the whole document; use ``:scope`` to anchor it here
        (``":scope > li"``). Raises :class:`ValueError` for an invalid selector.
        """
    def select_first(self, selector: str) -> Optional[Element]:
        """The first descendant matching a CSS selector, or ``None``."""
    def closest(self, selector: str) -> Optional[Element]:
        """The nearest element matching ``selector``, starting with this
        element itself and walking up its ancestors."""
    def matches(self, selector: str) -> bool:
        """Whether this element matches a CSS selector."""
    @property
    def parent(self) -> Optional[Element]:
        """The parent element, or ``None`` for a top-level element."""
    @property
    def children(self) -> list[Element]:
        """The child elements (text and comment nodes are skipped)."""
    @property
    def document(self) -> Document:
        """The :class:`Document` this element belongs to."""
    def to_markdown(self) -> str:
        """This element as Markdown."""
    def to_json(self) -> str:
        """This element's subtree as a JSON string."""
    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...
    def __eq__(self, other: object, /) -> bool: ...
    def __hash__(self) -> int: ...
