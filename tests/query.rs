//! Tests for the CSS selector engine.
#![cfg(feature = "query")]

use std::time::{Duration, Instant};

use domq::{AsSelector, Dom, NodeRef, Selector, parse};

/// The `id` of every node, in order (`"?"` when it has none).
fn ids<'a>(nodes: impl Iterator<Item = NodeRef<'a>>) -> Vec<&'a str> {
    nodes.map(|n| n.attr("id").unwrap_or("?")).collect()
}

/// The ids of every element in `dom` matching `selector`.
fn sel<'a>(dom: &'a Dom, selector: &str) -> Vec<&'a str> {
    ids(dom.select(selector))
}

/// `(position, message)` of the error for an invalid selector.
fn err(selector: &str) -> (usize, String) {
    let e = Selector::parse(selector).unwrap_err();
    (e.position(), e.message().to_string())
}

const NONE: [&str; 0] = [];

// --- basics --------------------------------------------------------------

#[test]
fn selectors() {
    let dom = parse("<div class='a'><p id='x'>1</p><p class='y z'>2</p></div><p>3</p>");
    assert_eq!(dom.select("p").count(), 3);
    assert_eq!(dom.select("div p").count(), 2);
    assert_eq!(dom.select("div > p").count(), 2);
    assert_eq!(dom.select("#x").count(), 1);
    assert_eq!(dom.select(".y").count(), 1);
    assert_eq!(dom.select("p.y.z").count(), 1);
    assert_eq!(dom.select("p.y.q").count(), 0);
    assert_eq!(dom.select("div.a > p#x").count(), 1);
    assert_eq!(dom.select("p, div").count(), 4);
    assert_eq!(dom.select("*").count(), 4);
    assert_eq!(dom.select("[id=x]").count(), 1);
    assert_eq!(dom.select("[id]").count(), 1);
    assert_eq!(dom.select("[class]").count(), 2);
}

#[test]
fn child_vs_descendant() {
    let dom = parse("<div><section><p>deep</p></section><p>shallow</p></div>");
    assert_eq!(dom.select("div p").count(), 2);
    assert_eq!(dom.select("div > p").count(), 1);
    assert_eq!(
        dom.select("div > p").next().unwrap().text_content(),
        "shallow"
    );
}

#[test]
fn attribute_quoted_value() {
    let dom = parse(r#"<a href="/x">1</a><a href="/y">2</a>"#);
    assert_eq!(dom.select(r#"a[href="/x"]"#).count(), 1);
    assert_eq!(dom.select("a[href='/y']").count(), 1);
}

#[test]
fn empty_or_garbage_selector_matches_nothing() {
    let dom = parse("<p>x</p>");
    assert_eq!(dom.select("").count(), 0);
    assert_eq!(dom.select("   ").count(), 0);
}

#[test]
fn selector_lists_and_whitespace() {
    let dom = parse("<div id=d><p id=p></p><span id=s></span></div>");
    assert_eq!(sel(&dom, "span, p"), ["p", "s"]); // document order
    assert_eq!(sel(&dom, "p,p , p"), ["p"]); // no duplicates
    assert_eq!(sel(&dom, "  div\n>\tp  "), ["p"]);
    assert_eq!(sel(&dom, "div>p,div>span"), ["p", "s"]);
    assert_eq!(sel(&dom, "*:not(div)"), ["p", "s"]);
}

// --- combinators ---------------------------------------------------------

#[test]
fn sibling_combinators_skip_text_and_comments() {
    let dom = parse(
        "<div><h1 id=h>t</h1>text<!-- c --><p id=a>1</p> <p id=b>2</p>\
         <span id=s></span><p id=c>3</p></div>",
    );
    assert_eq!(sel(&dom, "h1 + p"), ["a"]);
    assert_eq!(sel(&dom, "h1+p"), ["a"]);
    assert_eq!(sel(&dom, "h1 ~ p"), ["a", "b", "c"]);
    assert_eq!(sel(&dom, "h1~p"), ["a", "b", "c"]);
    assert_eq!(sel(&dom, "p + p"), ["b"]);
    assert_eq!(sel(&dom, "p ~ p"), ["b", "c"]);
    assert_eq!(sel(&dom, "span + p"), ["c"]);
    assert_eq!(sel(&dom, "p + span"), ["s"]);
    assert_eq!(sel(&dom, "h1 + p + p ~ p"), ["c"]);
    assert_eq!(sel(&dom, "div > h1 ~ span"), ["s"]);
    assert_eq!(sel(&dom, "p + h1"), NONE);
    assert_eq!(sel(&dom, "span ~ span"), NONE);
}

#[test]
fn matching_looks_past_the_nearest_partial_match() {
    // The nearest `.b` ancestor's parent is not `.a`, but a farther one's is.
    let dom = parse(
        "<div id=d1 class=a><div id=d2 class=b><div id=d3 class=b><p id=t></p></div></div></div>",
    );
    assert_eq!(sel(&dom, ".a > .b p"), ["t"]);
    assert_eq!(sel(&dom, ".a > .b > .b > p"), ["t"]);
    assert_eq!(sel(&dom, ".a .b .b p"), ["t"]);
    assert_eq!(sel(&dom, ".b > .a p"), NONE);
    assert_eq!(sel(&dom, ".a .b .b .b p"), NONE);

    // The nearest preceding `.b` isn't preceded by `.a`, but an earlier one is.
    let dom = parse(
        "<div><i id=i1 class=a></i><i id=i2 class=b></i><i id=i3 class=x></i>\
         <i id=i4 class=b></i><i id=t class=c></i></div>",
    );
    assert_eq!(sel(&dom, ".a + .b ~ .c"), ["t"]);
    assert_eq!(sel(&dom, ".x + .b + .c"), ["t"]);
    assert_eq!(sel(&dom, ".a ~ .x ~ .c"), ["t"]);
    assert_eq!(sel(&dom, ".a + .b + .c"), NONE);
    assert_eq!(sel(&dom, ".x ~ .a ~ .c"), NONE);

    // Chunks mixing every combinator.
    let dom = parse(
        "<ul id=u><li id=l1><a id=x1></a></li><li id=l2 class=on></li>\
         <li id=l3><a id=x3></a></li></ul>",
    );
    assert_eq!(sel(&dom, "ul > li.on ~ li > a"), ["x3"]);
    assert_eq!(sel(&dom, "ul li + li ~ li a"), ["x3"]);
    assert_eq!(sel(&dom, "ul > li + li.on + li a"), ["x3"]);
    assert_eq!(sel(&dom, "li ~ li.on a"), NONE);
}

// --- attributes ----------------------------------------------------------

#[test]
fn attribute_operators() {
    let dom = parse(
        r#"<div>
            <a id=a1 href="https://example.com/doc.pdf" class="btn primary" lang="en-US" data-x="Foo Bar"></a>
            <a id=a2 href="/local" lang="en" data-x=""></a>
            <a id=a3 href="mailto:x@y.z" lang="english" rel></a>
        </div>"#,
    );
    assert_eq!(sel(&dom, "[rel]"), ["a3"]);
    assert_eq!(sel(&dom, "a[data-x]"), ["a1", "a2"]);
    assert_eq!(sel(&dom, "[data-x='']"), ["a2"]);
    assert_eq!(sel(&dom, "[href=/local]"), ["a2"]); // lenient unquoted value
    assert_eq!(sel(&dom, "[class~=primary]"), ["a1"]);
    assert_eq!(sel(&dom, "[class~=prim]"), NONE);
    assert_eq!(sel(&dom, "[data-x~='Foo']"), ["a1"]);
    assert_eq!(sel(&dom, "[data-x~='Foo Bar']"), NONE); // never matches with whitespace
    assert_eq!(sel(&dom, "[lang|=en]"), ["a1", "a2"]);
    assert_eq!(sel(&dom, "[lang|=en-US]"), ["a1"]);
    assert_eq!(sel(&dom, "[href^='https:']"), ["a1"]);
    assert_eq!(sel(&dom, r#"[href$=".pdf"]"#), ["a1"]);
    assert_eq!(sel(&dom, "[href*=example]"), ["a1"]);
    assert_eq!(sel(&dom, "[href*='@']"), ["a3"]);
    assert_eq!(sel(&dom, "[ lang |= 'en' ]"), ["a1", "a2"]);
    assert_eq!(sel(&dom, "a:not([href^=http])"), ["a2", "a3"]);

    // As in CSS, an empty value never matches the substring operators.
    for op in ["^=", "$=", "*=", "~="] {
        assert_eq!(sel(&dom, &format!("[data-x{op}'']")), NONE, "{op}");
        assert_eq!(sel(&dom, &format!("[href{op}\"\"]")), NONE, "{op}");
    }
}

#[test]
fn case_sensitivity_rules() {
    let dom = parse(r#"<DIV ID=Main CLASS="Big"><P DATA-Kind="Note">x</P></DIV>"#);
    // Tag and attribute names: ASCII case-insensitive.
    assert_eq!(dom.select("div").count(), 1);
    assert_eq!(dom.select("DiV > P").count(), 1);
    assert_eq!(dom.select("[data-kind]").count(), 1);
    assert_eq!(dom.select("[DATA-KIND]").count(), 1);
    // Ids, classes and attribute values: case-sensitive...
    assert_eq!(dom.select("#Main").count(), 1);
    assert_eq!(dom.select("#main").count(), 0);
    assert_eq!(dom.select(".Big").count(), 1);
    assert_eq!(dom.select(".big").count(), 0);
    assert_eq!(dom.select("[data-kind=note]").count(), 0);
    // ...unless the `i` flag says otherwise (`s` keeps them sensitive).
    assert_eq!(dom.select("[data-kind=note i]").count(), 1);
    assert_eq!(dom.select("[data-kind='NOTE' I]").count(), 1);
    assert_eq!(dom.select(r#"[data-kind="note"i]"#).count(), 1);
    assert_eq!(dom.select("[data-kind^=no i]").count(), 1);
    assert_eq!(dom.select("[data-kind*=OT i]").count(), 1);
    assert_eq!(dom.select("[data-kind=note s]").count(), 0);
    assert_eq!(dom.select("[data-kind=Note s]").count(), 1);
    // Pseudo-class names: case-insensitive.
    assert_eq!(dom.select("P:FIRST-CHILD").count(), 1);
    assert_eq!(dom.select("div:Not(p)").count(), 1);
    assert_eq!(dom.select("p:NTH-child(ODD)").count(), 1);
}

#[test]
fn css_escapes_in_names_and_strings() {
    let dom = parse(
        r#"<div id="a.b" class="md:flex w-1/2 10 [&>p]:mt-2"><p id="x:y" data-q='say "hi"'>t</p></div>"#,
    );
    assert_eq!(dom.select(r"#a\.b").count(), 1);
    assert_eq!(dom.select(r".md\:flex").count(), 1);
    assert_eq!(dom.select(r".w-1\/2").count(), 1);
    assert_eq!(dom.select(r".\31 0").count(), 1); // hex escape + terminating space
    assert_eq!(dom.select(r".\000031 0").count(), 1);
    assert_eq!(dom.select(r".\[\&\>p\]\:mt-2").count(), 1);
    assert_eq!(dom.select(r"#x\3A y").count(), 1);
    assert_eq!(dom.select(r"\64 iv > p").count(), 1); // escaped type selector
    assert_eq!(dom.select(r#"[data-q="say \"hi\""]"#).count(), 1);
    assert_eq!(dom.select(r#"[data-q='say "hi"']"#).count(), 1);
    assert_eq!(dom.select(r"[id=x\:y]").count(), 1);
    assert_eq!(dom.select(".md:flex").count(), 0); // unescaped: an unknown pseudo-class
}

// --- pseudo-classes ------------------------------------------------------

#[test]
fn nth_child_formulas() {
    let dom = parse(
        "<ul> <li id=1></li> <!-- c --> <li id=2></li> text <li id=3></li>\
         <li id=4></li><li id=5></li><li id=6></li><li id=7></li></ul>",
    );
    let n = |s: &str| sel(&dom, s);
    assert_eq!(n("li:nth-child(odd)"), ["1", "3", "5", "7"]);
    assert_eq!(n("li:nth-child(ODD)"), ["1", "3", "5", "7"]);
    assert_eq!(n("li:nth-child(even)"), ["2", "4", "6"]);
    assert_eq!(n("li:nth-child(2n+1)"), ["1", "3", "5", "7"]);
    assert_eq!(n("li:nth-child( 2n + 1 )"), ["1", "3", "5", "7"]);
    assert_eq!(n("li:nth-child(2n-1)"), ["1", "3", "5", "7"]);
    assert_eq!(n("li:nth-child(+2n+2)"), ["2", "4", "6"]);
    assert_eq!(n("li:nth-child(3n)"), ["3", "6"]);
    assert_eq!(n("li:nth-child(n)").len(), 7);
    assert_eq!(n("li:nth-child(N+6)"), ["6", "7"]);
    assert_eq!(n("li:nth-child(n+5)"), ["5", "6", "7"]);
    assert_eq!(n("li:nth-child(-n+3)"), ["1", "2", "3"]);
    assert_eq!(n("li:nth-child(-2n+5)"), ["1", "3", "5"]);
    assert_eq!(n("li:nth-child(3)"), ["3"]);
    assert_eq!(n("li:nth-child(+3)"), ["3"]);
    assert_eq!(n("li:nth-child(0n+2)"), ["2"]);
    assert_eq!(n("li:nth-child(0)"), NONE);
    assert_eq!(n("li:nth-child(-n-1)"), NONE);
    assert_eq!(n("li:nth-child(-3)"), NONE);
    assert_eq!(n("li:nth-last-child(1)"), ["7"]);
    assert_eq!(n("li:nth-last-child(-n+2)"), ["6", "7"]);
    assert_eq!(n("li:nth-last-child(odd)"), ["1", "3", "5", "7"]);
    assert_eq!(n("li:nth-last-child(3n)"), ["2", "5"]);
    assert_eq!(n("li:first-child"), ["1"]);
    assert_eq!(n("li:last-child"), ["7"]);
    assert_eq!(n("li:only-child"), NONE);
}

#[test]
fn of_type_pseudo_classes() {
    let dom = parse(
        "<div id=d><h2 id=h1></h2><p id=p1></p><p id=p2></p><h2 id=h2></h2>\
         <p id=p3></p><span id=s></span></div>",
    );
    assert_eq!(sel(&dom, "p:first-of-type"), ["p1"]);
    assert_eq!(sel(&dom, "p:last-of-type"), ["p3"]);
    assert_eq!(sel(&dom, "div > :first-of-type"), ["h1", "p1", "s"]);
    assert_eq!(sel(&dom, "div > :last-of-type"), ["h2", "p3", "s"]);
    assert_eq!(sel(&dom, "div > :only-of-type"), ["s"]);
    assert_eq!(sel(&dom, "p:nth-of-type(2)"), ["p2"]);
    assert_eq!(sel(&dom, "p:nth-of-type(odd)"), ["p1", "p3"]);
    assert_eq!(sel(&dom, "p:nth-last-of-type(1)"), ["p3"]);
    assert_eq!(sel(&dom, "h2:nth-last-of-type(2)"), ["h1"]);
    assert_eq!(sel(&dom, "div > :nth-child(2)"), ["p1"]);
}

#[test]
fn nth_child_of_selector() {
    let dom = parse(
        "<ul><li id=a class=x></li><li id=b></li><li id=c class=x></li><li id=d class=x></li></ul>",
    );
    assert_eq!(sel(&dom, "li:nth-child(2 of .x)"), ["c"]);
    assert_eq!(sel(&dom, ":nth-child(odd of li.x)"), ["a", "d"]);
    assert_eq!(sel(&dom, "li:nth-last-child(1 of .x)"), ["d"]);
    assert_eq!(sel(&dom, "li:nth-child(n of .x)"), ["a", "c", "d"]);
    assert_eq!(sel(&dom, "li:nth-child(2n OF :not(.x), .x)"), ["b", "d"]);
    assert_eq!(sel(&dom, "li:nth-child(1 of .x):not(.x)"), NONE);
}

#[test]
fn only_child_empty_and_root() {
    let dom = parse(
        "<div id=r><p id=only>x</p></div><section id=r2><b id=b1></b> <b id=b2><!-- c --></b>\
         <b id=b3> </b><b id=b4><i id=i></i></b></section>",
    );
    assert_eq!(sel(&dom, "div :only-child"), ["only"]);
    assert_eq!(sel(&dom, "section :only-child"), ["i"]);
    assert_eq!(sel(&dom, "b:only-child"), NONE);
    // `:empty`: comments don't count, any text (even whitespace) does.
    assert_eq!(sel(&dom, "b:empty"), ["b1", "b2"]);
    assert_eq!(sel(&dom, ":empty"), ["b1", "b2", "i"]);
    // `:root`: every top-level element.
    assert_eq!(sel(&dom, ":root"), ["r", "r2"]);
    assert_eq!(sel(&dom, ":root > p"), ["only"]);

    let page = parse("<!doctype html><html id=h><body id=b><p id=p></p></body></html>");
    assert_eq!(sel(&page, ":root"), ["h"]);
    assert_eq!(sel(&page, ":root > body > p"), ["p"]);
}

#[test]
fn logical_pseudo_classes() {
    let dom = parse(
        "<div id=d><p id=p1 class=a></p><p id=p2 class=b></p><span id=s class=a></span></div>",
    );
    assert_eq!(sel(&dom, "p:not(.a)"), ["p2"]);
    assert_eq!(sel(&dom, "div > :not(p)"), ["s"]);
    assert_eq!(sel(&dom, "div > :not(.a, .b)"), NONE);
    assert_eq!(sel(&dom, ":is(p, span).a"), ["p1", "s"]);
    assert_eq!(sel(&dom, ":where(p, span).a"), ["p1", "s"]);
    assert_eq!(sel(&dom, ":is(div > p)"), ["p1", "p2"]);
    assert_eq!(sel(&dom, "p:not(div > .a)"), ["p2"]); // complex selectors inside
    assert_eq!(sel(&dom, ":not(:not(.b))"), ["p2"]);
    assert_eq!(sel(&dom, ":is(:is(:is(span)))"), ["s"]);
    assert_eq!(sel(&dom, ":is(.a, .b) + :is(.a, .b)"), ["p2", "s"]);
}

#[test]
fn has_relative_selectors() {
    let dom = parse(
        "<main id=m><div id=d1><img id=i1></div><div id=d2><p id=p><img id=i2></p></div>\
         <div id=d3></div><h2 id=h1></h2><p id=q1></p><h2 id=h2></h2><span id=sp></span>\
         <p id=q2></p></main>",
    );
    assert_eq!(sel(&dom, "div:has(img)"), ["d1", "d2"]);
    assert_eq!(sel(&dom, "div:has(> img)"), ["d1"]);
    assert_eq!(sel(&dom, "div:has(p img)"), ["d2"]);
    assert_eq!(sel(&dom, "div:has(div img)"), NONE); // relative to each div
    assert_eq!(sel(&dom, "main:has(div img)"), ["m"]);
    assert_eq!(sel(&dom, ":has(> #i2)"), ["p"]);
    assert_eq!(sel(&dom, "h2:has(+ p)"), ["h1"]);
    assert_eq!(sel(&dom, "h2:has(~ p)"), ["h1", "h2"]);
    assert_eq!(sel(&dom, "h2:has(+ span + p)"), ["h2"]);
    assert_eq!(sel(&dom, "div:has(+ div > p)"), ["d1"]);
    assert_eq!(sel(&dom, "div:has(~ h2 + p)"), ["d1", "d2", "d3"]);
    assert_eq!(sel(&dom, "div:has(span, > img)"), ["d1"]);
    assert_eq!(sel(&dom, "div:not(:has(*))"), ["d3"]);
    assert_eq!(sel(&dom, "img:has(*)"), NONE);
    assert_eq!(sel(&dom, ":has(> img) + div"), ["d2"]);
}

#[test]
fn contains_is_a_case_sensitive_text_search() {
    let dom = parse(
        "<ul id=u><li id=a>Hello <b>World</b></li><li id=b>hello</li><li id=c>Price: $5 (USD)</li></ul>",
    );
    assert_eq!(sel(&dom, "li:contains('Hello World')"), ["a"]); // spans child elements
    assert_eq!(sel(&dom, r#"li:contains("hello")"#), ["b"]);
    assert_eq!(sel(&dom, "li:contains(Hello)"), ["a"]); // quotes optional
    assert_eq!(sel(&dom, "li:contains( World )"), ["a"]);
    assert_eq!(sel(&dom, "li:contains('(USD)')"), ["c"]);
    assert_eq!(sel(&dom, "li:contains('')").len(), 3);
    assert_eq!(
        sel(&dom, "ul:contains(Price) > li:not(:contains(Price))"),
        ["a", "b"]
    );
}

#[test]
fn form_state_pseudo_classes() {
    let dom = parse(
        "<form><input id=i1 type=checkbox checked><input id=i2 disabled>\
         <select id=s><option id=o1 selected>a</option><option id=o2 disabled>b</option></select>\
         <button id=b>go</button><div id=d disabled></div></form>",
    );
    assert_eq!(sel(&dom, ":checked"), ["i1", "o1"]);
    assert_eq!(sel(&dom, ":disabled"), ["i2", "o2"]);
    assert_eq!(sel(&dom, ":enabled"), ["i1", "s", "o1", "b"]);
}

// --- errors --------------------------------------------------------------

#[test]
fn parse_errors_report_byte_positions() {
    let e = |pos: usize, msg: &str| (pos, msg.to_string());
    assert_eq!(err("div > p)"), e(7, "unexpected ')'"));
    assert_eq!(err(""), e(0, "expected a selector"));
    assert_eq!(err("   "), e(3, "expected a selector"));
    assert_eq!(err("a,"), e(2, "expected a selector"));
    assert_eq!(err("a, ,b"), e(3, "unexpected ','"));
    assert_eq!(err("ul >"), e(4, "expected a selector"));
    assert_eq!(err("a > > b"), e(4, "unexpected '>'"));
    assert_eq!(err("> a"), e(0, "unexpected '>'"));
    assert_eq!(err("a:hover"), e(1, "unsupported pseudo-class ':hover'"));
    assert_eq!(err("a:HOVER"), e(1, "unsupported pseudo-class ':hover'"));
    assert_eq!(err("é:hover"), e(2, "unsupported pseudo-class ':hover'"));
    assert_eq!(
        err("li:nth-col(1)"),
        e(2, "unsupported pseudo-class ':nth-col()'")
    );
    assert_eq!(err("p::before"), e(1, "pseudo-elements are not supported"));
    assert_eq!(err("."), e(1, "unexpected end of selector"));
    assert_eq!(err("#>"), e(1, "unexpected '>'"));
    assert_eq!(err("[a"), e(2, "unexpected end of selector"));
    assert_eq!(err("[a!=b]"), e(2, "unexpected '!'"));
    assert_eq!(err("[=b]"), e(1, "unexpected '='"));
    assert_eq!(err("[a=]"), e(3, "unexpected ']'"));
    assert_eq!(err("a[b='c]"), e(4, "unterminated string"));
    assert_eq!(err("[a=b x]"), e(5, "unexpected 'x'"));
    assert_eq!(err("li:nth-child(x)"), e(13, "invalid :nth-*() argument"));
    assert_eq!(err("li:nth-child(2n+)"), e(16, "unexpected ')'"));
    assert_eq!(err("li:nth-child(2n1)"), e(15, "unexpected '1'"));
    assert_eq!(err("li:nth-child(99999999999)"), e(13, "number too large"));
    assert_eq!(err("li:nth-child(1"), e(14, "unexpected end of selector"));
    assert_eq!(err("li:nth-of-type(1 of p)"), e(17, "unexpected 'o'"));
    assert_eq!(err(":not()"), e(5, "unexpected ')'"));
    assert_eq!(err(":not(a"), e(6, "unexpected end of selector"));
    assert_eq!(err(":contains()"), e(10, "unexpected ')'"));
    assert_eq!(err(":has(:has(a))"), e(5, ":has() cannot be nested"));
    assert_eq!(err(":has(:not(:has(a)))"), e(10, ":has() cannot be nested"));
    assert_eq!(err("svg|rect"), e(3, "unexpected '|'"));
    assert_eq!(err("a /* c */"), e(2, "unexpected '/'"));
    assert_eq!(err("a || b"), e(2, "unexpected '|'"));

    let error = Selector::parse("div > p)").unwrap_err();
    assert_eq!(error.to_string(), "unexpected ')' at byte 7");
    assert_eq!(error.clone(), error);
}

#[test]
fn invalid_selector_strings_match_nothing() {
    let dom = parse("<a id=x>x</a>");
    let a = dom.select_first("a").unwrap();
    for bad in [
        "a:hover",
        "a::before",
        "a)",
        "a[",
        ":has(:has(a))",
        "a,",
        "a:focus-visible",
    ] {
        assert_eq!(dom.select(bad).count(), 0, "{bad}");
        assert!(dom.select_first(bad).is_none(), "{bad}");
        assert!(!a.matches(bad), "{bad}");
        assert!(a.closest(bad).is_none(), "{bad}");
        assert!(bad.as_selector().is_none(), "{bad}");
    }
}

#[test]
fn nesting_depth_is_bounded() {
    let nest = |n: usize| format!("{}p{}", ":is(".repeat(n), ")".repeat(n));
    assert!(Selector::parse(&nest(32)).is_ok());
    let error = Selector::parse(&nest(33)).unwrap_err();
    assert_eq!(error.message(), "selectors nested too deeply");
    assert_eq!(error.position(), 33 * 4);
    let mixed = format!("{}p{}", ":not(:is(".repeat(16), "))".repeat(16));
    assert!(Selector::parse(&mixed).is_ok());

    // Absurd nesting is an error, never a stack overflow.
    assert!(Selector::parse(&nest(100_000)).is_err());
    assert!(Selector::parse(&":not(".repeat(100_000)).is_err());

    let dom = parse("<p>x</p>");
    assert_eq!(dom.select(nest(32).as_str()).count(), 1);
    assert_eq!(dom.select(nest(33).as_str()).count(), 0);
}

// --- scoped API ----------------------------------------------------------

#[test]
fn node_select_has_query_selector_all_semantics() {
    let dom = parse(
        "<div id=outer><section id=sec><p id=p1>a</p><div id=inner><p id=p2>b</p></div>\
         </section><p id=p3>c</p></div>",
    );
    let sec = dom.select_first("#sec").unwrap();
    assert_eq!(ids(sec.select("p")), ["p1", "p2"]); // descendants only
    assert_eq!(ids(sec.select("div p")), ["p1", "p2"]); // #outer is above the scope
    assert_eq!(ids(sec.select(":scope > p")), ["p1"]);
    assert_eq!(ids(sec.select(":scope div p")), ["p2"]);
    assert_eq!(ids(sec.select("section p")), ["p1", "p2"]);
    assert!(sec.select("section").next().is_none()); // never the node itself
    assert!(sec.select(":scope").next().is_none());
    assert_eq!(sec.select_first("p").unwrap().attr("id"), Some("p1"));
    assert!(sec.select_first("table").is_none());

    // At the document level `:scope` is `:root`.
    assert_eq!(sel(&dom, ":scope > section"), ["sec"]);
    assert_eq!(sel(&dom, ":scope"), ["outer"]);

    // Text nodes have no descendants.
    let text = dom.select_first("#p1").unwrap().first_child().unwrap();
    assert_eq!(text.select("*").count(), 0);
}

#[test]
fn matches_and_closest() {
    let dom =
        parse("<div id=a class=card><ul id=u><li id=l class=item><b id=b>x</b></li></ul></div>");
    let li = dom.select_first("li").unwrap();
    let b = dom.select_first("#b").unwrap();
    let text = b.first_child().unwrap();

    assert!(li.matches("li"));
    assert!(li.matches(".card li.item"));
    assert!(li.matches("ul > li:first-child:last-child"));
    assert!(li.matches(":scope")); // the node itself
    assert!(!li.matches("ol li"));
    assert!(!text.matches("*")); // only elements match

    assert_eq!(b.closest("b").unwrap().id(), b.id()); // self-inclusive
    assert_eq!(b.closest("li").unwrap().id(), li.id());
    assert_eq!(b.closest("ul > li").unwrap().id(), li.id());
    assert_eq!(b.closest(".card").unwrap().attr("id"), Some("a"));
    assert_eq!(b.closest("div:has(> ul)").unwrap().attr("id"), Some("a"));
    assert_eq!(b.closest(":scope").unwrap().id(), b.id());
    assert_eq!(text.closest("li").unwrap().id(), li.id());
    assert!(b.closest("table").is_none());

    // `Selector::matches` has no scope element: `:scope` is `:root` there.
    let scope = Selector::parse(":scope").unwrap();
    assert!(!scope.matches(li));
    assert!(scope.matches(dom.root().unwrap()));
}

#[test]
fn accepts_str_string_and_selector() {
    let dom = parse("<ul><li class=a>1</li><li>2</li></ul><ul><li class=a>3</li></ul>");
    let owned = String::from("li.a");
    let parsed = Selector::parse("li.a").unwrap();
    let from_str: Selector = "li.a".parse().unwrap();

    assert_eq!(dom.select("li.a").count(), 2);
    assert_eq!(dom.select(&owned).count(), 2);
    assert_eq!(dom.select(&parsed).count(), 2);
    assert_eq!(dom.select(&from_str).count(), 2);
    assert_eq!(dom.select(owned.as_str()).count(), 2);
    assert!(dom.select_first(&owned).is_some());
    assert!(dom.select_first(&parsed).is_some());

    let ul = dom.select_first("ul").unwrap();
    assert_eq!(ul.select(&parsed).count(), 1);
    assert_eq!(ul.select(&owned).count(), 1);
    let li = ul.select_first(&parsed).unwrap();
    assert!(li.matches(&parsed) && li.matches(&owned));
    assert!(parsed.matches(li));
    assert_eq!(
        li.closest(&Selector::parse("ul").unwrap()).unwrap().id(),
        ul.id()
    );

    // One parsed selector reused across many queries.
    let total: usize = dom.select("ul").map(|u| u.select(&parsed).count()).sum();
    assert_eq!(total, 2);

    // Clone / Debug.
    assert_eq!(format!("{:?}", parsed.clone()), r#"Selector("li.a")"#);

    // The iterator doesn't borrow the (temporary) node handle it came from.
    let items = dom.select_first("ul").unwrap().select("li");
    assert_eq!(items.count(), 2);
}

// --- robustness & performance ---------------------------------------------

fn nested_divs(depth: usize) -> String {
    let mut html = "<div>".repeat(depth);
    html.push_str("<span id=deep>x</span>");
    html.push_str(&"</div>".repeat(depth));
    html
}

#[test]
fn deep_descendant_chains_do_not_backtrack_exponentially() {
    let dom = parse(&nested_divs(5_000));
    let start = Instant::now();
    assert_eq!(dom.select("div div div div div span").count(), 1);
    assert_eq!(dom.select("section div div div div span").count(), 0);
    assert_eq!(dom.select("div div div div div p").count(), 0);
    assert_eq!(dom.select("div > div div > div div span").count(), 1);
    assert_eq!(dom.select("div div div div > p > div span").count(), 0);
    assert_eq!(
        dom.select("div:first-child div:only-child > span:last-child")
            .count(),
        1
    );
    assert_eq!(dom.select(":is(div div div) span").count(), 1);
    assert_eq!(dom.select(":is(div p div) span").count(), 0);
    let span = dom.select_first("span").unwrap();
    assert!(span.matches(":not(section div div div span)"));
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_secs(2), "took {elapsed:?}");
}

#[test]
fn very_deep_documents_do_not_overflow_the_stack() {
    let dom = parse(&nested_divs(100_000));
    assert_eq!(dom.select("div div span").count(), 1);
    assert_eq!(dom.select("p div span").count(), 0);
    let span = dom.select_first("#deep").unwrap();
    assert!(span.closest("p").is_none());
    assert!(span.closest("div:root").is_some());
    assert!(span.matches("div > span:only-child"));
    assert!(span.matches(":root span"));
    let root = dom.root().unwrap();
    assert!(root.matches(":has(> div)"));
    assert!(root.matches(":has(span#deep)"));
    assert!(!root.matches(":has(p)"));
    assert_eq!(root.select(":scope span").count(), 1);
}

/// Time `f` (excluding document parsing). Each block takes ~0.1–0.3 s in a
/// debug build; the quadratic matching it guards against takes 30 s to
/// minutes. The bound leaves ~10x headroom so a busy CI machine can't trip it.
fn fast<T>(what: &str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = f();
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_secs(3), "{what} took {elapsed:?}");
    out
}

#[test]
fn wide_sibling_lists_stay_linear() {
    let n = 50_000;
    let list = parse(&format!("<ul>{}</ul>", "<li>x</li> ".repeat(n)));
    fast("nth-* over 50k siblings", || {
        assert_eq!(list.select("li:nth-child(odd)").count(), n / 2);
        assert_eq!(list.select("li:nth-child(2n+1)").count(), n / 2);
        assert_eq!(list.select("li:nth-child(n+3)").count(), n - 2);
        assert_eq!(list.select("li:nth-child(-n+3)").count(), 3);
        assert_eq!(list.select("li:nth-last-child(odd)").count(), n / 2);
        assert_eq!(list.select("li:nth-of-type(3n)").count(), n / 3);
        assert_eq!(list.select("li:nth-last-of-type(2n)").count(), n / 2);
        // Items 2..=n count, so item k is at position k - 1.
        let of_s = "li:nth-child(even of :not(:first-child))";
        assert_eq!(list.select(of_s).count(), n / 2 - 1);
        assert_eq!(list.select("li:last-child").count(), 1);
    });
    let flat = parse(&format!("<div><h2>t</h2>{}</div>", "<p>x</p>".repeat(n)));
    fast("~ over 50k siblings", || {
        assert_eq!(flat.select("h2 ~ p").count(), n);
        assert_eq!(flat.select("h2 + p ~ p").count(), n - 1);
        assert_eq!(flat.select("h3 ~ p").count(), 0);
    });
}

#[test]
fn nested_nth_of_selectors_are_memoized() {
    let dom = parse(&format!("<ul>{}</ul>", "<li>x</li>".repeat(1_000)));
    let nested = "li:nth-child(n of :nth-child(n of :nth-child(n of :nth-child(n of li))))";
    fast("4 nested `of` over 1k siblings", || {
        assert_eq!(dom.select(nested).count(), 1_000);
        let odd = "li:nth-child(odd of :nth-child(odd of :nth-child(odd of li)))";
        assert_eq!(dom.select(odd).count(), 125);
    });
}

#[test]
fn deep_descendant_searches_are_memoized() {
    let n = 100_000;
    let spans = |open: &str, close: &str| {
        parse(&format!(
            "{open}{}{}{close}",
            "<span>".repeat(n),
            "</span>".repeat(n)
        ))
    };
    let bare = spans("", "");
    let wrapped = spans("<div>", "</div>");
    fast("descendant searches over 100k nested spans", || {
        assert_eq!(bare.select("div span").count(), 0); // no ancestor ever matches
        assert_eq!(wrapped.select("div span").count(), n); // the same one always does
        assert_eq!(wrapped.select("div > span span").count(), n - 1);
        assert_eq!(wrapped.select(":root span").count(), n);
        assert_eq!(bare.select("span:not(:root) span").count(), n - 2);
    });
}

#[test]
fn per_row_queries_cost_what_they_walk() {
    // A scoped or single-node query memoizes only what it walks, so a loop
    // over every row of a big table stays linear. (Each call used to clear a
    // table the size of the document: 5-50 s here in a debug build.)
    let n = 20_000;
    let mut html = String::from("<table>");
    for i in 0..n {
        html.push_str(&format!("<tr><td>{i}</td><td><a href=/{i}>x</a></td></tr>"));
    }
    html.push_str("</table>");
    let dom = parse(&html);
    let rows: Vec<_> = dom.find_by_tag("tr").collect();
    let cells: Vec<_> = dom.find_by_tag("td").collect();
    fast("tr.select(..) per row", || {
        let hits: usize = rows
            .iter()
            .map(|tr| tr.select("td:nth-child(2)").count())
            .sum();
        assert_eq!(hits, n);
        let links = rows.iter().filter(|tr| tr.select_first("td a").is_some());
        assert_eq!(links.count(), n);
    });
    fast("td.matches(..) / td.closest(..) per cell", || {
        assert_eq!(cells.iter().filter(|td| td.matches("tr td")).count(), 2 * n);
        let last = cells
            .iter()
            .filter_map(|td| td.closest("tr > td:last-child"));
        assert_eq!(last.count(), n);
    });
    let sel = Selector::parse("table tr > td:nth-child(2):has(a)").unwrap();
    fast("Selector::matches per cell", || {
        assert_eq!(cells.iter().filter(|td| sel.matches(**td)).count(), n);
    });
}

#[test]
fn long_selector_lists_stay_linear() {
    // Every `:nth-child()` shares one table of positions, whatever its An+B.
    let list = parse(&format!("<ul>{}</ul>", "<li>x</li>".repeat(4_000)));
    let nth: Vec<String> = (1..=200)
        .map(|k| format!("li:nth-child(200n+{k})"))
        .collect();
    let nth = Selector::parse(&nth.join(", ")).unwrap();
    fast("200 :nth-child()s over 4k siblings", || {
        assert_eq!(list.select(&nth).count(), 4_000);
        assert_eq!(list.root().unwrap().select(&nth).count(), 4_000);
    });
    // A hundred combinators over deep nesting: the memo keeps what the query
    // is about to reuse even when it can't keep everything it walked.
    let deep = parse(&format!(
        "{}{}",
        "<div>".repeat(2_000),
        "</div>".repeat(2_000)
    ));
    let comb: Vec<String> = (1..=100).map(|k| format!(".a{k} div")).collect();
    let comb = Selector::parse(&comb.join(", ")).unwrap();
    fast("100 descendant combinators over 2k-deep nesting", || {
        assert_eq!(deep.select(&comb).count(), 0);
        assert_eq!(deep.root().unwrap().select(&comb).count(), 0);
    });
}

#[test]
fn has_arguments_depend_on_their_subject() {
    // Each `:has()` subject is matched on its own, even though both subjects
    // share siblings (or ancestors) with each other.
    let dom = parse("<div><h2 id=h1></h2><i class=x></i><h2 id=h2></h2><q></q><p></p></div>");
    assert_eq!(sel(&dom, "h2:has(+ .x ~ p)"), ["h1"]);
    let dom = parse("<div id=d1><div class=x><div id=d2><p></p></div></div></div>");
    assert_eq!(sel(&dom, "div:has(.x p)"), ["d1"]);
}

#[test]
fn enumerated_attribute_values_ignore_case() {
    let dom = parse(
        r#"<form id=f method=POST><input id=t TYPE=Text><a id=a rel="NoFollow" hreflang=EN-gb data-k=Foo></a></form>"#,
    );
    // HTML's enumerated attributes compare ASCII case-insensitively...
    assert_eq!(sel(&dom, "[type=text]"), ["t"]);
    assert_eq!(sel(&dom, "form[method=post]"), ["f"]);
    assert_eq!(sel(&dom, "[rel~=nofollow]"), ["a"]);
    assert_eq!(sel(&dom, "[hreflang|=en]"), ["a"]);
    // ...unless `s` says otherwise; other attributes stay case-sensitive.
    assert_eq!(sel(&dom, "[type=text s]"), NONE);
    assert_eq!(sel(&dom, "[type=Text s]"), ["t"]);
    assert_eq!(sel(&dom, "[data-k=foo]"), NONE);
    assert_eq!(sel(&dom, "[data-k=foo i]"), ["a"]);
}

#[test]
fn class_lists_split_on_ascii_whitespace_only() {
    let dom = parse("<p id=nbsp class=\"a\u{a0}b\"></p><p id=ws class=\"c\td\ne\u{c}f\rg\"></p>");
    assert_eq!(sel(&dom, ".a"), NONE); // U+00A0 is part of the class name
    assert_eq!(sel(&dom, ".a\\a0 b"), ["nbsp"]);
    assert_eq!(sel(&dom, ".c.d.e.f.g"), ["ws"]);
    assert_eq!(sel(&dom, "[class~=f]"), ["ws"]);
}
