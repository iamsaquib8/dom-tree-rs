"""Parse messy real-world HTML, query it with CSS selectors, and get readable
text, Markdown, HTML or JSON back. Powered by the dom-tree-rs Rust crate.

>>> import domtree
>>> doc = domtree.parse("<ul><li class='x'>a</li><li>b</li></ul>")
>>> [li.text for li in doc.select("li")]
['a', 'b']
>>> doc.select_first("li.x").html
'<li class="x">a</li>'
"""

from ._domtree import Document, Element, __version__, parse

__all__ = ["Document", "Element", "__version__", "parse"]
