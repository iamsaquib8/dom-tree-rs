# dom-tree-rs for Python

Parse messy real-world HTML, query it with CSS selectors, and get **readable
text** or **Markdown** back — fast, typed, and it never chokes on bad markup.
Python bindings for the [dom-tree-rs](https://github.com/iamsaquib8/dom-tree-rs)
Rust crate.

```sh
pip install dom-tree-rs
```

```python
import domtree

doc = domtree.parse("<ul><li class='x'>a</li><li>b</li></ul>")
[li.text for li in doc.select("li")]   # ['a', 'b']
doc.select_first("li.x").html          # '<li class="x">a</li>'
```

The distribution is called `dom-tree-rs` (the name `domtree` was taken on
PyPI); the import name is `domtree`. Wheels are `abi3`: one wheel per platform
works on every CPython ≥ 3.9 (Linux x86_64/aarch64, macOS Intel/Apple Silicon,
Windows).

## Scraping

```python
import domtree
import urllib.request

html = urllib.request.urlopen("https://example.com/shop").read().decode()
doc = domtree.parse(html)

products = [
    {
        "name": card.select_first(".title").text,
        "price": card.select_first(".price").text,
        "url": card.select_first("a.buy").attr("href"),
    }
    for card in doc.select("div.card")
]
# [{'name': 'Widget', 'price': '$9.99', 'url': '/buy/1'}, ...]

# Everything a selector engine should do, without a browser:
doc.select("ul > li:nth-child(odd)")
doc.select_first(".card:not(.featured) a[href^='/buy']")
doc.select_first("div:has(> h2:contains('Gadget'))")

# Navigate from any element.
price = doc.select_first("p.price")
price.parent.attrs          # {'class': 'card featured', 'data-id': '1'}
price.closest(".card")      # nearest matching ancestor (or itself)
price.parent.children       # child elements
```

## Declarative extraction

Describe the data you want as a dataclass, one CSS selector per field, and let
`domtree.extract` fill it in. Values are converted to the field's type, lists
collect every match, nested dataclasses are extracted from each matched element,
and an error names the field and selector that failed.

```python
from dataclasses import dataclass
from typing import Annotated, Optional

import domtree
from domtree import ExtractError, Select, extract, extract_all

@dataclass
class Review:
    author: Annotated[str, Select(".author")]
    rating: Annotated[int, Select(".stars", attr="data-rating")]

@dataclass
class Product:
    __select__ = ".product"                     # optional struct-level selector, used by extract_all
    title: Annotated[str, Select("h1")]                               # readable text (Element.text)
    price: Annotated[float, Select(".price", attr="data-value")]
    image: Annotated[Optional[str], Select("img", attr="src")]        # None when no match / attr missing
    tags: Annotated[list[str], Select(".tag")]                        # every match, document order
    reviews: Annotated[list[Review], Select(".review")]               # nested dataclass per match
    description: Annotated[str, Select(".desc", html=True)]           # inner HTML  (also outer_html=True, markdown=True)
    sku: Annotated[str, Select(attr="data-sku")]                      # no selector -> the scope element itself
    note: Annotated[str, Select(".note", default="")]                 # default when missing
    cents: Annotated[int, Select(".price", with_=lambda s: round(float(s.strip("$")) * 100))]

html = """
<div class="product" data-sku="WID-1">
  <h1>Widget</h1>
  <span class="price" data-value="9.99">$9.99</span>
  <img src="/img/widget.png" alt="Widget">
  <a class="tag">tools</a> <a class="tag">sale</a>
  <div class="desc"><p>A <b>very</b> useful widget.</p></div>
  <div class="review"><span class="author">Ann</span> <span class="stars" data-rating="5">★★★★★</span></div>
  <div class="review"><span class="author">Bob</span> <span class="stars" data-rating="4">★★★★</span></div>
</div>
<div class="product" data-sku="GAD-2">
  <h1>Gadget</h1>
  <span class="price" data-value="19.5">$19.50</span>
  <div class="desc">Does <i>gadget</i> things.</div>
  <p class="note">Back in stock soon</p>
</div>
"""

extract_all(Product, html)          # one Product per `.product`, in document order
# [Product(title='Widget',
#          price=9.99,
#          image='/img/widget.png',
#          tags=['tools', 'sale'],
#          reviews=[Review(author='Ann', rating=5), Review(author='Bob', rating=4)],
#          description='<p>A <b>very</b> useful widget.</p>',
#          sku='WID-1',
#          note='',
#          cents=999),
#  Product(title='Gadget',
#          price=19.5,
#          image=None,
#          tags=[],
#          reviews=[],
#          description='Does <i>gadget</i> things.',
#          sku='GAD-2',
#          note='Back in stock soon',
#          cents=1950)]

extract(Product, html)        # one Product: the first match of __select__
# Product(title='Widget', price=9.99, image='/img/widget.png', tags=['tools', 'sale'], ...)

doc = domtree.parse(html)
extract(Product, doc.select(".product")[1])   # an Element is used as given
# Product(title='Gadget', price=19.5, image=None, tags=[], ...)
```

**Where values come from.** `extract(cls, source)` takes an HTML string (parsed
first), a `Document` or an `Element`. For a string or `Document`, a
`__select__` on the class narrows the scope to its first match (an `Element`
is used as given); that is the scope every selector runs in
(an element's descendants, or the whole document). `extract_all(cls, source)`
returns one `cls` per element matching `cls.__select__`, each scoped to its
element. A field reads the first match's readable text unless it says
`attr="name"`, `html=True` (inner HTML), `outer_html=True` or `markdown=True`.
Without a selector a field reads the scope itself: at document scope that is the
whole document's text, HTML or Markdown (and `attr=` is an error, since a
document has no attributes). `Attr(name)` / `Attr(selector, name)`,
`Html(selector)`, `OuterHtml(selector)` and `Markdown(selector)` are shorthands
for the same `Select` markers.

**Types.** `str` is taken as-is; `int` and `float` parse the trimmed value;
`bool` accepts `true`/`false` in any case; a nested dataclass is extracted with
the matched element as its scope; any other class is called with the string
(`Decimal`, an `Enum` by value, ...). `Optional[T]` is `None` when nothing
matches or the attribute is missing (a value that does not parse is still an
error); `list[T]` is every match (possibly none); anything else is required.
`default=` (or an ordinary dataclass default, `note: Annotated[str, Select(".note")] = ""`)
replaces a missing value, and `with_=` converts the source string yourself, e.g.
`Select("time", attr="datetime", with_=date.fromisoformat)`; its result is used
as-is. Fields without a `Select` marker keep their dataclass default (and need
one). Each class is analysed once and cached; `from __future__ import
annotations` works.

**Errors.** Anything that goes wrong while extracting raises `ExtractError` (a
`ValueError`) naming the field path, the selector and what was read:

```python
bad = domtree.parse(html.replace('data-value="9.99"', 'data-value="$5"'))
try:
    extract(Product, bad.select_first(".product"))
except ExtractError as err:
    print(err)
    # Product.price (".price" @data-value): cannot parse "$5" as float: could not convert string to float: '$5'
    err.path, err.selector, err.source
    # ('Product.price', '.price', '@data-value')

# Others look like:
# Product[1].title ("h1"): no element matches
# Product[1].price (".price" @data-value): <span> has no "data-value" attribute
# Product[0].reviews[1].rating (".stars" @data-rating): cannot parse "four" as int: invalid literal for int() with base 10: 'four'
# Typo.title ("h1:hover"): invalid selector: unsupported pseudo-class ':hover' at byte 2
```

A class that cannot be extracted at all (not a dataclass, a field with neither
a marker nor a default, an unsupported type such as `dict[str, str]`) raises
`TypeError` instead.

## HTML → Markdown for LLMs and RAG

Feeding raw HTML to a model wastes tokens on markup, scripts and navigation.
Pick the content you want and convert it to clean Markdown (CommonMark + GFM:
headings, lists, links, images, code blocks, tables, quotes):

```python
doc = domtree.parse(html)

article = doc.select_first("article") or doc.root
markdown = article.to_markdown()
# # Release notes
#
# Version **2.0** is out. See the [docs](/docs).
#
# - Faster
# - Smaller
#
# | OS | Status |
# | --- | --- |
# | Linux | ok |

prompt = f"Summarize this page:\n\n{markdown}"
```

`<script>`, `<style>` and `<head>` never leak into the output. For plain text
instead (a line per block, a blank line between paragraphs) use `.text` /
`to_text()`.

## API

```python
domtree.parse(html: str) -> Document
```

Never raises on malformed HTML — the parser recovers like a browser would
(implicit `</p>`/`</li>`, stray and mis-nested end tags, …) and records what it
recovered from in `Document.errors`.

| `Document` | |
|---|---|
| `select(selector) -> list[Element]` | every match, in document order |
| `select_first(selector) -> Element \| None` | the first match |
| `to_text()` / `to_markdown()` / `to_html()` | the whole document as text, Markdown or HTML |
| `to_json() -> str` | the tree as JSON (`json.loads` it) |
| `errors -> list[tuple[int, str]]` | `(byte_offset, message)` recovery diagnostics |
| `root -> Element \| None`, `roots -> list[Element]` | the first / all top-level elements |

| `Element` | |
|---|---|
| `tag` | lower-cased tag name |
| `attrs -> dict[str, str]`, `attr(name, default=None)` | attributes (valueless ones are `""`) |
| `text` | readable text (like `to_text()`) |
| `text_content` | raw concatenated text (DOM `textContent`) |
| `html`, `inner_html` | outer / inner HTML |
| `select(sel)`, `select_first(sel)` | matching descendants |
| `closest(sel)`, `matches(sel)` | nearest matching ancestor-or-self / does it match |
| `parent`, `children`, `document` | navigation |
| `to_markdown()`, `to_json()` | this subtree as Markdown / JSON |

| Extraction ([above](#declarative-extraction)) | |
|---|---|
| `extract(cls, source) -> cls` | fill a dataclass from a `str`, `Document` or `Element` |
| `extract_all(cls, source) -> list[cls]` | one per element matching `cls.__select__` |
| `Select(selector=None, *, attr=, html=, outer_html=, markdown=, default=, with_=)` | a field marker: `Annotated[T, Select(...)]` |
| `Attr(name)`, `Attr(selector, name)`, `Html(sel)`, `OuterHtml(sel)`, `Markdown(sel)` | shorthand markers |
| `ExtractError` | a `ValueError` with `.path`, `.selector`, `.source`, `.reason` |

An invalid or unsupported selector raises `ValueError` with the reason and
position (`invalid selector 'a:hover': unsupported pseudo-class ':hover' at
byte 1`). Supported: type, `.class`, `#id`, `*`, attribute selectors
(`[a]`, `=`, `~=`, `|=`, `^=`, `$=`, `*=`, `i` flag), all four combinators,
`:nth-child()`/`:nth-of-type()` and friends, `:first-child`, `:only-child`,
`:empty`, `:root`, `:scope`, `:not()`, `:is()`, `:where()`, `:has()`,
`:contains()`, `:checked`, `:disabled`, … — see the
[main README](https://github.com/iamsaquib8/dom-tree-rs#readme) for the full list.

Elements keep their document alive, compare equal when they are the same node,
and are hashable. Documents and elements are immutable, so they are safe to
share between threads; large inputs are parsed and serialized with the GIL
released, so a thread pool parses pages in parallel.

The package ships type stubs (`py.typed`), so editors and type checkers know
every signature.

## Development

```sh
cd python
python -m venv .venv && . .venv/bin/activate
pip install maturin pytest
maturin develop --release
pytest
```

## License

MIT
