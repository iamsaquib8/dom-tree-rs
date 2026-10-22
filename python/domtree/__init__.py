"""Parse messy real-world HTML, query it with CSS selectors, and get readable
text, Markdown, HTML or JSON back. Powered by the dom-tree-rs Rust crate.

>>> import domtree
>>> doc = domtree.parse("<ul><li class='x'>a</li><li>b</li></ul>")
>>> [li.text for li in doc.select("li")]
['a', 'b']
>>> doc.select_first("li.x").html
'<li class="x">a</li>'

Declarative extraction into dataclasses (:func:`extract`, :func:`extract_all`):

>>> from dataclasses import dataclass
>>> from typing import Annotated
>>> @dataclass
... class Item:
...     name: Annotated[str, domtree.Select("li.x")]
>>> domtree.extract(Item, "<ul><li class='x'>a</li><li>b</li></ul>")
Item(name='a')
"""

from ._domtree import Document, Element, __version__, parse
from ._extract import Attr, ExtractError, Html, Markdown, OuterHtml, Select, extract, extract_all

__all__ = [
    "Attr",
    "Document",
    "Element",
    "ExtractError",
    "Html",
    "Markdown",
    "OuterHtml",
    "Select",
    "__version__",
    "extract",
    "extract_all",
    "parse",
]
