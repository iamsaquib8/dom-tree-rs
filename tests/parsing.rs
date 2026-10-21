//! Behavioural tests for the parser and tree.

use domtree::{NodeKind, Token, parse, tokenize};

#[test]
fn basic_nesting() {
    let dom = parse("<a><b>hi</b></a>");
    let a = dom.root().unwrap();
    assert_eq!(a.tag_name(), Some("a"));
    let b = a.first_child().unwrap();
    assert_eq!(b.tag_name(), Some("b"));
    assert_eq!(b.text_content(), "hi");
    assert_eq!(b.parent().unwrap().id(), a.id());
}

#[test]
fn void_elements_have_no_children() {
    let dom = parse("<div><br><img src=x>text</div>");
    let div = dom.root().unwrap();
    let kids: Vec<_> = div.children().collect();
    assert_eq!(kids.len(), 3);
    assert_eq!(kids[0].tag_name(), Some("br"));
    assert_eq!(kids[1].tag_name(), Some("img"));
    assert_eq!(kids[1].attr("src"), Some("x"));
    assert_eq!(kids[2].text(), Some("text"));
    assert_eq!(div.child_elements().count(), 2);
}

#[test]
fn self_closing_tag() {
    let dom = parse("<div><span/>x</div>");
    let div = dom.root().unwrap();
    let kids: Vec<_> = div.children().collect();
    assert_eq!(kids[0].tag_name(), Some("span"));
    assert_eq!(kids[0].children().count(), 0);
    assert_eq!(kids[1].text(), Some("x"));
}

#[test]
fn attribute_variants() {
    let dom = parse(r#"<input type="text" disabled value=hello data-x='y'>"#);
    let input = dom.root().unwrap();
    assert_eq!(input.attr("type"), Some("text")); // double-quoted
    assert_eq!(input.attr("disabled"), Some("")); // boolean
    assert_eq!(input.attr("value"), Some("hello")); // unquoted
    assert_eq!(input.attr("data-x"), Some("y")); // single-quoted
    assert_eq!(input.attributes().count(), 4);
}

#[test]
fn attribute_names_lowercased() {
    let dom = parse(r#"<DIV CLASS="X" Id="y">"#);
    let div = dom.root().unwrap();
    assert_eq!(div.tag_name(), Some("div"));
    assert_eq!(div.attr("class"), Some("X")); // name lowered, value preserved
    assert_eq!(div.attr("id"), Some("y"));
}

#[test]
fn comments_doctype_cdata() {
    let dom = parse("<!doctype html><!-- hi --><p>x</p>");
    let roots: Vec<_> = dom.roots().collect();
    assert_eq!(roots.len(), 3);
    assert!(matches!(roots[0].kind(), NodeKind::Doctype(_)));
    assert_eq!(roots[1].comment(), Some(" hi "));
    assert_eq!(roots[2].tag_name(), Some("p"));
}

#[test]
fn script_content_is_raw() {
    let dom = parse("<script>if (a < b && c > d) { x('<p>'); }</script>");
    let s = dom.root().unwrap();
    assert_eq!(s.tag_name(), Some("script"));
    assert_eq!(s.children().count(), 1);
    assert_eq!(s.text_content(), "if (a < b && c > d) { x('<p>'); }");
}

#[test]
fn entities_decoded_in_text_but_not_in_script() {
    let dom = parse("<p>Tom &amp; Jerry &lt;3 &#169; &#x1F600;</p>");
    assert_eq!(dom.root().unwrap().text_content(), "Tom & Jerry <3 © 😀");

    let dom = parse("<style>a::before{content:'&amp;'}</style>");
    assert_eq!(
        dom.root().unwrap().text_content(),
        "a::before{content:'&amp;'}"
    );
}

#[test]
fn implicit_close_p_and_li() {
    let dom = parse("<ul><li>a<li>b</ul><p>x<p>y");
    assert_eq!(dom.find_by_tag("li").count(), 2);
    assert_eq!(dom.find_by_tag("p").count(), 2);
    let ul = dom.find_by_tag("ul").next().unwrap();
    assert_eq!(ul.child_elements().count(), 2);
}

#[test]
fn mismatched_and_stray_end_tags_recover() {
    let dom = parse("<b><i>x</b>y</i>");
    assert!(!dom.errors().is_empty());
    let i = dom.find_by_tag("i").next().unwrap();
    assert_eq!(i.parent().unwrap().tag_name(), Some("b"));
    assert_eq!(i.text_content(), "x");
}

#[test]
fn custom_and_namespaced_tags() {
    let dom = parse("<my-component data-id='1'><svg:rect/></my-component>");
    let c = dom.root().unwrap();
    assert_eq!(c.tag_name(), Some("my-component"));
    assert_eq!(c.attr("data-id"), Some("1"));
    assert_eq!(c.first_child().unwrap().tag_name(), Some("svg:rect"));
}

#[test]
fn find_helpers() {
    let dom = parse("<div id='main'><span class='a b'>1</span><span class='a'>2</span></div>");
    assert_eq!(dom.find_by_id("main").unwrap().tag_name(), Some("div"));
    assert_eq!(dom.find_by_class("a").count(), 2);
    assert_eq!(dom.find_by_class("b").count(), 1);
    assert_eq!(dom.find_by_tag("span").count(), 2);
}

#[test]
fn ancestors_and_siblings() {
    let dom = parse("<a><b><c>x</c></b></a>");
    let c = dom.find_by_tag("c").next().unwrap();
    let names: Vec<_> = c.ancestors().filter_map(|n| n.tag_name()).collect();
    assert_eq!(names, vec!["b", "a"]);

    let dom = parse("<ul><li>1</li><li>2</li><li>3</li></ul>");
    let first = dom.find_by_tag("li").next().unwrap();
    assert_eq!(first.next_sibling().unwrap().text_content(), "2");
    assert!(first.prev_sibling().is_none());
}

#[test]
fn element_siblings_skip_other_nodes() {
    let dom = parse("<ul>\n  <li>1</li>\n  <!-- c -->\n  <li>2</li>text<li>3</li>\n</ul>");
    let items: Vec<_> = dom.find_by_tag("li").collect();
    // Plain siblings include the whitespace text...
    assert!(items[0].next_sibling().unwrap().is_text());
    // ...element siblings skip text and comments.
    assert_eq!(items[0].next_element_sibling().unwrap().id(), items[1].id());
    assert_eq!(items[1].next_element_sibling().unwrap().id(), items[2].id());
    assert!(items[2].next_element_sibling().is_none());
    assert_eq!(items[2].prev_element_sibling().unwrap().id(), items[1].id());
    assert_eq!(items[1].prev_element_sibling().unwrap().id(), items[0].id());
    assert!(items[0].prev_element_sibling().is_none());
    // Text nodes have element siblings too.
    let text = items[1].next_sibling().unwrap();
    assert_eq!(text.next_element_sibling().unwrap().id(), items[2].id());
}

#[test]
fn tokenizer_stream() {
    let toks: Vec<_> = tokenize("<a href='x'>hi</a>").collect();
    assert_eq!(toks.len(), 3);
    assert!(matches!(toks[0], Token::StartTag { name: "a", .. }));
    assert!(matches!(toks[1], Token::Text("hi")));
    assert!(matches!(toks[2], Token::EndTag { name: "a" }));
}

#[test]
fn tokenizer_gives_each_start_tag_its_own_attributes() {
    // The tokenizer reuses one attribute buffer internally; the public
    // iterator must still hand every tag exactly its own attributes.
    use domtree::AttrValue;
    let attrs: Vec<_> = tokenize("<a x=1 Y='2'><b><!-- c --><i z>t</i><img src=\"s\" alt>")
        .filter_map(|t| match t {
            Token::StartTag { name, attrs, .. } => Some((name, attrs)),
            _ => None,
        })
        .collect();
    assert_eq!(
        attrs,
        [
            (
                "a",
                vec![
                    ("x", AttrValue::Unquoted("1")),
                    ("Y", AttrValue::Quoted("2"))
                ]
            ),
            ("b", vec![]),
            ("i", vec![("z", AttrValue::Empty)]),
            (
                "img",
                vec![("src", AttrValue::Quoted("s")), ("alt", AttrValue::Empty)]
            ),
        ]
    );
}

#[test]
fn never_panics_on_garbage() {
    let inputs = [
        "",
        "<",
        "<<>>",
        "<a",
        "<!--",
        "</>",
        "<a b=>",
        "<a b='",
        "<![CDATA[",
        "&",
        "&#;",
        "&#xZZ;",
        "<a/></a>",
        "<\u{0}>",
        "🦀<b>",
        "<a =",
        "<!",
        "<?php ?>",
        "</",
        "<div><div><div>",
        "<p</p>",
        "=>'\"<>",
    ];
    for i in inputs {
        let _ = parse(i);
        let _ = parse(i).to_json();
    }
}

#[test]
fn deeply_nested_does_not_overflow() {
    // 50k nested <div>: builder and serializer must stay iterative.
    let depth = 50_000;
    let mut html = String::with_capacity(depth * 5);
    for _ in 0..depth {
        html.push_str("<div>");
    }
    let dom = parse(&html);
    assert_eq!(dom.find_by_tag("div").count(), depth);
    // to_json is iterative, so this must not blow the stack either.
    assert!(dom.to_json().len() > depth);
}

#[test]
fn stray_end_tags_on_a_deep_stack_are_linear() {
    // Each `</x>` has no open match. Scanning the whole open-element stack for
    // every one of them used to make this quadratic (~400 s at n = 100k, ~35 s
    // at this n); with the search bounded, it is a few hundredths of a second.
    let n = 30_000;
    let mut html = "<div>".repeat(n);
    html.push_str(&"</x>".repeat(n));
    let start = std::time::Instant::now();
    let dom = parse(&html);
    assert!(
        start.elapsed().as_millis() < 1000,
        "took {:?}",
        start.elapsed()
    );
    assert_eq!(dom.find_by_tag("div").count(), n);

    // Many distinct stray names, then the real closes.
    let mut html = "<span>".repeat(n);
    for i in 0..n {
        html.push_str(&format!("</q{}>", i % 997));
    }
    html.push_str(&"</span>".repeat(n));
    html.push_str("<p>after</p>");
    let start = std::time::Instant::now();
    let dom = parse(&html);
    assert!(
        start.elapsed().as_millis() < 1000,
        "took {:?}",
        start.elapsed()
    );
    // Every span closed properly, so the trailing <p> is top-level.
    assert_eq!(dom.roots().count(), 2);
}

/// `n` distinct tag names whose FNV-1a hashes agree in their low 20 bits, so
/// they all land in one slot of a hash table of up to 2^20 slots indexed by
/// them. Found by meeting in the middle: FNV-1a's step `h = (h ^ b) * P` is
/// invertible (P is odd), and the low bits only depend on the low bits.
fn fnv_colliding_names(n: usize) -> Vec<String> {
    const P: u32 = 0x0100_0193;
    const MASK: u32 = (1 << 20) - 1;
    const TARGET: u32 = 0xb_eef5;
    let alnum = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut triples = Vec::new();
    for &a in alnum {
        for &b in alnum {
            for &c in alnum {
                triples.push([a, b, c]);
            }
        }
    }
    let step = |h: u32, b: &u8| (h ^ u32::from(*b)).wrapping_mul(P);
    // P's inverse modulo 2^32, by Newton's iteration.
    let p_inv = (0..5).fold(P, |x, _| {
        x.wrapping_mul(2u32.wrapping_sub(P.wrapping_mul(x)))
    });
    // The state each suffix takes to TARGET (modulo 2^20).
    let mut suffix_from = vec![usize::MAX; 1 << 20];
    for (i, s) in triples.iter().enumerate() {
        let h = s
            .iter()
            .rev()
            .fold(TARGET, |h, &b| h.wrapping_mul(p_inv) ^ u32::from(b));
        suffix_from[(h & MASK) as usize] = i;
    }
    let mut names = Vec::with_capacity(n);
    for k in 0.. {
        let prefix = format!("x{k}q");
        let h0 = prefix.bytes().fold(0x811c_9dc5, |h, b| step(h, &b));
        for middle in &triples {
            let h = middle.iter().fold(h0, step) & MASK;
            if let Some(suffix) = triples.get(suffix_from[h as usize]) {
                let mut name = prefix.clone();
                name.extend(middle.iter().chain(suffix).map(|&b| char::from(b)));
                names.push(name);
                if names.len() == n {
                    return names;
                }
            }
        }
    }
    names
}

#[test]
fn colliding_names_on_a_deep_stack_are_linear() {
    // Names that collide in a power-of-two hash table indexed by FNV-1a: open
    // and close each, then end it again (stray), on a stack deeper than the
    // end-tag search reaches. The parser used to keep per-name counts on deep
    // stacks in such a table, which this made quadratic (40,000 names: 6 s);
    // it keeps no per-name state now, so names cannot matter.
    let n = 20_000;
    let names = fnv_colliding_names(n);
    let fnv = |s: &str| {
        s.bytes().fold(0x811c_9dc5u32, |h, b| {
            (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
        })
    };
    assert!(
        names
            .iter()
            .all(|s| fnv(s) & 0xf_ffff == fnv(&names[0]) & 0xf_ffff)
    );
    assert_eq!(
        names.iter().collect::<std::collections::HashSet<_>>().len(),
        n
    );

    let mut html = "<div>".repeat(300);
    for name in &names {
        html.push_str(&format!("<{name}></{name}></{name}>"));
    }
    for (label, events) in [("parse", false), ("events", true)] {
        let start = std::time::Instant::now();
        let count = if events {
            domtree::events(&html).count()
        } else {
            parse(&html).len()
        };
        assert!(
            start.elapsed().as_millis() < 1000,
            "{label} took {:?}",
            start.elapsed()
        );
        assert!(count >= 300 + n);
    }
    let dom = parse(&html);
    let innermost = dom.find_by_tag("div").last().unwrap();
    assert_eq!(innermost.children().count(), n); // every name closed
    let stray = dom
        .errors()
        .iter()
        .filter(|e| e.kind == domtree::ErrorKind::StrayEndTag)
        .count();
    assert_eq!(stray, n); // every second end tag
}

/// The tag names of `el` and its ancestors, innermost first.
fn path(el: domtree::NodeRef<'_>) -> Vec<String> {
    std::iter::once(el)
        .chain(el.ancestors())
        .filter_map(|n| n.tag_name().map(str::to_owned))
        .collect()
}

#[test]
fn end_tags_reach_the_256_innermost_and_8_outermost_open_elements() {
    use domtree::ErrorKind::{MismatchedEndTag, StrayEndTag};
    // `<main>` sits under 8 wrappers (out of the outer reach) with `k` open
    // `<b>`s above it.
    let doc = |k: usize| {
        format!(
            "{}<main>{}</main><p>after</p>",
            "<div>".repeat(8),
            "<b>".repeat(k)
        )
    };

    // 255 above it: `<main>` is the 256th from the top and in reach, so
    // `</main>` closes it (and the `<b>`s, mis-nested) and `<p>` follows it.
    let dom = parse(&doc(255));
    assert!(kinds(&doc(255)).contains(&MismatchedEndTag));
    assert!(!kinds(&doc(255)).contains(&StrayEndTag));
    let p = dom.find_by_tag("p").next().unwrap();
    assert_eq!(
        path(p),
        ["p", "div", "div", "div", "div", "div", "div", "div", "div"]
    );
    let main = dom.find_by_tag("main").next().unwrap();
    assert_eq!(
        main.source_range().unwrap().end,
        doc(255).find("<p>").unwrap()
    );

    // 256 above it: out of reach. `</main>` is stray, everything stays open,
    // and `<p>` lands in the innermost `<b>`.
    let dom = parse(&doc(256));
    let stray: Vec<_> = dom
        .errors()
        .iter()
        .filter(|e| e.kind == StrayEndTag)
        .map(|e| e.position)
        .collect();
    assert_eq!(stray, [doc(256).find("</main>").unwrap()]);
    assert!(!kinds(&doc(256)).contains(&MismatchedEndTag));
    let p = dom.find_by_tag("p").next().unwrap();
    assert_eq!(p.parent().unwrap().tag_name(), Some("b"));
    assert_eq!(path(p).len(), 1 + 256 + 1 + 8);

    // The 8 outermost are always in reach: `</body>` and `</html>` close
    // everything above them however deep the stack.
    let html = format!(
        "<html>{}<body>{}x</body></html><p>after</p>",
        "<div>".repeat(6),
        "<b>".repeat(10_000)
    );
    let dom = parse(&html);
    assert!(!kinds(&html).contains(&StrayEndTag));
    assert_eq!(dom.roots().count(), 2);
    let body = dom.find_by_tag("body").next().unwrap();
    assert_eq!(
        body.source_range().unwrap().end,
        html.find("</html>").unwrap()
    );
    // ...but only those: one level further down is out of reach.
    let html = format!(
        "<html>{}<body>{}x</body>",
        "<div>".repeat(7),
        "<b>".repeat(300)
    );
    assert_eq!(
        parse(&html)
            .errors()
            .iter()
            .filter(|e| e.kind == StrayEndTag)
            .map(|e| e.position)
            .collect::<Vec<_>>(),
        [html.find("</body>").unwrap()]
    );
}

// --- implicit closes (WHATWG optional tags) ------------------------------

/// Re-serialized structure: shows where elements were closed.
fn structure(html: &str) -> String {
    parse(html).to_html()
}

#[test]
fn p_is_closed_by_every_block_start_tag() {
    for tag in [
        "address",
        "article",
        "aside",
        "blockquote",
        "center",
        "details",
        "dialog",
        "dir",
        "div",
        "dl",
        "fieldset",
        "figcaption",
        "figure",
        "footer",
        "form",
        "h1",
        "h6",
        "header",
        "hgroup",
        "listing",
        "main",
        "menu",
        "nav",
        "ol",
        "p",
        "plaintext",
        "pre",
        "search",
        "section",
        "summary",
        "table",
        "ul",
        "xmp",
    ] {
        assert_eq!(
            structure(&format!("<p>a<{tag}>b</{tag}>")),
            format!("<p>a</p><{tag}>b</{tag}>"),
            "<{tag}> should close <p>"
        );
    }
    assert_eq!(structure("<p>a<hr>b"), "<p>a</p><hr>b");
    // Inline content stays inside.
    for tag in ["span", "a", "b", "img", "button", "label", "q"] {
        assert!(structure(&format!("<p>a<{tag}>")).starts_with(&format!("<p>a<{tag}")));
    }
}

#[test]
fn list_items_and_definitions_close_an_open_p() {
    assert_eq!(
        structure("<ul><li><p>one<li><p>two</ul>"),
        "<ul><li><p>one</p></li><li><p>two</p></li></ul>"
    );
    assert_eq!(
        structure("<dl><dt><p>term<dd><p>def<dt>t2</dl>"),
        "<dl><dt><p>term</p></dt><dd><p>def</p></dd><dt>t2</dt></dl>"
    );
}

#[test]
fn headings_close_an_open_heading() {
    assert_eq!(
        structure("<h1>Title<h2>Sub<h3>Section"),
        "<h1>Title</h1><h2>Sub</h2><h3>Section</h3>"
    );
    // Only when the heading is the current node, as in browsers.
    assert_eq!(
        structure("<h1><span>a<h2>b"),
        "<h1><span>a<h2>b</h2></span></h1>"
    );
}

#[test]
fn head_is_closed_by_body_content() {
    assert_eq!(
        structure("<html><head><meta charset=utf-8><title>T</title><body><p>x"),
        "<html><head><meta charset=\"utf-8\"><title>T</title></head><body><p>x</p></body></html>"
    );
    assert_eq!(
        structure("<head><link rel=x><div>y</div>"),
        "<head><link rel=\"x\"></head><div>y</div>"
    );
    // Metadata, whitespace and comments stay in the head; real text does not.
    assert_eq!(
        structure("<head>\n<style>a{}</style><!--c--><script>s</script><noscript></noscript>Hello"),
        "<head>\n<style>a{}</style><!--c--><script>s</script><noscript></noscript></head>Hello"
    );
    // So the body survives in readable output instead of hiding inside <head>.
    assert_eq!(
        parse("<head><title>T</title><body><p>Visible").to_text(),
        "Visible"
    );
}

#[test]
fn ruby_annotations() {
    assert_eq!(
        structure("<ruby>漢<rp>(<rt>kan<rp>)</ruby>"),
        "<ruby>漢<rp>(</rp><rt>kan</rt><rp>)</rp></ruby>"
    );
    assert_eq!(
        structure("<ruby><rb>a<rb>b<rt>x<rt>y</ruby>"),
        "<ruby><rb>a</rb><rb>b</rb><rt>x</rt><rt>y</rt></ruby>"
    );
    // <rt> nests in <rtc>; <rb> and <rtc> close an <rtc>.
    assert_eq!(
        structure("<ruby><rb>a<rtc><rt>x<rt>y<rtc>z<rb>c</ruby>"),
        "<ruby><rb>a</rb><rtc><rt>x</rt><rt>y</rt></rtc><rtc>z</rtc><rb>c</rb></ruby>"
    );
}

#[test]
fn options_and_groups() {
    assert_eq!(
        structure(
            "<select><option>a<option>b<optgroup label=g><option>c<optgroup><option>d</select>"
        ),
        "<select><option>a</option><option>b</option><optgroup label=\"g\"><option>c</option></optgroup><optgroup><option>d</option></optgroup></select>"
    );
    assert_eq!(
        structure("<select><optgroup><option>a<hr><option>b</select>"),
        "<select><optgroup><option>a</option></optgroup><hr><option>b</option></select>"
    );
}

#[test]
fn table_sections_rows_and_cells_close_each_other() {
    assert_eq!(
        structure("<table><thead><tr><th>h<tbody><tr><td>a<td>b<tr><td>c<tfoot><tr><td>f</table>"),
        "<table><thead><tr><th>h</th></tr></thead><tbody><tr><td>a</td><td>b</td></tr><tr><td>c</td></tr></tbody><tfoot><tr><td>f</td></tr></tfoot></table>"
    );
    // A caption is closed by table structure, and closes open cells and rows.
    assert_eq!(
        structure("<table><caption>Cap<tr><td>a<caption>Again</table>"),
        "<table><caption>Cap</caption><tr><td>a</td></tr><caption>Again</caption></table>"
    );
    assert_eq!(
        structure("<table><tr><td>a<colgroup><col></table>"),
        "<table><tr><td>a</td></tr><colgroup><col></colgroup></table>"
    );
    // Nested tables inside a cell are fine.
    assert_eq!(
        structure("<table><tr><td><table><tr><td>in</table><td>out</table>"),
        "<table><tr><td><table><tr><td>in</td></tr></table></td><td>out</td></tr></table>"
    );
}

#[test]
fn colgroup_ends_at_non_col_content() {
    assert_eq!(
        structure("<table><colgroup><col><col span=2><tr><td>x</table>"),
        "<table><colgroup><col><col span=\"2\"></colgroup><tr><td>x</td></tr></table>"
    );
    assert_eq!(
        structure("<table><colgroup>\n<col>\n<thead>"),
        "<table><colgroup>\n<col>\n</colgroup><thead></thead></table>"
    );
    assert_eq!(
        structure("<table><colgroup><col>text</table>"),
        "<table><colgroup><col></colgroup>text</table>"
    );
}

#[test]
fn implicit_closes_are_reported() {
    let dom = parse("<h1>a<h2>b</h2>");
    assert!(
        dom.errors()
            .iter()
            .any(|e| e.kind == domtree::ErrorKind::ImplicitlyClosed && e.position == 5)
    );
}

fn kinds(html: &str) -> Vec<domtree::ErrorKind> {
    domtree::parse(html)
        .errors()
        .iter()
        .map(|e| e.kind.clone())
        .collect()
}

#[test]
fn valid_html_is_not_reported_as_an_error() {
    use domtree::ErrorKind::*;
    // A bare `&` is valid, in text and in attribute values (query strings).
    assert!(kinds("<p>a & b, 1&2</p>").is_empty());
    assert!(kinds(r#"<a href="?a=1&b=2&region=eu&not=x">x</a>"#).is_empty());
    // Ancestor end tags may close elements whose end tag is optional.
    assert!(!kinds("<ul><li>a<li>b</ul>").contains(&MismatchedEndTag));
    assert!(!kinds("<table><tr><td>x<td>y</table>").contains(&MismatchedEndTag));
    assert!(!kinds("<div><p>text</div>").contains(&MismatchedEndTag));
    assert!(kinds("<ul><li>a</ul>").contains(&ImplicitlyClosed));
    // An `&` followed by letters is plain text unless they start with one of
    // WHATWG's 106 legacy names (`amp`, `copy`, …; the only ones that work
    // without a `;`). `or`, `ne`, `sub`, `part` and `int` are names, but
    // only with their `;`.
    for html in [
        "<p>x&order=1</p>",
        "<p>Q&A: tom&nearby</p>",
        "<p>see page?a=1&sub=2&part=3</p>",
        "<p>a&int b</p>",
        "<p>a&lend</p>",
        "<p>Fish&chips, P&L, AT&T</p>",
        // In attributes a legacy name followed by an alphanumeric or `=` is
        // valid too.
        r#"<a href="/s?a=1&or">x</a>"#,
        r#"<a title="x &ne y">x</a>"#,
        r#"<a href="?a=1&copyright=2&amp=3&notify">x</a>"#,
    ] {
        assert_eq!(kinds(html), [], "{html}");
    }
}

#[test]
fn real_problems_are_still_reported() {
    use domtree::ErrorKind::*;
    assert!(kinds("<p>&nosuchentity;</p>").contains(&UnknownEntity));
    assert!(kinds("<p>&#xZZ;</p>").contains(&UnknownEntity));
    assert!(kinds("<p>AT&ampT, &copy 2026</p>").contains(&UnknownEntity));
    assert!(kinds(r#"<a title="&copy">x</a>"#).contains(&UnknownEntity));
    assert!(kinds("<div><b>bold</div>").contains(&MismatchedEndTag));
    assert!(kinds("<p>x</span>").contains(&StrayEndTag));
    for html in [
        "<p>AT&ampT</p>",
        "<p>&copy 2026</p>",
        "<p>a&notin b</p>", // `not` + `in`
        "<p>&nosuch;</p>",
        "<p>&#xZZ;</p>",
        "<p>&#169 x</p>", // numeric without `;`
        "<p>&#</p>",
        r#"<a title="&copy">x</a>"#,
        r#"<a href="?a=1&lt">x</a>"#,
        r#"<a title="&nosuch;x">x</a>"#,
    ] {
        assert_eq!(kinds(html), [UnknownEntity], "{html}");
    }
}

#[test]
fn unknown_entities_are_reported_at_their_ampersand() {
    let positions = |html: &str| -> Vec<usize> {
        let dom = parse(html);
        let mut events = domtree::events(html);
        for _ in events.by_ref() {}
        assert_eq!(events.errors(), dom.errors(), "{html}");
        dom.errors()
            .iter()
            .filter(|e| e.kind == domtree::ErrorKind::UnknownEntity)
            .map(|e| e.position)
            .collect()
    };
    let html = "<p>aaaa\nbbbb &bogus; cc</p>";
    assert_eq!(positions(html), [13]);
    assert_eq!(domtree::line_col(html, 13), (2, 6));
    // Only the first per text run or attribute value; multi-byte text before.
    let html = "<p>é &amp; &x; &y;</p><p>&#</p>";
    assert_eq!(
        positions(html),
        [html.find("&x").unwrap(), html.find("&#").unwrap()]
    );
    // Attribute values: at the `&` inside the value, quoted or not.
    let html = r#"<a  href='/?q=1&copy' title=a&bogus;b>&lt</a>"#;
    assert_eq!(
        positions(html),
        [
            html.find("&copy").unwrap(),
            html.find("&bogus").unwrap(),
            html.find("&lt").unwrap(),
        ]
    );
    // In `<title>` and `<textarea>` (decoded raw text) too.
    let html = "<title>a &bogus;</title><textarea>\n&#xZZ;</textarea>";
    assert_eq!(positions(html), [9, html.find("&#").unwrap()]);
}
