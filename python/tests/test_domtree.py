"""Tests for the `domtree` Python bindings (`pip install dom-tree-rs`)."""

import gc
import json
import random
import re
import threading

import pytest

import domtree

ARTICLE = (
    "<article><h2>News</h2><p>It <em>works</em>!</p>"
    "<script>track()</script></article>"
)

PAGE = """<!doctype html>
<html>
<head><title>Shop</title><style>p { color: red }</style></head>
<body>
  <nav><a href="/">Home</a> <a href="/about">About</a></nav>
  <main id="content">
    <div class="card featured" data-id="1">
      <h2 class="title">Widget</h2>
      <p class="price">$9.99</p>
      <a class="buy" href="/buy/1">Buy</a>
    </div>
    <div class="card" data-id="2">
      <h2 class="title">Gadget</h2>
      <p class="price">$19.99</p>
      <a class="buy" href="/buy/2">Buy</a>
    </div>
    <ul><li>one<li>two<li>three</ul>
  </main>
</body>
</html>"""


@pytest.fixture
def page():
    return domtree.parse(PAGE)


# --- module ------------------------------------------------------------------


def test_version():
    assert re.fullmatch(r"\d+\.\d+\.\d+.*", domtree.__version__)


def test_classes_are_not_constructible():
    with pytest.raises(TypeError):
        domtree.Element()
    with pytest.raises(TypeError):
        domtree.Document()


def test_parse_requires_str():
    with pytest.raises(TypeError):
        domtree.parse(b"<p>bytes</p>")
    with pytest.raises(TypeError):
        domtree.parse(None)


# --- Document ----------------------------------------------------------------


def test_document_serializers():
    doc = domtree.parse(ARTICLE)
    assert doc.to_text() == "News\n\nIt works!"
    assert doc.to_markdown() == "## News\n\nIt *works*!"
    assert doc.to_html() == ARTICLE
    assert str(doc) == ARTICLE


def test_document_to_json():
    data = json.loads(domtree.parse("<p class=a>hi</p>").to_json())
    assert data["type"] == "document"
    (p,) = data["children"]
    assert p["tag"] == "p"
    assert p["attrs"] == {"class": "a"}
    assert p["children"] == [{"type": "text", "value": "hi"}]


def test_root_and_roots(page):
    # The doctype is not an element, so `root` is <html>.
    assert page.root.tag == "html"
    assert [e.tag for e in page.roots] == ["html"]

    frag = domtree.parse("text <b>one</b><!-- c --><i>two</i>")
    assert frag.root.tag == "b"
    assert [e.tag for e in frag.roots] == ["b", "i"]

    empty = domtree.parse("")
    assert empty.root is None
    assert empty.roots == []
    assert empty.to_text() == ""


def test_errors():
    assert domtree.parse("<p>clean</p>").errors == []
    errors = domtree.parse("<div><p>x</span></div>").errors
    assert errors, "expected a recovery diagnostic"
    for pos, msg in errors:
        assert isinstance(pos, int) and pos >= 0
        assert isinstance(msg, str) and msg
    assert any("stray end tag" in msg for _, msg in errors)


def test_repr(page):
    r = repr(page)
    assert r.startswith("<Document roots=[<html>]")
    assert f"errors={len(page.errors)}>" in r
    assert repr(domtree.parse("<p>a</p><p>b</p>")) == "<Document roots=[<p>, <p>] nodes=4 errors=0>"


# --- selectors ---------------------------------------------------------------


def test_select(page):
    titles = page.select(".card .title")
    assert [t.text for t in titles] == ["Widget", "Gadget"]
    assert len(page.select("div.card, ul")) == 3
    assert [li.text for li in page.select("ul > li:nth-child(odd)")] == ["one", "three"]
    assert page.select("table") == []


def test_select_first(page):
    assert page.select_first("a.buy").attr("href") == "/buy/1"
    assert page.select_first(".card:not(.featured) a").attr("href") == "/buy/2"
    assert page.select_first("table") is None


def test_has_and_contains(page):
    assert page.select_first("div:has(> h2:contains('Gadget'))").attr("data-id") == "2"


@pytest.mark.parametrize("bad", ["p)", "a:hover", "p::before", "ul >", "[x", ""])
def test_invalid_selector_raises_value_error(page, bad):
    with pytest.raises(ValueError, match="invalid selector"):
        page.select(bad)
    with pytest.raises(ValueError):
        page.select_first(bad)
    el = page.root
    for method in (el.select, el.select_first, el.closest, el.matches):
        with pytest.raises(ValueError):
            method(bad)


def test_invalid_selector_message_has_position(page):
    with pytest.raises(ValueError) as info:
        page.select("div > p)")
    assert "unexpected ')' at byte 7" in str(info.value)
    with pytest.raises(ValueError, match="unsupported pseudo-class ':hover'"):
        page.select("a:hover")


def test_selector_must_be_str(page):
    with pytest.raises(TypeError):
        page.select(42)


# --- Element -----------------------------------------------------------------


def test_element_basics(page):
    card = page.select_first("div.card")
    assert card.tag == "div"
    assert card.attrs == {"class": "card featured", "data-id": "1"}
    assert card.attr("data-id") == "1"
    assert card.attr("missing") is None
    assert card.attr("missing", "fallback") == "fallback"
    assert card.attr("missing", 0) == 0
    assert card.matches("div.featured")
    assert not card.matches("span")


def test_attrs_is_a_fresh_dict(page):
    card = page.select_first("div.card")
    attrs = card.attrs
    attrs["class"] = "changed"
    assert card.attr("class") == "card featured"


def test_boolean_and_duplicate_attributes():
    el = domtree.parse('<input disabled type="text" type="number">').root
    assert el.attr("disabled") == ""
    assert el.attrs["disabled"] == ""
    # The first of a duplicated attribute wins, in both accessors.
    assert el.attr("type") == "text"
    assert el.attrs["type"] == "text"


def test_text_vs_text_content():
    el = domtree.parse("<div><p>a  b</p><script>x()</script><p>c</p></div>").root
    assert el.text == "a b\n\nc"
    assert el.text_content == "a  bx()c"


def test_html_and_inner_html():
    el = domtree.parse('<ul id="l"><li>a</li><li>b &amp; c</li></ul>').root
    assert el.html == '<ul id="l"><li>a</li><li>b &amp; c</li></ul>'
    assert el.inner_html == "<li>a</li><li>b &amp; c</li>"
    assert str(el) == el.html
    assert el.children[1].text == "b & c"


def test_element_select_is_scoped_to_descendants(page):
    second = page.select("div.card")[1]
    assert [a.attr("href") for a in second.select("a")] == ["/buy/2"]
    assert second.select_first("h2").text == "Gadget"
    ul = page.select_first("ul")
    assert len(ul.select(":scope > li")) == 3


def test_tree_navigation(page):
    price = page.select_first("p.price")
    card = price.parent
    assert card.tag == "div" and card.attr("data-id") == "1"
    assert [c.tag for c in card.children] == ["h2", "p", "a"]
    assert price.closest(".card") == card
    assert price.closest("p") == price  # self-inclusive
    assert price.closest("table") is None
    assert page.root.parent is None


def test_element_markdown_and_json(page):
    assert page.select_first("a.buy").to_markdown() == "[Buy](/buy/1)"
    data = json.loads(page.select_first("h2").to_json())
    assert data["tag"] == "h2"


def test_element_equality_and_hash(page):
    a1 = page.select_first("main")
    a2 = page.select("main")[0]
    assert a1 is not a2
    assert a1 == a2
    assert hash(a1) == hash(a2)
    assert len({a1, a2}) == 1
    assert a1 != page.root
    assert a1 != "main"
    # Same markup, different documents: different nodes.
    other = domtree.parse(PAGE).select_first("main")
    assert other != a1


def test_element_repr():
    el = domtree.parse('<a href="/x" class="btn" hidden>go</a>').root
    assert repr(el) == '<Element <a href="/x" class="btn" hidden>>'
    long = domtree.parse("<p " + " ".join(f'data-a{i}="{i}"' for i in range(50)) + ">").root
    assert len(repr(long)) < 100
    assert repr(long).endswith("…>>")


def test_element_keeps_document_alive():
    def first_link():
        return domtree.parse('<p>see <a href="/x">here</a></p>').select_first("a")

    link = first_link()
    gc.collect()
    assert link.attr("href") == "/x"
    assert link.parent.text == "see here"
    assert link.document.root.tag == "p"
    assert link.document.to_text() == "see here"


def test_elements_outlive_the_document_variable():
    doc = domtree.parse("<p><b>x</b></p>")
    elements = doc.select("*")
    del doc
    gc.collect()
    assert [e.tag for e in elements] == ["p", "b"]
    assert elements[1].parent == elements[0]
    del elements
    gc.collect()


# --- robustness ----------------------------------------------------------------


def test_unicode():
    doc = domtree.parse("<p title='naïve'>héllo 🌍 &amp; 日本語</p>")
    p = doc.root
    assert p.text == "héllo 🌍 & 日本語"
    assert p.attr("title") == "naïve"
    assert doc.select_first("p[title='naïve']") == p


def test_error_offsets_are_utf8_bytes():
    src = "<p>é</span>"
    ((pos, _),) = [e for e in domtree.parse(src).errors if "stray" in e[1]]
    assert src.encode()[pos:].startswith(b"</span>")


def test_never_raises_on_garbage():
    rng = random.Random(1234)
    alphabet = "<>/=\"'!-&;#abcdpliu x\n\té\U0001f600[]()"
    for _ in range(300):
        html = "".join(rng.choice(alphabet) for _ in range(rng.randrange(0, 200)))
        doc = domtree.parse(html)
        doc.to_text()
        doc.to_markdown()
        doc.to_html()
        json.loads(doc.to_json())
        for el in doc.select("*"):
            el.attrs
            el.text


def test_lone_surrogate_is_rejected():
    with pytest.raises(UnicodeEncodeError):
        domtree.parse("<p>\ud800</p>")


def test_deep_nesting_does_not_overflow():
    html = "<div>" * 50_000 + "deep" + "</div>" * 50_000
    doc = domtree.parse(html)
    assert doc.to_text() == "deep"
    assert len(doc.select("div")) == 50_000


def test_large_input_and_threads():
    # Large inputs take the GIL-released path; run several at once.
    rows = "".join(f"<tr><td>{i}</td><td><a href='/r/{i}'>row {i}</a></td></tr>" for i in range(5000))
    html = f"<table>{rows}</table>"
    assert len(html) > 64 * 1024
    results = []

    def work():
        doc = domtree.parse(html)
        results.append((len(doc.select("tr")), len(doc.to_markdown()) > 0))

    threads = [threading.Thread(target=work) for _ in range(4)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    assert results == [(5000, True)] * 4


# --- the README examples ---------------------------------------------------------


def test_readme_scraping_example():
    doc = domtree.parse(PAGE)
    products = [
        {
            "name": card.select_first(".title").text,
            "price": card.select_first(".price").text,
            "url": card.select_first("a.buy").attr("href"),
        }
        for card in doc.select("div.card")
    ]
    assert products == [
        {"name": "Widget", "price": "$9.99", "url": "/buy/1"},
        {"name": "Gadget", "price": "$19.99", "url": "/buy/2"},
    ]


def test_readme_markdown_example():
    html = """
    <html><head><title>Post</title><script>analytics()</script></head>
    <body>
      <nav><a href="/">Home</a></nav>
      <article>
        <h1>Release notes</h1>
        <p>Version <strong>2.0</strong> is out. See the <a href="/docs">docs</a>.</p>
        <ul><li>Faster</li><li>Smaller</li></ul>
        <table><tr><th>OS</th><th>Status</th></tr><tr><td>Linux</td><td>ok</td></tr></table>
      </article>
    </body></html>
    """
    doc = domtree.parse(html)
    md = doc.select_first("article").to_markdown()
    assert md == (
        "# Release notes\n\n"
        "Version **2.0** is out. See the [docs](/docs).\n\n"
        "- Faster\n- Smaller\n\n"
        "| OS | Status |\n| --- | --- |\n| Linux | ok |"
    )
    assert "analytics" not in doc.to_markdown()
