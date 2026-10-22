"""Tests for declarative extraction: `domtree.extract` / `domtree.extract_all`.

Python 3.9 compatible: `typing.Annotated` / `Optional` / `List`, no `X | None`
outside of version-gated tests.
"""

import __future__

import dataclasses
import datetime
import enum
import json
import pickle
import random
import re
import sys
import threading
import types
import typing
from dataclasses import dataclass, field
from decimal import Decimal
from typing import Annotated, Any, List, Optional

import pytest

import domtree
from domtree import Attr, ExtractError, Html, Markdown, OuterHtml, Select, extract, extract_all

# --- the README example --------------------------------------------------------


@dataclass
class Review:
    author: Annotated[str, Select(".author")]
    rating: Annotated[int, Select(".stars", attr="data-rating")]


@dataclass
class Product:
    __select__ = ".product"  # optional struct-level selector, used by extract_all
    title: Annotated[str, Select("h1")]
    price: Annotated[float, Select(".price", attr="data-value")]
    image: Annotated[Optional[str], Select("img", attr="src")]
    tags: Annotated[List[str], Select(".tag")]
    reviews: Annotated[List[Review], Select(".review")]
    description: Annotated[str, Select(".desc", html=True)]
    sku: Annotated[str, Select(attr="data-sku")]
    note: Annotated[str, Select(".note", default="")]
    cents: Annotated[int, Select(".price", with_=lambda s: round(float(s.strip("$")) * 100))]


SHOP = """
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

WIDGET = Product(
    title="Widget",
    price=9.99,
    image="/img/widget.png",
    tags=["tools", "sale"],
    reviews=[Review(author="Ann", rating=5), Review(author="Bob", rating=4)],
    description="<p>A <b>very</b> useful widget.</p>",
    sku="WID-1",
    note="",
    cents=999,
)
GADGET = Product(
    title="Gadget",
    price=19.5,
    image=None,
    tags=[],
    reviews=[],
    description="Does <i>gadget</i> things.",
    sku="GAD-2",
    note="Back in stock soon",
    cents=1950,
)


def test_readme_example():
    assert extract_all(Product, SHOP) == [WIDGET, GADGET]
    doc = domtree.parse(SHOP)
    assert extract(Product, doc.select_first(".product")) == WIDGET
    assert extract(Product, doc.select(".product")[1]) == GADGET
    # On a whole document, __select__ narrows extract() to the first match.
    assert extract(Product, SHOP) == WIDGET
    assert extract(Product, doc) == WIDGET


def test_readme_error_messages():
    def message(source: Any) -> str:
        with pytest.raises(ExtractError) as info:
            if isinstance(source, str):
                extract_all(Product, source)
            else:
                extract(Product, source)
        return str(info.value)

    bad = domtree.parse(SHOP.replace('data-value="9.99"', 'data-value="$5"'))
    assert message(bad.select_first(".product")) == (
        'Product.price (".price" @data-value): cannot parse "$5" as float: '
        "could not convert string to float: '$5'"
    )
    assert message(SHOP.replace("<h1>Gadget</h1>", "")) == 'Product[1].title ("h1"): no element matches'
    assert message(SHOP.replace(' data-value="19.5"', "")) == (
        'Product[1].price (".price" @data-value): <span> has no "data-value" attribute'
    )
    assert message(SHOP.replace('data-rating="4"', 'data-rating="four"')) == (
        'Product[0].reviews[1].rating (".stars" @data-rating): '
        "cannot parse \"four\" as int: invalid literal for int() with base 10: 'four'"
    )

    @dataclass
    class Typo:
        title: Annotated[str, Select("h1:hover")]

    with pytest.raises(ExtractError) as info:
        extract(Typo, SHOP)
    assert str(info.value) == (
        "Typo.title (\"h1:hover\"): invalid selector: unsupported pseudo-class ':hover' at byte 2"
    )


# --- a realistic page ----------------------------------------------------------

PAGE = """<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Acme Kettle 2000 | Acme Store</title>
  <meta property="og:title" content="Acme Kettle 2000">
  <link rel="canonical" href="https://shop.example/p/kettle-2000">
  <script>window.dataLayer = [];</script>
</head>
<body>
  <nav class="breadcrumbs"><a href="/">Home</a> › <a href="/kitchen">Kitchen</a> › <a href="/kitchen/kettles">Kettles</a></nav>
  <main>
    <article class="product" data-sku="KT-2000" data-in-stock="True">
      <h1 class="name">  Acme Kettle 2000  </h1>
      <div class="pricing">
        <span class="price" data-value=" 49.90 " data-currency="EUR">€49.90</span>
        <s class="was">€59.90</s>
      </div>
      <img class="hero" src="/img/kettle.jpg" alt="Acme Kettle">
      <ul class="specs">
        <li><b>Capacity</b> <span class="v" data-n="1.7">1.7 l</span></li>
        <li><b>Power</b> <span class="v" data-n="2200">2200 W</span></li>
      </ul>
      <section class="description">
        <h2>About</h2>
        <p>Boils <em>fast</em>. See the <a href="/manual.pdf">manual</a>.</p>
      </section>
      <section class="reviews" data-count="3">
        <div class="review" id="r1"><span class="author">Ann</span><span class="stars" data-rating="5"></span><p class="body">Great kettle.</p></div>
        <div class="review" id="r2"><span class="author">Bob</span><span class="stars" data-rating="3"></span><p class="body">Loud.</p><time datetime="2026-03-01">March</time></div>
        <div class="review" id="r3"><span class="author">Cy</span><span class="stars" data-rating="4"></span></div>
      </section>
    </article>
    <aside class="related">
      <div class="card"><a href="/p/toaster">Toaster</a> <span class="price" data-value="29">€29</span></div>
      <div class="card"><a href="/p/mug">Mug</a> <span class="price" data-value="7.5">€7.50</span></div>
    </aside>
  </main>
  <footer>© Acme</footer>
</body>
</html>"""


class Currency(enum.Enum):
    EUR = "EUR"
    USD = "USD"


@dataclass
class Spec:
    name: Annotated[str, Select("b")]
    value: Annotated[float, Attr(".v", "data-n")]


@dataclass
class PageReview:
    id: Annotated[str, Attr("id")]
    author: Annotated[str, Select(".author")]
    stars: Annotated[int, Attr(".stars", "data-rating")]
    body: Annotated[Optional[str], Select(".body")]
    date: Annotated[Optional[datetime.date], Attr("time", "datetime", with_=datetime.date.fromisoformat)]


@dataclass
class Card:
    __select__ = ".related .card"
    name: Annotated[str, Select("a")]
    url: Annotated[str, Attr("a", "href")]
    price: Annotated[Decimal, Attr(".price", "data-value")]


@dataclass
class KettlePage:
    og_title: Annotated[str, Attr("meta[property='og:title']", "content")]
    canonical: Annotated[str, Attr("link[rel=canonical]", "href")]
    breadcrumbs: Annotated[List[str], Select(".breadcrumbs a")]
    sku: Annotated[str, Attr(".product", "data-sku")]
    in_stock: Annotated[bool, Attr(".product", "data-in-stock")]
    name: Annotated[str, Select(".product .name")]
    price: Annotated[Decimal, Attr(".price", "data-value", with_=lambda s: Decimal(s.strip()))]
    currency: Annotated[Currency, Attr(".price", "data-currency")]
    was: Annotated[Optional[str], Select(".was")]
    coupon: Annotated[Optional[str], Select(".coupon")]
    image: Annotated[str, Attr("img.hero", "src")]
    specs: Annotated[List[Spec], Select(".specs li")]
    about: Annotated[str, Markdown(".description")]
    review_count: Annotated[int, Attr(".reviews", "data-count")]
    reviews: Annotated[List[PageReview], Select(".review")]
    related: Annotated[List[Card], Select(".related .card")]


def test_realistic_product_page():
    page = extract(KettlePage, PAGE)
    assert page == KettlePage(
        og_title="Acme Kettle 2000",
        canonical="https://shop.example/p/kettle-2000",
        breadcrumbs=["Home", "Kitchen", "Kettles"],
        sku="KT-2000",
        in_stock=True,
        name="Acme Kettle 2000",
        price=Decimal("49.90"),
        currency=Currency.EUR,
        was="€59.90",
        coupon=None,
        image="/img/kettle.jpg",
        specs=[Spec("Capacity", 1.7), Spec("Power", 2200.0)],
        about="## About\n\nBoils *fast*. See the [manual](/manual.pdf).",
        review_count=3,
        reviews=[
            PageReview("r1", "Ann", 5, "Great kettle.", None),
            PageReview("r2", "Bob", 3, "Loud.", datetime.date(2026, 3, 1)),
            PageReview("r3", "Cy", 4, None, None),
        ],
        related=[
            Card("Toaster", "/p/toaster", Decimal("29")),
            Card("Mug", "/p/mug", Decimal("7.5")),
        ],
    )
    doc = domtree.parse(PAGE)
    assert extract(KettlePage, doc) == page
    # Any part of the page can be extracted on its own.
    assert extract(PageReview, doc.select_first("#r2")) == page.reviews[1]


# --- extract_all ---------------------------------------------------------------


def test_extract_all_over_cards():
    doc = domtree.parse(PAGE)
    expected = [Card("Toaster", "/p/toaster", Decimal("29")), Card("Mug", "/p/mug", Decimal("7.5"))]
    assert extract_all(Card, PAGE) == expected
    assert extract_all(Card, doc) == expected
    # From an element: only its descendants.
    assert extract_all(Card, doc.select_first("aside")) == expected
    assert extract_all(Card, doc.select_first("article")) == []
    assert extract_all(Card, "<p>no cards</p>") == []


def test_extract_all_reports_the_item_index():
    html = PAGE.replace('data-value="7.5"', 'data-value="free"')
    with pytest.raises(ExtractError) as info:
        extract_all(Card, html)
    err = info.value
    assert err.path == "Card[1].price"
    assert str(err).startswith('Card[1].price (".price" @data-value): cannot parse "free" as Decimal: ')


def test_extract_all_requires_select():
    with pytest.raises(TypeError, match=r"extract_all\(\) needs Review.__select__"):
        extract_all(Review, SHOP)


def test_select_is_inherited():
    @dataclass
    class FeaturedCard(Card):
        pass

    assert [c.name for c in extract_all(FeaturedCard, PAGE)] == ["Toaster", "Mug"]


# --- nesting -------------------------------------------------------------------


@dataclass
class Money:
    amount: Annotated[Decimal, Attr("data-amount")]
    currency: Annotated[str, Attr("data-currency")]


@dataclass
class Line:
    sku: Annotated[str, Attr("data-sku")]
    qty: Annotated[int, Select(".qty")]
    price: Annotated[Money, Select(".money")]


@dataclass
class Customer:
    name: Annotated[str, Select(".name")]
    email: Annotated[Optional[str], Attr("a[href^='mailto:']", "href", with_=lambda s: s[len("mailto:") :])]


@dataclass
class Order:
    id: Annotated[str, Attr(".order", "data-id")]
    customer: Annotated[Customer, Select(".customer")]
    lines: Annotated[List[Line], Select(".line")]
    total: Annotated[Money, Select(".total .money")]
    gift: Annotated[Optional[Customer], Select(".gift-to")]


ORDER = """
<div class="order" data-id="A-17">
  <div class="customer"><span class="name">Dana</span> <a href="mailto:dana@example.com">mail</a></div>
  <table>
    <tr class="line" data-sku="KT"><td class="qty"> 2 </td><td class="money" data-amount="49.90" data-currency="EUR"></td></tr>
    <tr class="line" data-sku="MG"><td class="qty">4</td><td class="money" data-amount="7.50" data-currency="EUR"></td></tr>
  </table>
  <p class="total"><span class="money" data-amount="129.80" data-currency="EUR"></span></p>
</div>
"""


def test_nested_and_lists_of_nested():
    order = extract(Order, ORDER)
    assert order == Order(
        id="A-17",
        customer=Customer("Dana", "dana@example.com"),
        lines=[
            Line("KT", 2, Money(Decimal("49.90"), "EUR")),
            Line("MG", 4, Money(Decimal("7.50"), "EUR")),
        ],
        total=Money(Decimal("129.80"), "EUR"),
        gift=None,
    )


def test_nested_error_paths():
    bad = ORDER.replace('data-amount="7.50"', 'data-amount="7,50"')
    with pytest.raises(ExtractError) as info:
        extract(Order, bad)
    assert info.value.path == "Order.lines[1].price.amount"
    assert info.value.selector is None
    assert info.value.source == "@data-amount"

    with pytest.raises(ExtractError) as info:
        extract(Order, ORDER.replace('class="customer"', 'class="client"'))
    assert str(info.value) == 'Order.customer (".customer"): no element matches'
    assert info.value.source == "element"

    with pytest.raises(ExtractError) as info:
        extract(Order, ORDER.replace('<span class="name">Dana</span>', ""))
    assert str(info.value) == 'Order.customer.name (".name"): no element matches'


def test_spec_path_example():
    reviews = "".join(
        f'<div class="review"><span class="author">a{i}</span><span class="stars" data-rating="{r}"></span></div>'
        for i, r in enumerate(["5", "4", "x"])
    )
    html = (
        '<div class="product" data-sku="S"><h1>T</h1><span class="price" data-value="1">$1</span>'
        f'<div class="desc"></div>{reviews}</div>'
    )
    with pytest.raises(ExtractError) as info:
        extract_all(Product, html)
    assert info.value.path == "Product[0].reviews[2].rating"
    with pytest.raises(ExtractError) as info:
        extract(Product, domtree.parse(html).root)
    assert str(info.value) == (
        'Product.reviews[2].rating (".stars" @data-rating): '
        "cannot parse \"x\" as int: invalid literal for int() with base 10: 'x'"
    )


@dataclass
class Comment:
    author: Annotated[str, Select(":scope > .author")]
    replies: Annotated[List["Comment"], Select(":scope > ul > li.comment")]


def test_recursive_dataclass():
    html = """
    <li class="comment"><span class="author">a</span>
      <ul>
        <li class="comment"><span class="author">b</span><ul></ul></li>
        <li class="comment"><span class="author">c</span>
          <ul><li class="comment"><span class="author">d</span></li></ul>
        </li>
      </ul>
    </li>"""
    root = extract(Comment, domtree.parse(html).root)
    assert root == Comment("a", [Comment("b", []), Comment("c", [Comment("d", [])])])


# --- value sources -----------------------------------------------------------------

SNIPPET = '<div id="s" class="box" data-n="7"><p>Hi <b>there</b></p><ul><li>a</li><li>b</li></ul></div>'


def test_every_source_kind_on_an_element():
    @dataclass
    class Kinds:
        text: Annotated[str, Select("p")]
        attr: Annotated[str, Select(attr="id")]
        html: Annotated[str, Select("p", html=True)]
        outer: Annotated[str, Select("p", outer_html=True)]
        md: Annotated[str, Select("ul", markdown=True)]
        own_text: Annotated[str, Select()]
        own_html: Annotated[str, Html()]
        own_outer: Annotated[str, OuterHtml()]
        own_md: Annotated[str, Markdown()]
        # The shorthands are the same markers.
        attr2: Annotated[str, Attr("id")]
        html2: Annotated[str, Html("p")]
        outer2: Annotated[str, OuterHtml("p")]
        md2: Annotated[str, Markdown("ul")]
        n: Annotated[int, Attr("data-n")]

    box = domtree.parse(SNIPPET).root
    k = extract(Kinds, box)
    assert k.text == "Hi there"
    assert k.attr == k.attr2 == "s"
    assert k.html == k.html2 == "Hi <b>there</b>"
    assert k.outer == k.outer2 == "<p>Hi <b>there</b></p>"
    assert k.md == k.md2 == "- a\n- b"
    assert k.own_text == box.text == "Hi there\n\na\nb"
    assert k.own_html == box.inner_html
    assert k.own_outer == box.html == SNIPPET
    assert k.own_md == box.to_markdown()
    assert k.n == 7


def test_every_source_kind_on_a_document():
    @dataclass
    class Whole:
        text: Annotated[str, Select()]
        html: Annotated[str, Html()]
        outer: Annotated[str, OuterHtml()]
        md: Annotated[str, Markdown()]
        first_li: Annotated[str, Select("li")]

    doc = domtree.parse(SNIPPET)
    w = extract(Whole, doc)
    assert w == Whole(doc.to_text(), doc.to_html(), doc.to_html(), doc.to_markdown(), "a")
    assert extract(Whole, SNIPPET) == w


def test_attr_without_selector_needs_an_element():
    @dataclass
    class Id:
        id: Annotated[Optional[str], Attr("id")]

    assert extract(Id, domtree.parse(SNIPPET).root) == Id("s")
    with pytest.raises(ExtractError) as info:
        extract(Id, SNIPPET)  # even an Optional field: this is a usage error
    assert info.value.path == "Id.id"
    assert info.value.selector is None
    assert info.value.source == "@id"
    with pytest.raises(ExtractError):
        extract(Id, domtree.parse(SNIPPET))


def test_sources_str_document_element():
    @dataclass
    class Items:
        items: Annotated[List[str], Select("li")]

    doc = domtree.parse("<ul id=a><li>1</li></ul><ul id=b><li>2</li><li>3</li></ul>")
    assert extract(Items, "<ul><li>1</li></ul><ol><li>2</li></ol>") == Items(["1", "2"])
    assert extract(Items, doc) == Items(["1", "2", "3"])
    assert extract(Items, doc.select_first("#b")) == Items(["2", "3"])
    assert extract(Items, "") == Items([])
    for bad in (b"<li>x</li>", None, 42, ["<li>"]):
        with pytest.raises(TypeError, match="source must be a str, Document or Element"):
            extract(Items, bad)
        with pytest.raises(TypeError):
            extract_all(Card, bad)


def test_element_scope_is_descendants_only():
    @dataclass
    class Inner:
        title: Annotated[Optional[str], Select("h2")]
        div: Annotated[Optional[str], Select("div")]

    doc = domtree.parse("<h2>outside</h2><div><p>in</p></div>")
    assert extract(Inner, doc.select_first("div")) == Inner(None, None)
    assert extract(Inner, doc) == Inner("outside", "in")


# --- value conversion ---------------------------------------------------------------


class Size(enum.Enum):
    S = "small"
    L = "large"


class Slug:
    def __init__(self, value: str) -> None:
        self.value = value.strip().lower().replace(" ", "-")

    def __eq__(self, other: object) -> bool:
        return isinstance(other, Slug) and other.value == self.value


def test_value_conversion():
    @dataclass
    class Values:
        s: Annotated[str, Attr(".v", "data-s")]
        i: Annotated[int, Attr(".v", "data-i")]
        neg: Annotated[int, Select(".neg")]
        f: Annotated[float, Attr(".v", "data-f")]
        exp: Annotated[float, Attr(".v", "data-e")]
        yes: Annotated[bool, Attr(".v", "data-yes")]
        no: Annotated[bool, Attr(".v", "data-no")]
        d: Annotated[Decimal, Attr(".v", "data-f")]
        size: Annotated[Size, Attr(".v", "data-size")]
        slug: Annotated[Slug, Select(".title")]
        when: Annotated[datetime.date, Attr(".v", "data-date", with_=datetime.date.fromisoformat)]

    html = (
        '<p class="v" data-s=" keep  spaces " data-i=" 42 " data-f="\n2.50\t" data-e="1e3" '
        'data-yes=" TRUE " data-no="False" data-size="large" data-date="2026-10-22"></p>'
        '<p class="neg">-3</p><h1 class="title">Hello World</h1>'
    )
    v = extract(Values, html)
    assert v == Values(
        s=" keep  spaces ",  # str is taken as-is
        i=42,
        neg=-3,
        f=2.5,
        exp=1000.0,
        yes=True,
        no=False,
        d=Decimal("2.50"),
        size=Size.L,
        slug=Slug("hello-world"),
        when=datetime.date(2026, 10, 22),
    )
    assert type(v.i) is int and type(v.yes) is bool and type(v.f) is float


@pytest.mark.parametrize(
    "tp, raw, message",
    [
        (int, "4.5", "cannot parse \"4.5\" as int: invalid literal for int() with base 10: '4.5'"),
        (int, " 1 2 ", "cannot parse \"1 2\" as int: invalid literal for int() with base 10: '1 2'"),
        (float, "$5", "cannot parse \"$5\" as float: could not convert string to float: '$5'"),
        (float, "", "cannot parse \"\" as float: could not convert string to float: ''"),
        (bool, "yes", 'cannot parse "yes" as bool: expected "true" or "false"'),
        (bool, "1", 'cannot parse "1" as bool: expected "true" or "false"'),
        (Size, "medium", "cannot parse \"medium\" as Size: 'medium' is not a valid Size"),
    ],
)
def test_parse_error_messages(tp, raw, message):
    @dataclass
    class One:
        value: Annotated[tp, Attr("p", "data-v")]

    with pytest.raises(ExtractError) as info:
        extract(One, f'<p data-v="{raw}">')
    err = info.value
    assert str(err) == f'One.value ("p" @data-v): {message}'
    assert err.reason == message
    assert isinstance(err.__cause__, ValueError)


def test_parse_error_quotes_like_rust():
    @dataclass
    class One:
        n: Annotated[int, Select("p")]

    with pytest.raises(ExtractError) as info:
        extract(One, '<p>"quoted"\\ é</p>')
    assert info.value.reason.startswith('cannot parse "\\"quoted\\"\\\\ é" as int: ')


# --- Optional / default / with_ --------------------------------------------------------


def test_optional():
    @dataclass
    class Opt:
        missing_el: Annotated[Optional[str], Select(".nope")]
        missing_attr: Annotated[Optional[str], Attr("p", "href")]
        present: Annotated[Optional[int], Attr("p", "data-n")]
        typing_form: Optional[Annotated[str, Select("p")]]
        nested: Annotated[Optional[Money], Select(".money")]

    assert extract(Opt, '<p data-n="3">x</p>') == Opt(None, None, 3, "x", None)

    # A parse error is still an error, Optional or not.
    with pytest.raises(ExtractError, match=r'Opt.present \("p" @data-n\): cannot parse "three" as int'):
        extract(Opt, '<p data-n="three">x</p>')


@pytest.mark.skipif(sys.version_info < (3, 10), reason="`X | None` needs Python 3.10")
def test_pipe_none_optional():
    @dataclass
    class Pipe:
        a: Annotated[int | None, Select("a")]
        b: Annotated[str | None, Select("b")]

    assert extract(Pipe, "<a>1</a>") == Pipe(1, None)


def test_list():
    @dataclass
    class Lists:
        words: Annotated[List[str], Select("li")]
        numbers: Annotated[list[int], Attr("li", "data-n")]
        none: Annotated[List[str], Select("dt")]
        own: Annotated[List[str], Select()]

    html = '<ul><li data-n="1">a</li><li data-n=" 2 ">b</li><li data-n="3">c</li></ul>'
    ul = domtree.parse(html).root
    assert extract(Lists, ul) == Lists(["a", "b", "c"], [1, 2, 3], [], ["a\nb\nc"])

    with pytest.raises(ExtractError) as info:
        extract(Lists, html.replace(' data-n=" 2 "', ""))
    assert str(info.value) == 'Lists.numbers[1] ("li" @data-n): <li> has no "data-n" attribute'

    with pytest.raises(ExtractError) as info:
        extract(Lists, html.replace('data-n="3"', 'data-n="x"'))
    assert info.value.path == "Lists.numbers[2]"


def test_default():
    @dataclass
    class Defaults:
        el: Annotated[str, Select(".nope", default="n/a")]
        attr: Annotated[int, Attr("p", "data-n", default=0)]
        none: Annotated[str, Select(".nope", default=None)]  # used as-is
        opt: Annotated[Optional[str], Select(".nope", default="dflt")]
        items: Annotated[List[str], Select("li", default=["-"])]
        own: Annotated[str, Select(".nope")] = "dataclass default"
        own_list: Annotated[List[str], Select(".nope")] = field(default_factory=lambda: ["factory"])

    assert extract(Defaults, "<p>x</p>") == Defaults("n/a", 0, None, "dflt", ["-"])
    assert extract(Defaults, "<p>x</p>").own_list == ["factory"]
    assert extract(Defaults, '<p class="nope" data-n="5">y</p><li>z</li>') == Defaults(
        "y", 5, "y", "y", ["z"], "y", ["y"]
    )
    # Defaults cover missing values only, never parse errors.
    with pytest.raises(ExtractError, match="cannot parse"):
        extract(Defaults, '<p data-n="five">x</p>')


def parse_cents(s: str) -> int:
    return round(float(s.strip().lstrip("$")) * 100)


def test_with():
    calls = []

    def spy(s: str) -> Any:
        calls.append(s)
        return {"raw": s}

    @dataclass
    class With:
        cents: Annotated[int, Select(".price", with_=parse_cents)]
        anything: Annotated[str, Select(".price", with_=spy)]  # result used as-is
        each: Annotated[List[int], Attr("li", "data-n", with_=lambda s: int(s) * 10)]
        opt: Annotated[Optional[int], Select(".nope", with_=spy)]
        dflt: Annotated[int, Select(".nope", with_=spy, default=-1)]
        size: Annotated[Size, Select(".size", with_=lambda s: Size[s])]

    html = '<p class="price">$1.25</p><li data-n="1"></li><li data-n="2"></li><i class="size">S</i>'
    w = extract(With, html)
    assert w == With(125, {"raw": "$1.25"}, [10, 20], None, -1, Size.S)
    assert calls == ["$1.25"]  # not called for missing values


def test_with_errors_are_wrapped():
    @dataclass
    class Cents:
        cents: Annotated[int, Select(".price", with_=parse_cents)]
        size: Annotated[Optional[Size], Select(".size", with_=lambda s: Size[s])]

    with pytest.raises(ExtractError) as info:
        extract(Cents, '<p class="price">$x</p>')
    assert str(info.value) == (
        'Cents.cents (".price"): cannot convert "$x" with parse_cents: '
        "could not convert string to float: 'x'"
    )
    assert isinstance(info.value.__cause__, ValueError)

    # Any exception from the converter (here a KeyError) becomes an ExtractError.
    with pytest.raises(ExtractError) as info:
        extract(Cents, '<p class="price">$1</p><i class="size">XL</i>')
    assert info.value.path == "Cents.size"
    assert info.value.reason.startswith('cannot convert "XL" with ')
    assert info.value.reason.endswith(": 'XL'")
    assert isinstance(info.value.__cause__, KeyError)


# --- dataclass fields -------------------------------------------------------------------


def test_unmarked_fields():
    @dataclass
    class Partly:
        title: Annotated[str, Select("h1")]
        seen: int = 0
        tags: List[str] = field(default_factory=list)
        computed: str = field(init=False, default="x")
        note: Annotated[str, "not a Select marker"] = "plain"

    assert extract(Partly, "<h1>T</h1>") == Partly("T")

    @dataclass
    class Unmarked:
        title: Annotated[str, Select("h1")]
        count: int

    with pytest.raises(TypeError) as info:
        extract(Unmarked, "<h1>T</h1>")
    assert str(info.value) == (
        "Unmarked.count has no Select(...) marker: "
        "annotate it as Annotated[int, Select(...)] or give it a default"
    )


def test_frozen_dataclass():
    @dataclass(frozen=True)
    class Frozen:
        a: Annotated[str, Select("a")]

    assert extract(Frozen, "<a>x</a>") == Frozen("x")


@pytest.mark.parametrize(
    "annotation, message",
    [
        (Annotated[str, Select], "use Select(...) (an instance), not the Select class"),
        (Annotated[str, Select("a"), Select("b")], "more than one Select(...) marker"),
        (Annotated[list, Select("a")], "give the list an item type"),
        (Annotated[List, Select("a")], "give the list an item type"),
        (Annotated[Optional[List[str]], Select("a")], "Optional[list[...]] is not supported"),
        (Annotated[List[Optional[str]], Select("a")], "unsupported item type"),
        (Annotated[typing.Dict[str, str], Select("a")], "unsupported field type"),
        (Annotated[Any, Select("a")], "unsupported field type"),
        (Annotated[typing.Union[int, str], Select("a")], "unsupported field type"),
        (
            Annotated[Money, Select("a", attr="href")],
            "a nested dataclass is extracted from the matched element",
        ),
        (Annotated[Money, Html("a")], "a nested dataclass"),
    ],
)
def test_definition_errors(annotation, message):
    Bad = dataclasses.make_dataclass("Bad", [("x", annotation)])
    with pytest.raises(TypeError, match=re.escape(message)) as info:
        extract(Bad, "<a>1</a>")
    assert str(info.value).startswith("Bad.x")
    # Never cached: it fails the same way every time.
    with pytest.raises(TypeError):
        extract(Bad, "<a>1</a>")


def test_init_false_field_with_marker():
    @dataclass
    class NoInit:
        x: Annotated[str, Select("a")] = field(init=False, default="")

    with pytest.raises(TypeError, match="init=False"):
        extract(NoInit, "<a>1</a>")


def test_not_a_dataclass():
    class Plain:
        x: Annotated[str, Select("a")]

    for bad in (Plain, int, WIDGET, None):
        with pytest.raises(TypeError, match="expected a dataclass type"):
            extract(bad, "<a>")
        with pytest.raises(TypeError, match="expected a dataclass type"):
            extract_all(bad, "<a>")


def test_select_arguments_are_checked():
    with pytest.raises(TypeError, match="mutually exclusive"):
        Select("a", attr="href", html=True)
    with pytest.raises(TypeError, match="mutually exclusive"):
        Select("a", markdown=True, outer_html=True)
    with pytest.raises(TypeError, match="selector must be a str"):
        Select(3)
    with pytest.raises(TypeError, match="attr must be a str"):
        Select("a", attr=1)
    with pytest.raises(TypeError, match="with_ must be callable"):
        Select("a", with_="int")
    with pytest.raises(TypeError, match="attribute name must be a str"):
        Attr(".a", None)
    with pytest.raises(TypeError):
        Attr()

    @dataclass
    class BadSelect:
        __select__ = 3
        x: Annotated[str, Select("a")]

    with pytest.raises(TypeError, match="BadSelect.__select__ must be a str"):
        extract_all(BadSelect, "<a>")


def test_markers():
    assert repr(Select(".a", attr="b", default=1)) == "Select('.a', attr='b', default=1)"
    assert repr(Select()) == "Select()"
    assert repr(Html("p")) == "Select('p', html=True)"
    a = Attr("href")
    assert (a.selector, a.attr) == (None, "href")
    a = Attr(".x", "href", default="")
    assert (a.selector, a.attr, a.default) == (".x", "href", "")
    a = Attr(None, "href")
    assert (a.selector, a.attr) == (None, "href")
    assert Select().default is dataclasses.MISSING
    assert isinstance(Markdown(), Select) and Markdown().markdown
    assert OuterHtml("p").outer_html and not OuterHtml("p").html


# --- errors -----------------------------------------------------------------------


def test_missing_values_messages():
    @dataclass
    class Need:
        title: Annotated[str, Select("h1")]
        link: Annotated[str, Attr("a", "href")]

    with pytest.raises(ExtractError) as info:
        extract(Need, "<a href='/'>x</a>")
    err = info.value
    assert str(err) == 'Need.title ("h1"): no element matches'
    assert (err.path, err.selector, err.source, err.reason) == (
        "Need.title",
        "h1",
        "text",
        "no element matches",
    )

    with pytest.raises(ExtractError) as info:
        extract(Need, "<h1>T</h1><a>x</a>")
    assert str(info.value) == 'Need.link ("a" @href): <a> has no "href" attribute'

    with pytest.raises(ExtractError) as info:
        extract(Need, "<h1>T</h1>")
    assert str(info.value) == 'Need.link ("a" @href): no element matches'


@pytest.mark.parametrize(
    "marker, source",
    [
        (Select(".x"), "text"),
        (Html(".x"), "html"),
        (OuterHtml(".x"), "outer_html"),
        (Markdown(".x"), "markdown"),
        (Attr(".x", "href"), "@href"),
    ],
)
def test_message_names_the_source_kind(marker, source):
    Kind = dataclasses.make_dataclass("Kind", [("v", Annotated[str, marker])])
    with pytest.raises(ExtractError) as info:
        extract(Kind, "<p>")
    where = '".x"' if source == "text" else f'".x" {source}'
    assert str(info.value) == f"Kind.v ({where}): no element matches"
    assert info.value.source == source


def test_extract_error_is_a_value_error_and_pickles():
    with pytest.raises(ValueError):
        extract(Review, "<p>")
    err = ExtractError("Product.price", ".price", "@data-value", 'cannot parse "$5" as float: x')
    assert str(err) == 'Product.price (".price" @data-value): cannot parse "$5" as float: x'
    again = pickle.loads(pickle.dumps(err))
    assert type(again) is ExtractError
    assert (again.path, again.selector, again.source, again.reason) == (
        err.path,
        err.selector,
        err.source,
        err.reason,
    )
    assert str(again) == str(err)
    assert str(ExtractError("A.b", None, "text", "r")) == "A.b (text): r"
    assert ExtractError.__module__ == "domtree"


@pytest.mark.parametrize("bad", ["a:hover", "p)", "", "[x"])
def test_invalid_selector(bad):
    @dataclass
    class BadSel:
        ok: Annotated[Optional[str], Select("p")]
        link: Annotated[Optional[str], Select(bad, attr="href")]

    with pytest.raises(ValueError) as native:
        domtree.parse("").select(bad)

    for _ in range(2):  # not cached: raised on every call
        with pytest.raises(ExtractError) as info:
            extract(BadSel, "")  # raised even though nothing would match
        err = info.value
        assert isinstance(err, ValueError)
        assert (err.path, err.selector, err.source) == ("BadSel.link", bad, "@href")
        # Same reason as the native error, minus the selector (already in the parentheses).
        expected = str(native.value).replace(f"invalid selector '{bad}': ", "invalid selector: ", 1)
        assert err.reason == expected
        assert str(err) == f"BadSel.link ({json.dumps(bad)} @href): {expected}"


def test_invalid_selector_exact_message():
    @dataclass
    class Hover:
        link: Annotated[str, Select("a:hover")]

    with pytest.raises(ExtractError) as info:
        extract(Hover, "<a>")
    assert str(info.value) == (
        "Hover.link (\"a:hover\"): invalid selector: unsupported pseudo-class ':hover' at byte 1"
    )
    assert isinstance(info.value.__cause__, ValueError)

    @dataclass
    class BadAll:
        __select__ = "div >"
        x: Annotated[str, Select()]

    with pytest.raises(ExtractError) as info:
        extract_all(BadAll, "<div>")
    assert info.value.path == "BadAll"
    assert info.value.selector == "div >"
    assert str(info.value).startswith("BadAll (\"div >\"): invalid selector: ")


# --- `from __future__ import annotations` -----------------------------------------

FUTURE_MODELS = """
from __future__ import annotations

from dataclasses import dataclass
from typing import Annotated, List, Optional

from domtree import Attr, Select


@dataclass
class Author:
    name: Annotated[str, Select(".name")]
    url: Annotated[Optional[str], Attr("a", "href")]


@dataclass
class Post:
    __select__ = "article"
    title: Annotated[str, Select("h2")]
    votes: Annotated[int, Attr("data-votes")]
    author: Annotated[Author, Select(".by")]
    tags: Annotated[List[str], Select(".tag")]
    editor: Annotated[Optional[Author], Select(".edited-by")]


def local_classes():
    @dataclass
    class Inner:
        x: Annotated[str, Select("p")]

    @dataclass
    class Outer:
        inner: Annotated[Inner, Select("div")]

    return Outer
"""


@pytest.fixture
def future_models():
    name = "_domtree_future_models"
    module = types.ModuleType(name)
    code = compile(
        FUTURE_MODELS, f"<{name}>", "exec", flags=__future__.annotations.compiler_flag, dont_inherit=True
    )
    sys.modules[name] = module
    try:
        exec(code, module.__dict__)
        yield module
    finally:
        del sys.modules[name]


def test_future_annotations(future_models):
    m = future_models
    assert isinstance(m.Post.__dataclass_fields__["title"].type, str)  # stringified
    html = """
    <article data-votes="3"><h2>One</h2><p class="by"><span class="name">Ann</span><a href="/ann">@ann</a></p>
      <a class="tag">x</a><a class="tag">y</a></article>
    <article data-votes="0"><h2>Two</h2><p class="by"><span class="name">Bob</span></p>
      <p class="edited-by"><span class="name">Cy</span></p></article>
    """
    assert extract_all(m.Post, html) == [
        m.Post("One", 3, m.Author("Ann", "/ann"), ["x", "y"], None),
        m.Post("Two", 0, m.Author("Bob", None), [], m.Author("Cy", None)),
    ]

    # Classes local to a function can't be resolved from stringified annotations.
    with pytest.raises(
        TypeError, match="cannot resolve the type hints of Outer: name 'Inner' is not defined"
    ):
        extract(m.local_classes(), "<div><p>x</p></div>")


# --- caching and threads ------------------------------------------------------------


def test_plans_are_cached(monkeypatch):
    @dataclass
    class Leaf:
        v: Annotated[str, Select("i")]

    @dataclass
    class Tree:
        leaves: Annotated[List[Leaf], Select("b")]
        first: Annotated[Leaf, Select("b")]

    calls = []
    real = typing.get_type_hints

    def counting(obj: Any, *args: Any, **kwargs: Any) -> Any:
        calls.append(obj)
        return real(obj, *args, **kwargs)

    monkeypatch.setattr(typing, "get_type_hints", counting)
    html = "<b><i>1</i></b><b><i>2</i></b>"
    for _ in range(3):
        assert extract(Tree, html) == Tree([Leaf("1"), Leaf("2")], Leaf("1"))
        assert extract(Leaf, "<i>x</i>") == Leaf("x")
    assert sorted(c.__name__ for c in calls) == ["Leaf", "Tree"]


def test_concurrent_first_use():
    @dataclass
    class Fresh:
        a: Annotated[List[int], Select("a")]

    html = "".join(f"<a>{i}</a>" for i in range(50))
    results: List[Any] = []
    barrier = threading.Barrier(8)

    def work() -> None:
        barrier.wait()
        results.append(extract(Fresh, html))

    threads = [threading.Thread(target=work) for _ in range(8)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    assert results == [Fresh(list(range(50)))] * 8


# --- robustness ----------------------------------------------------------------------


@dataclass
class FuzzItem:
    __select__ = ".item, li, [data-n]"
    n: Annotated[int, Attr("data-n")]
    label: Annotated[Optional[str], Select("b")]


@dataclass
class Fuzz:
    __select__ = "div, section"
    title: Annotated[Optional[str], Select("h1, h2")]
    n: Annotated[Optional[float], Attr("[data-n]", "data-n")]
    flag: Annotated[bool, Select(".flag", attr="data-on", default=False)]
    words: Annotated[List[str], Select("b, i")]
    numbers: Annotated[List[int], Attr("[data-n]", "data-n")]
    items: Annotated[List[FuzzItem], Select(".item")]
    first: Annotated[Optional[FuzzItem], Select("li")]
    md: Annotated[str, Markdown()]
    html: Annotated[str, OuterHtml()]
    size: Annotated[Optional[Size], Select(".size")]
    cents: Annotated[Optional[int], Select(".price", with_=parse_cents)]


def random_html(rng: random.Random) -> str:
    tags = ["div", "section", "p", "b", "i", "h1", "h2", "ul", "li", "span", "a", "table", "tr", "td"]
    classes = ["item", "flag", "size", "price", "x", ""]
    values = [
        "1",
        "-2",
        " 3 ",
        "4.5",
        "true",
        "FALSE",
        "small",
        "large",
        "$1.25",
        "$",
        "",
        "nan",
        "é",
        "1" * 30,
    ]
    out = []
    for _ in range(rng.randrange(0, 40)):
        roll = rng.random()
        if roll < 0.45:
            tag = rng.choice(tags)
            attrs = ""
            if rng.random() < 0.5:
                attrs += f' class="{rng.choice(classes)}"'
            if rng.random() < 0.4:
                attrs += f' data-n="{rng.choice(values)}"'
            if rng.random() < 0.3:
                attrs += f' data-on="{rng.choice(values)}"'
            out.append(f"<{tag}{attrs}>")
        elif roll < 0.75:
            out.append(f"</{rng.choice(tags)}>")
        elif roll < 0.95:
            out.append(rng.choice(values + ["<", "&amp;", "&", "'", '"', "<!--", "-->"]))
        else:
            out.append(rng.choice(["<br>", "<img src=x>", "<script>x<y</script>", "<!doctype html>"]))
    return "".join(out)


def test_random_html_only_raises_extract_error():
    rng = random.Random(4)
    outcomes = {"ok": 0, "error": 0}

    def attempt(fn: Any, cls: Any, src: Any) -> None:
        try:
            result = fn(cls, src)
        except ExtractError as err:
            assert err.path.startswith(cls.__name__)
            assert str(err).startswith(err.path + " (")
            outcomes["error"] += 1
        else:
            assert isinstance(result, (cls, list))
            outcomes["ok"] += 1

    for _ in range(200):
        html = random_html(rng)
        doc = domtree.parse(html)
        attempt(extract, Fuzz, html)
        attempt(extract, Fuzz, doc)
        attempt(extract_all, Fuzz, html)
        attempt(extract_all, FuzzItem, doc)
        for el in doc.select("div, li")[:3]:
            attempt(extract, Fuzz, el)
            attempt(extract, FuzzItem, el)
    # Both paths are exercised.
    assert outcomes["ok"] > 100 and outcomes["error"] > 100, outcomes


# --- extract() honours __select__ on documents (same as Rust's Dom::extract) ---


@dataclass
class _SelCard:
    __select__ = "li.card"
    name: Annotated[str, Select("h2")]


def test_extract_uses_select_on_documents_and_strings() -> None:
    page = "<ul><li class=card><h2>Ann</h2></li><li class=card><h2>Bob</h2></li></ul>"
    assert extract(_SelCard, page) == _SelCard(name="Ann")
    assert extract(_SelCard, domtree.parse(page)) == _SelCard(name="Ann")
    # An Element is used as given: __select__ does not apply.
    second = domtree.parse(page).select("li.card")[1]
    assert extract(_SelCard, second) == _SelCard(name="Bob")


def test_extract_select_no_match_matches_rust_message() -> None:
    with pytest.raises(ExtractError) as info:
        extract(_SelCard, "<p>nothing</p>")
    assert str(info.value) == 'Card ("li.card"): no element matches'.replace("Card", "_SelCard")
