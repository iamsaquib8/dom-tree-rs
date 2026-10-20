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
