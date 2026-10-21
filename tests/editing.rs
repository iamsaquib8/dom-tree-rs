//! Tests for the editing API: removing, attributes, text, renaming, parsed
//! fragments, creating and moving nodes, wrap/unwrap — and the invariants
//! every edit must keep (document order, detached flags, top-level links).

use domtree::{Dom, NodeId, parse};

/// The document's node ids by following links from the roots — the reference
/// that `Dom::nodes()` must agree with after any edit.
fn walk(dom: &Dom) -> Vec<NodeId> {
    let mut out = Vec::new();
    for r in dom.roots() {
        out.push(r.id());
        out.extend(r.descendants().map(|d| d.id()));
    }
    out
}

/// `nodes()` is the link walk, links are symmetric, top-level nodes are
/// linked siblings, and `is_detached` is set on exactly those of `extra` (and
/// their subtrees) that the walk does not reach.
fn assert_consistent(dom: &Dom, extra: &[NodeId]) {
    let walked = walk(dom);
    let listed: Vec<_> = dom.nodes().map(|n| n.id()).collect();
    assert_eq!(listed, walked, "nodes() must be document order");
    #[cfg(feature = "query")]
    {
        let selected: Vec<_> = dom.select("*").map(|n| n.id()).collect();
        let elements: Vec<_> = dom
            .nodes()
            .filter(|n| n.is_element())
            .map(|n| n.id())
            .collect();
        assert_eq!(selected, elements);
    }
    let mut all = walked.clone();
    for &id in extra {
        let n = dom.get(id).unwrap();
        all.push(id);
        all.extend(n.descendants().map(|d| d.id()));
    }
    for &id in &all {
        let n = dom.get(id).unwrap();
        assert_eq!(
            n.is_detached(),
            !walked.contains(&id),
            "detached flag of {id:?}"
        );
        for c in n.children() {
            assert_eq!(c.parent().map(|p| p.id()), Some(id));
        }
        if let Some(next) = n.next_sibling() {
            assert_eq!(next.prev_sibling().map(|p| p.id()), Some(id));
        }
        if let Some(prev) = n.prev_sibling() {
            assert_eq!(prev.next_sibling().map(|p| p.id()), Some(id));
        }
        if n.is_detached() && n.parent().is_none() {
            assert!(n.prev_sibling().is_none() && n.next_sibling().is_none());
        }
    }
    let mut prev: Option<NodeId> = None;
    for r in dom.roots() {
        assert!(r.parent().is_none() && !r.is_detached());
        assert_eq!(r.prev_sibling().map(|p| p.id()), prev);
        prev = Some(r.id());
    }
    assert_eq!(
        dom.root().map(|r| r.id()),
        dom.roots().next().map(|r| r.id())
    );
}

fn tag_ids(dom: &Dom, tag: &str) -> Vec<NodeId> {
    dom.find_by_tag(tag).map(|n| n.id()).collect()
}

fn texts(dom: &Dom) -> Vec<&str> {
    dom.nodes().filter_map(|n| n.text()).collect()
}

#[test]
fn remove_unlinks_subtree_everywhere() {
    let mut dom = parse("<ul><li>a</li><li class=x>b <b>bold</b></li><li>c</li></ul>");
    let x = dom.find_by_class("x").next().unwrap().id();
    dom.remove(x);

    assert_eq!(dom.to_html(), "<ul><li>a</li><li>c</li></ul>");
    assert_eq!(dom.find_by_tag("li").count(), 2);
    assert_eq!(dom.find_by_tag("b").count(), 0);
    assert_eq!(dom.nodes().filter(|n| n.is_text()).count(), 2);

    let ul = dom.root().unwrap();
    let kids: Vec<_> = ul.children().map(|c| c.text_content()).collect();
    assert_eq!(kids, ["a", "c"]);
    assert_eq!(
        ul.last_child()
            .unwrap()
            .prev_sibling()
            .unwrap()
            .text_content(),
        "a"
    );

    // The removed node is still a valid, serializable handle.
    assert_eq!(
        dom.get(x).unwrap().to_html(),
        r#"<li class="x">b <b>bold</b></li>"#
    );
    assert!(dom.get(x).unwrap().parent().is_none());

    // Idempotent.
    dom.remove(x);
    assert_eq!(dom.to_html(), "<ul><li>a</li><li>c</li></ul>");
}

#[test]
fn remove_first_last_and_only_children() {
    let mut dom = parse("<div><i>1</i><i>2</i><i>3</i></div>");
    let ids: Vec<_> = dom.find_by_tag("i").map(|n| n.id()).collect();
    dom.remove(ids[0]);
    assert_eq!(dom.to_html(), "<div><i>2</i><i>3</i></div>");
    dom.remove(ids[2]);
    assert_eq!(dom.to_html(), "<div><i>2</i></div>");
    dom.remove(ids[1]);
    assert_eq!(dom.to_html(), "<div></div>");
    let div = dom.root().unwrap();
    assert!(div.first_child().is_none() && div.last_child().is_none());
}

#[test]
fn remove_top_level_nodes() {
    let mut dom = parse("<p>a</p><p>b</p><p>c</p>");
    let b = dom.find_by_tag("p").nth(1).unwrap().id();
    dom.remove(b);
    assert_eq!(dom.to_html(), "<p>a</p><p>c</p>");
    assert_eq!(dom.roots().count(), 2);
    let first = dom.root().unwrap();
    assert_eq!(first.next_sibling().unwrap().text_content(), "c");
}

#[test]
fn top_level_nodes_are_siblings() {
    let dom = parse("<!doctype html><p>a</p>text<p>b</p>");
    let roots: Vec<_> = dom.roots().collect();
    assert_eq!(roots.len(), 4);
    for pair in roots.windows(2) {
        assert_eq!(pair[0].next_sibling().map(|n| n.id()), Some(pair[1].id()));
        assert_eq!(pair[1].prev_sibling().map(|n| n.id()), Some(pair[0].id()));
    }
    // Serializing one root must not run into its siblings.
    assert_eq!(roots[1].to_html(), "<p>a</p>");
    assert_eq!(roots[1].to_text(), "a");
    assert_eq!(roots[1].descendants().count(), 1);
    #[cfg(feature = "markdown")]
    assert_eq!(roots[1].to_markdown(), "a");
    #[cfg(feature = "json")]
    assert_eq!(
        roots[1].to_json(),
        r#"{"type":"element","tag":"p","attrs":{},"children":[{"type":"text","value":"a"}]}"#
    );
}

#[test]
fn set_and_remove_attributes() {
    let mut dom = parse("<img SRC=a.png alt=''>");
    let img = dom.root().unwrap().id();
    dom.set_attr(img, "src", "b.png");
    dom.set_attr(img, "Loading", "lazy");
    assert_eq!(dom.to_html(), r#"<img src="b.png" alt loading="lazy">"#);
    assert_eq!(dom.root().unwrap().attr("loading"), Some("lazy"));

    dom.remove_attr(img, "ALT");
    assert_eq!(dom.to_html(), r#"<img src="b.png" loading="lazy">"#);
    dom.remove_attr(img, "missing");
    assert_eq!(dom.to_html(), r#"<img src="b.png" loading="lazy">"#);
}

#[test]
fn attribute_edits_ignore_non_elements() {
    let mut dom = parse("text");
    let t = dom.root().unwrap().id();
    dom.set_attr(t, "a", "b");
    dom.remove_attr(t, "a");
    assert_eq!(dom.to_html(), "text");
}

#[cfg(feature = "query")]
#[test]
fn select_skips_removed_nodes_and_sees_sibling_roots() {
    let mut dom = parse("<li>a</li><li>b</li><li>c</li>");
    assert_eq!(dom.select("li + li").count(), 2);
    assert_eq!(dom.select("li:first-child").count(), 1);
    assert_eq!(
        dom.select_first("li:last-child").unwrap().text_content(),
        "c"
    );

    let ids: Vec<_> = dom.select("li:nth-child(2)").map(|n| n.id()).collect();
    for id in ids {
        dom.remove(id);
    }
    let left: Vec<_> = dom.select("li").map(|n| n.text_content()).collect();
    assert_eq!(left, ["a", "c"]);
    assert_eq!(dom.select("li + li").count(), 1);
}

#[test]
fn removing_nested_chains_is_linear() {
    // Ancestors first, like `domtree -r div`: each later remove() hits an
    // already-detached node and must not re-walk its subtree.
    let html = "<div>".repeat(20_000);
    let mut dom = parse(&html);
    let ids: Vec<_> = dom.find_by_tag("div").map(|n| n.id()).collect();
    let start = std::time::Instant::now();
    for id in ids {
        dom.remove(id);
    }
    assert!(start.elapsed().as_secs() < 2, "took {:?}", start.elapsed());
    assert_eq!(dom.nodes().count(), 0);
    assert_eq!(dom.roots().count(), 0);

    // Many top-level removals don't shift the roots vector each time.
    let mut dom = parse(&"<p>x</p>".repeat(50_000));
    let ids: Vec<_> = dom.find_by_tag("p").map(|n| n.id()).collect();
    let start = std::time::Instant::now();
    for id in ids.iter().step_by(2) {
        dom.remove(*id);
    }
    assert!(start.elapsed().as_secs() < 2, "took {:?}", start.elapsed());
    assert_eq!(dom.roots().count(), 25_000);
    assert_eq!(dom.root().map(|r| r.id()), Some(ids[1]));
}

#[test]
fn debug_output_survives_deep_nesting() {
    let dom = parse(&"<b>".repeat(100_000));
    let s = format!("{dom:?}");
    assert!(s.contains('…'));
}

// --- set_text ------------------------------------------------------------

#[test]
fn set_text_on_text_and_elements() {
    let mut dom = parse("<div><p>old <b>bold</b> tail</p><br><!--c--></div>");
    let p = tag_ids(&dom, "p")[0];
    let b = tag_ids(&dom, "b")[0];
    dom.set_text(p, "new <text> & more");
    assert_eq!(
        dom.to_html(),
        "<div><p>new &lt;text&gt; &amp; more</p><br><!--c--></div>"
    );
    // The old children are detached, not visible anywhere.
    assert!(dom.get(b).unwrap().is_detached());
    assert!(tag_ids(&dom, "b").is_empty());
    assert_eq!(dom.get(p).unwrap().children().count(), 1);
    assert_consistent(&dom, &[b]);

    // Text node: content replaced in place (same id).
    let t = dom.get(p).unwrap().first_child().unwrap().id();
    dom.set_text(t, "again");
    assert_eq!(dom.get(p).unwrap().first_child().unwrap().id(), t);
    assert_eq!(dom.get(t).unwrap().text(), Some("again"));

    // Empty text empties an element.
    dom.set_text(p, "");
    assert_eq!(dom.get(p).unwrap().children().count(), 0);

    // Void elements, comments: no-ops.
    let br = tag_ids(&dom, "br")[0];
    dom.set_text(br, "x");
    let comment = dom.nodes().find(|n| n.comment().is_some()).unwrap().id();
    dom.set_text(comment, "x");
    assert_eq!(dom.to_html(), "<div><p></p><br><!--c--></div>");
    assert_consistent(&dom, &[b, t]);
}

#[test]
fn set_text_on_script_stays_raw() {
    let mut dom = parse("<script>old()</script>");
    let s = dom.root().unwrap().id();
    dom.set_text(s, "if (a < b) go()");
    assert_eq!(dom.to_html(), "<script>if (a < b) go()</script>");
}

/// The tag names of `html` re-parsed, in document order.
fn reparsed_tags(html: &str) -> Vec<String> {
    parse(html)
        .nodes()
        .filter_map(|n| n.tag_name().map(str::to_string))
        .collect()
}

#[test]
fn set_text_cannot_close_a_script_or_style_early() {
    let mut dom = parse("<script>var a=1</script><p>ok</p>");
    let s = dom.root().unwrap().id();
    dom.set_text(s, "x</script><img src=x onerror=alert(1)><script>");
    let html = dom.to_html();
    assert_eq!(
        html,
        "<script>x<\\/script><img src=x onerror=alert(1)><script></script><p>ok</p>"
    );
    assert_eq!(reparsed_tags(&html), ["script", "p"]);
    let reparsed = parse(&html);
    let script = reparsed.root().unwrap();
    assert_eq!(script.children().count(), 1);
    assert_eq!(
        script.text_content(),
        "x<\\/script><img src=x onerror=alert(1)><script>"
    );

    // ASCII case-insensitive, any end-tag delimiter; other names and
    // non-delimited names are left alone.
    dom.set_text(s, "a</SCRIPT >b</Script/c</script\td</scriptx></style></");
    assert_eq!(
        dom.get(s).unwrap().to_html(),
        "<script>a<\\/SCRIPT >b<\\/Script/c<\\/script\td</scriptx></style></</script>"
    );
    // The serializer, not set_text, does it: the text itself is unchanged.
    assert_eq!(
        dom.get(s).unwrap().text_content(),
        "a</SCRIPT >b</Script/c</script\td</scriptx></style></"
    );

    // create_text + append_child into a <style>.
    let mut dom = parse("<style></style>");
    let st = dom.root().unwrap().id();
    let t = dom.create_text("</style><b>bold</b>");
    assert!(dom.append_child(st, t));
    assert_eq!(dom.to_html(), "<style><\\/style><b>bold</b></style>");
    assert_eq!(reparsed_tags(&dom.to_html()), ["style"]);
    // inner_html is the content to_html writes.
    assert_eq!(dom.root().unwrap().inner_html(), "<\\/style><b>bold</b>");
    // A text node's own HTML is its text, raw in a <style>.
    assert_eq!(dom.get(t).unwrap().to_html(), "</style><b>bold</b>");
}

#[test]
fn nothing_moved_into_a_raw_text_element_closes_it_early() {
    // An end tag split across two text nodes.
    let mut dom = parse("<script></script>");
    let s = dom.root().unwrap().id();
    let a = dom.create_text("x<");
    let b = dom.create_text("/script><img src=x onerror=alert(1)>");
    dom.append_child(s, a);
    dom.append_child(s, b);
    assert_eq!(
        dom.to_html(),
        "<script>x<\\/script><img src=x onerror=alert(1)></script>"
    );
    assert_eq!(reparsed_tags(&dom.to_html()), ["script"]);

    // A parsed comment moved into a script.
    let mut dom = parse("<p><!--</script><img src=x onerror=alert(1)>--></p><script></script>");
    let c = dom.nodes().find(|n| n.comment().is_some()).unwrap().id();
    let s = tag_ids(&dom, "script")[0];
    assert!(dom.append_child(s, c));
    assert_eq!(reparsed_tags(&dom.to_html()), ["p", "script"]);

    // An element whose attribute value holds the end tag. Element children
    // of a raw-text element come back as text, but never as live markup.
    let mut dom = parse("<style></style>");
    let st = dom.root().unwrap().id();
    dom.append_html(st, "<b title='</style><img src=x onerror=alert(1)>'>b</b>");
    assert_eq!(reparsed_tags(&dom.to_html()), ["style"]);
    assert_eq!(
        dom.root().unwrap().inner_html(),
        r#"<b title="<\/style><img src=x onerror=alert(1)>">b</b>"#
    );

    // <textarea> and <title> also end only at their end tag; their text is
    // escaped, but elements and comments moved in are not.
    let mut dom = parse("<textarea></textarea><title></title>");
    let ta = tag_ids(&dom, "textarea")[0];
    let title = tag_ids(&dom, "title")[0];
    dom.append_html(ta, "<b title='</textarea><img src=x onerror=alert(1)>'>");
    dom.append_html(title, "<!--</TITLE><img src=x onerror=alert(1)>-->");
    let html = dom.to_html();
    assert_eq!(
        html,
        r#"<textarea><b title="<\/textarea><img src=x onerror=alert(1)>"></b></textarea><title><!--<\/TITLE><img src=x onerror=alert(1)>--></title>"#
    );
    assert_eq!(reparsed_tags(&html), ["textarea", "title"]);

    // Nested: only the outermost element's end tag can end the text.
    let mut dom = parse("<style></style>");
    let st = dom.root().unwrap().id();
    let inner = dom.create_element("script").unwrap();
    dom.set_text(inner, "</script></style><img src=x onerror=alert(1)>");
    dom.append_child(st, inner);
    assert_eq!(
        dom.to_html(),
        "<style><script></script><\\/style><img src=x onerror=alert(1)></script></style>"
    );
    assert_eq!(reparsed_tags(&dom.to_html()), ["style"]);
}

// --- rename --------------------------------------------------------------

#[test]
fn rename_elements_only() {
    let mut dom = parse("<B id=x>bold</B>text");
    let b = dom.root().unwrap().id();
    dom.rename(b, "Strong");
    assert_eq!(dom.root().unwrap().tag_name(), Some("strong"));
    let text = dom.roots().nth(1).unwrap().id();
    dom.rename(text, "div");
    assert_eq!(dom.to_html(), r#"<strong id="x">bold</strong>text"#);
    assert_eq!(dom.find_by_tag("strong").count(), 1);
    assert_eq!(dom.find_by_tag("b").count(), 0);
}

// --- name validation -----------------------------------------------------

/// Names that would break out of a tag if written as they are.
const BAD_NAMES: [&str; 13] = [
    "",
    "x y",
    "x\ty",
    "x\ny",
    "a\"b",
    "a'b",
    "a<b",
    "a>b",
    "a/b",
    "a=b",
    "a\0b",
    "x><script>alert(1)</script><i y",
    "b><img src=x onerror=alert(1)",
];

#[test]
fn set_attr_refuses_invalid_names() {
    let mut dom = parse("<p>hi</p>");
    let p = dom.root().unwrap().id();
    for name in BAD_NAMES {
        dom.set_attr(p, name, "v");
        assert_eq!(dom.to_html(), "<p>hi</p>", "{name:?}");
    }
    // Everything else is fine, digits and non-ASCII included.
    for name in ["data-x", "x:y", "_a.b", "1x", "@click", "data-é"] {
        dom.set_attr(p, name, "v");
        assert_eq!(dom.root().unwrap().attr(name), Some("v"), "{name:?}");
    }
    assert_eq!(
        dom.to_html(),
        r#"<p data-x="v" x:y="v" _a.b="v" 1x="v" @click="v" data-é="v">hi</p>"#
    );
    // remove_attr needs no validation: no name it is given gets written.
    dom.remove_attr(p, "");
    dom.remove_attr(p, "a b");
    assert_eq!(dom.root().unwrap().attributes().count(), 6);
}

#[test]
fn rename_refuses_invalid_names() {
    let mut dom = parse("<p class=x>hi</p>");
    let p = dom.root().unwrap().id();
    let tag_only = ["1x", "-x", "_x", ":x", "!--", "?x", "éx"];
    for name in BAD_NAMES.iter().chain(&tag_only) {
        dom.rename(p, name);
        assert_eq!(dom.to_html(), r#"<p class="x">hi</p>"#, "{name:?}");
    }
    for name in ["H1", "my-élément", "svg:rect", "x1"] {
        dom.rename(p, name);
        assert_eq!(
            dom.root().unwrap().tag_name(),
            Some(name.to_ascii_lowercase().as_str())
        );
    }
}

#[test]
fn a_name_that_is_not_a_tag_cannot_expose_attribute_values() {
    // After `<`, only an ASCII letter starts a tag: `<1x title="...">` would
    // be text, and the attribute value parsed as markup.
    let mut dom = parse("<p>hi</p>");
    let p = dom.root().unwrap().id();
    dom.set_attr(p, "title", "--><img src=x onerror=alert(1)>");
    for name in ["1x", "!--", "?x"] {
        dom.rename(p, name);
    }
    let html = dom.to_html();
    assert_eq!(reparsed_tags(&html), ["p"]);
}

#[test]
fn create_element_and_wrap_refuse_invalid_names() {
    let mut dom = parse("<p>hi</p>");
    let text = dom.root().unwrap().first_child().unwrap().id();
    let len = dom.len();
    for name in BAD_NAMES.iter().chain(&["1x", "!x"]) {
        assert!(dom.create_element(name).is_none(), "{name:?}");
        assert!(dom.wrap(text, name).is_none(), "{name:?}");
    }
    assert_eq!(dom.len(), len, "nothing was allocated");
    assert_eq!(dom.to_html(), "<p>hi</p>");
    assert_consistent(&dom, &[]);

    let em = dom.wrap(text, "EM").unwrap();
    assert_eq!(dom.get(em).unwrap().tag_name(), Some("em"));
    assert_eq!(dom.to_html(), "<p><em>hi</em></p>");
}

// --- parsed fragments ----------------------------------------------------

#[test]
fn append_html_cost_does_not_grow_with_the_parents_depth() {
    // One cycle check per fragment root would climb all `n` ancestors for
    // each of the `n` roots: ~3.4 s at n = 40k in release, ~20 s in debug.
    let n = 40_000;
    let mut dom = parse(&"<div>".repeat(n));
    let deepest = dom.nodes().last().unwrap().id();
    let frag = "<i></i>".repeat(n);
    let start = std::time::Instant::now();
    let ids = dom.append_html(deepest, &frag);
    assert!(start.elapsed().as_secs() < 2, "took {:?}", start.elapsed());
    assert_eq!(ids.len(), n);
    assert_eq!(dom.get(deepest).unwrap().children().count(), n);
    assert_eq!(dom.find_by_tag("i").count(), n);
    assert!(ids.iter().all(|&id| !dom.get(id).unwrap().is_detached()));

    // Into a detached parent: linked, but left detached.
    let card = dom.create_element("section").unwrap();
    let ids = dom.append_html(card, "<i>a</i>b");
    assert!(ids.iter().all(|&id| dom.get(id).unwrap().is_detached()));
    assert_eq!(
        dom.get(card).unwrap().to_html(),
        "<section><i>a</i>b</section>"
    );
}

#[test]
fn fragment_insertion_in_all_four_positions() {
    let mut dom = parse("<ul><li>c</li></ul>");
    let ul = dom.root().unwrap().id();
    let c = tag_ids(&dom, "li")[0];

    let ids = dom.append_html(ul, "<li>e<li>f");
    assert_eq!(ids.len(), 2);
    let ids = dom.prepend_html(ul, "<li>a</li><li>b</li>");
    assert_eq!(ids.len(), 2);
    let ids = dom.insert_html_after(c, "<li>d</li>");
    assert_eq!(ids.len(), 1);
    let ids = dom.insert_html_before(c, "<!--before c-->");
    assert_eq!(dom.get(ids[0]).unwrap().comment(), Some("before c"));

    assert_eq!(
        dom.to_html(),
        "<ul><li>a</li><li>b</li><!--before c--><li>c</li><li>d</li><li>e</li><li>f</li></ul>"
    );
    assert_eq!(texts(&dom), ["a", "b", "c", "d", "e", "f"]);
    let items: Vec<_> = dom.find_by_tag("li").map(|n| n.text_content()).collect();
    assert_eq!(items, ["a", "b", "c", "d", "e", "f"]);
    assert_consistent(&dom, &[]);
}

#[test]
fn fragment_insertion_returns_top_level_ids_in_order() {
    let mut dom = parse("<div></div>");
    let div = dom.root().unwrap().id();
    let ids = dom.append_html(div, "one<b>two</b><!--three-->four");
    let kinds: Vec<_> = ids
        .iter()
        .map(|&id| dom.get(id).unwrap().outer_html())
        .collect();
    assert_eq!(kinds, ["one", "<b>two</b>", "<!--three-->", "four"]);
    assert!(dom.append_html(div, "").is_empty());
    assert_consistent(&dom, &[]);
}

#[test]
fn fragment_strings_are_reinterned_and_entities_decoded() {
    let mut dom = parse("<p>x</p>");
    let p = dom.root().unwrap().id();
    dom.append_html(
        p,
        r#" <a href="/q?a=1&amp;b=2" TITLE='T&eacute;'>Tom &amp; Jerry</a>"#,
    );
    let a = dom.find_by_tag("a").next().unwrap();
    assert_eq!(a.attr("href"), Some("/q?a=1&b=2"));
    assert_eq!(a.attr("title"), Some("Té"));
    assert_eq!(a.text_content(), "Tom & Jerry");
    assert_eq!(
        dom.to_html(),
        r#"<p>x <a href="/q?a=1&amp;b=2" title="Té">Tom &amp; Jerry</a></p>"#
    );
}

#[test]
fn fragment_parse_errors_are_not_added_and_nodes_have_no_source() {
    let src = "<p>x</p>";
    let mut dom = parse(src);
    assert!(dom.errors().is_empty());
    let p = dom.root().unwrap().id();
    let ids = dom.append_html(p, "<b><i>mis</b>nested</i> &bogus; </stray>");
    assert!(
        dom.errors().is_empty(),
        "fragment errors leak: {:?}",
        dom.errors()
    );
    for &id in &ids {
        let n = dom.get(id).unwrap();
        assert!(n.source_range().is_none());
        assert!(n.descendants().all(|d| d.source_range().is_none()));
    }
    // Original nodes keep theirs.
    assert_eq!(dom.root().unwrap().source_range(), Some(0..src.len()));
}

#[test]
fn fragment_insertion_refuses_impossible_targets() {
    let mut dom = parse("<p>text<br></p>");
    let text = dom.root().unwrap().first_child().unwrap().id();
    let br = tag_ids(&dom, "br")[0];
    let before = dom.len();
    assert!(dom.append_html(text, "<b>x</b>").is_empty());
    assert!(dom.prepend_html(br, "<b>x</b>").is_empty());
    // A detached node with no parent has no siblings to insert next to.
    let orphan = dom.create_element("div").unwrap();
    assert!(dom.insert_html_before(orphan, "<b>x</b>").is_empty());
    assert!(dom.insert_html_after(orphan, "<b>x</b>").is_empty());
    assert_eq!(dom.len(), before + 1); // only the orphan was allocated
    assert_eq!(dom.to_html(), "<p>text<br></p>");
}

#[test]
fn fragment_insertion_at_the_top_level() {
    let mut dom = parse("<p>b</p>");
    let p = dom.root().unwrap().id();
    dom.insert_html_before(p, "<!doctype html><p>a</p>");
    dom.insert_html_after(p, "<p>c</p>tail");
    assert_eq!(dom.to_html(), "<!doctype html><p>a</p><p>b</p><p>c</p>tail");
    assert_eq!(dom.roots().count(), 5);
    assert_eq!(dom.root().unwrap().to_html(), "<!doctype html>");
    assert_eq!(texts(&dom), ["a", "b", "c", "tail"]);
    #[cfg(feature = "query")]
    assert_eq!(dom.select("p + p").count(), 2);
    assert_consistent(&dom, &[]);
}

#[test]
fn fragment_into_a_detached_subtree_stays_detached() {
    let mut dom = parse("<main></main>");
    let main = dom.root().unwrap().id();
    let card = dom.create_element("section").unwrap();
    let ids = dom.append_html(card, "<h2>Title</h2><p>Body</p>");
    assert!(ids.iter().all(|&id| dom.get(id).unwrap().is_detached()));
    assert_eq!(dom.nodes().count(), 1);
    assert!(dom.find_by_tag("h2").next().is_none());
    assert_consistent(&dom, &[card]);

    assert!(dom.append_child(main, card));
    assert_eq!(dom.find_by_tag("h2").count(), 1);
    assert_eq!(
        dom.to_html(),
        "<main><section><h2>Title</h2><p>Body</p></section></main>"
    );
    assert_consistent(&dom, &[]);
}

// --- create / append_child / insert_before -------------------------------

#[test]
fn created_nodes_are_detached_until_attached() {
    let mut dom = parse("<p>a</p>");
    let before = dom.nodes().count();
    let em = dom.create_element("EM").unwrap();
    let t = dom.create_text("hi");
    assert!(dom.get(em).unwrap().is_detached());
    assert_eq!(dom.get(em).unwrap().tag_name(), Some("em"));
    assert!(dom.get(em).unwrap().source_range().is_none());
    assert_eq!(dom.nodes().count(), before);
    assert_eq!(dom.find_by_tag("em").count(), 0);

    assert!(dom.append_child(em, t)); // detached into detached
    assert!(dom.get(t).unwrap().is_detached());
    assert_eq!(dom.get(em).unwrap().to_html(), "<em>hi</em>");

    let p = dom.root().unwrap().id();
    assert!(dom.append_child(p, em));
    assert!(!dom.get(t).unwrap().is_detached());
    assert_eq!(dom.to_html(), "<p>a<em>hi</em></p>");
    assert_consistent(&dom, &[]);
}

#[test]
fn moving_nodes_around_keeps_document_order() {
    let mut dom = parse("<ol><li>1</li><li>2</li><li>3</li></ol><ul><li>4</li></ul>");
    let li = tag_ids(&dom, "li");
    let ul = tag_ids(&dom, "ul")[0];

    // Move the first item to the end of the other list.
    assert!(dom.append_child(ul, li[0]));
    assert_eq!(texts(&dom), ["2", "3", "4", "1"]);
    // ...and the last one back to the front of the first list.
    assert!(dom.insert_before(li[1], li[0]));
    assert_eq!(texts(&dom), ["1", "2", "3", "4"]);
    // Swap two siblings.
    assert!(dom.insert_before(li[1], li[2]));
    assert_eq!(texts(&dom), ["1", "3", "2", "4"]);
    // A whole list before the other, at the top level.
    let ol = tag_ids(&dom, "ol")[0];
    assert!(dom.insert_before(ol, ul));
    assert_eq!(texts(&dom), ["4", "1", "3", "2"]);
    assert_eq!(dom.root().unwrap().id(), ul);
    assert_eq!(
        dom.to_html(),
        "<ul><li>4</li></ul><ol><li>1</li><li>3</li><li>2</li></ol>"
    );
    #[cfg(feature = "query")]
    {
        let order: Vec<_> = dom.select("li").map(|n| n.text_content()).collect();
        assert_eq!(order, ["4", "1", "3", "2"]);
        assert_eq!(dom.select_first("li").unwrap().text_content(), "4");
        assert_eq!(dom.select("ul + ol > li:first-child").count(), 1);
    }
    let found: Vec<_> = dom.find_by_tag("li").map(|n| n.text_content()).collect();
    assert_eq!(found, ["4", "1", "3", "2"]);
    assert_consistent(&dom, &[]);
}

#[test]
fn moves_into_own_subtree_are_refused() {
    let mut dom = parse("<div id=a><div id=b><p id=c>x</p></div></div>");
    let a = dom.find_by_id("a").unwrap().id();
    let b = dom.find_by_id("b").unwrap().id();
    let c = dom.find_by_id("c").unwrap().id();
    let html = dom.to_html();

    assert!(!dom.append_child(a, a));
    assert!(!dom.append_child(b, a));
    assert!(!dom.append_child(c, a));
    assert!(!dom.insert_before(c, a));
    assert!(!dom.insert_before(b, b));
    assert!(dom.wrap(a, "section").is_some()); // wrapping is fine
    let section = dom.root().unwrap().id();
    assert!(!dom.append_child(c, section));
    assert_eq!(dom.to_html(), format!("<section>{html}</section>"));
    assert_consistent(&dom, &[]);
}

#[test]
fn append_child_refuses_non_containers() {
    let mut dom = parse("<p>t<img><!--c--></p>");
    let p = dom.root().unwrap().id();
    let kids: Vec<_> = dom.root().unwrap().children().map(|n| n.id()).collect();
    let x = dom.create_element("x").unwrap();
    for &k in &kids {
        assert!(!dom.append_child(k, x));
    }
    assert!(dom.get(x).unwrap().is_detached());
    assert!(dom.append_child(p, x));
    assert_eq!(dom.to_html(), "<p>t<img><!--c--><x></x></p>");
}

#[test]
fn insert_before_a_detached_orphan_is_refused() {
    let mut dom = parse("<p>a</p>");
    let p = dom.root().unwrap().id();
    let orphan = dom.create_element("div").unwrap();
    assert!(!dom.insert_before(orphan, p));
    dom.remove(p);
    let q = dom.create_element("q").unwrap();
    assert!(!dom.insert_before(p, q)); // removed root: no longer top-level
    assert_eq!(dom.roots().count(), 0);
}

#[test]
fn removed_subtrees_can_be_reattached() {
    let mut dom = parse("<div><p>a <b>b</b></p></div><footer></footer>");
    let p = tag_ids(&dom, "p")[0];
    let b = tag_ids(&dom, "b")[0];
    dom.remove(p);
    assert!(dom.get(b).unwrap().is_detached());
    let footer = tag_ids(&dom, "footer")[0];
    assert!(dom.append_child(footer, p));
    assert!(!dom.get(b).unwrap().is_detached());
    assert_eq!(
        dom.to_html(),
        "<div></div><footer><p>a <b>b</b></p></footer>"
    );
    // Removing a node inside a removed subtree only unlinks it.
    dom.remove(footer);
    dom.remove(b);
    assert_eq!(
        dom.get(footer).unwrap().to_html(),
        "<footer><p>a </p></footer>"
    );
    assert_consistent(&dom, &[footer, b]);
}

// --- wrap / unwrap -------------------------------------------------------

#[test]
fn wrap_inline_top_level_and_detached() {
    let mut dom = parse("<p>Hello <b>world</b>!</p><hr>");
    let b = tag_ids(&dom, "b")[0];
    let a = dom.wrap(b, "A").unwrap();
    assert_eq!(dom.get(a).unwrap().tag_name(), Some("a"));
    let text = dom.root().unwrap().first_child().unwrap().id();
    let em = dom.wrap(text, "em").unwrap();
    assert_eq!(
        dom.to_html(),
        "<p><em>Hello </em><a><b>world</b></a>!</p><hr>"
    );

    let p = dom.root().unwrap().id();
    let article = dom.wrap(p, "article").unwrap();
    assert_eq!(dom.root().unwrap().id(), article);
    assert_eq!(dom.roots().count(), 2);
    assert_eq!(
        dom.to_html(),
        "<article><p><em>Hello </em><a><b>world</b></a>!</p></article><hr>"
    );

    assert!(dom.wrap(p, "br").is_none()); // void wrapper refused

    let orphan = dom.create_text("loose");
    let span = dom.wrap(orphan, "span").unwrap();
    assert!(dom.get(span).unwrap().is_detached());
    assert_eq!(dom.get(span).unwrap().to_html(), "<span>loose</span>");
    assert_consistent(&dom, &[em, span]);
}

#[test]
fn unwrap_moves_children_into_place() {
    let mut dom = parse("<div><p>a <span>b <i>c</i> d</span> e</p></div>");
    let span = tag_ids(&dom, "span")[0];
    assert!(dom.unwrap(span));
    assert_eq!(dom.to_html(), "<div><p>a b <i>c</i> d e</p></div>");
    assert!(dom.get(span).unwrap().is_detached());
    assert_eq!(dom.get(span).unwrap().children().count(), 0);
    assert_eq!(texts(&dom), ["a ", "b ", "c", " d", " e"]);
    assert!(!dom.unwrap(span)); // removed: nowhere to put children

    // Unwrapping a root promotes its children to the top level.
    let div = dom.root().unwrap().id();
    assert!(dom.unwrap(div));
    assert_eq!(dom.roots().count(), 1);
    assert_eq!(dom.root().unwrap().tag_name(), Some("p"));
    let p = dom.root().unwrap().id();
    assert!(dom.unwrap(p));
    assert_eq!(dom.roots().count(), 5);
    assert_eq!(dom.to_html(), "a b <i>c</i> d e");
    assert_consistent(&dom, &[span, div, p]);

    // Text is not unwrappable.
    let t = dom.root().unwrap().id();
    assert!(!dom.unwrap(t));
}

#[test]
fn unwrap_and_remove_keep_arena_order_fast_path_correct() {
    // Edits that don't reorder keep `nodes()` on the arena scan; it must
    // still agree with the link walk.
    let mut dom = parse("<a><b><c>1</c><d>2</d></b><e>3</e></a><f>4</f>");
    let b = tag_ids(&dom, "b")[0];
    let d = tag_ids(&dom, "d")[0];
    dom.unwrap(b);
    dom.remove(d);
    dom.rename(tag_ids(&dom, "e")[0], "x");
    let t = dom.nodes().find(|n| n.text() == Some("3")).unwrap().id();
    dom.set_text(t, "three");
    assert_eq!(texts(&dom), ["1", "three", "4"]);
    assert_consistent(&dom, &[b, d]);
}

// --- serializers after edits ---------------------------------------------

#[test]
fn serializers_see_the_edited_document() {
    let mut dom =
        parse("<article><h1>Old</h1><p>Keep <b>this</b>.</p><div class=ad>buy</div></article>");
    let h1 = tag_ids(&dom, "h1")[0];
    dom.set_text(h1, "New");
    let ad = dom.find_by_class("ad").next().unwrap().id();
    dom.remove(ad);
    let p = tag_ids(&dom, "p")[0];
    dom.insert_html_after(p, "<ul><li>one<li>two</ul>");
    let b = tag_ids(&dom, "b")[0];
    dom.wrap(b, "em").unwrap();
    dom.unwrap(b);

    assert_eq!(
        dom.to_html(),
        "<article><h1>New</h1><p>Keep <em>this</em>.</p><ul><li>one</li><li>two</li></ul></article>"
    );
    assert_eq!(dom.to_text(), "New\n\nKeep this.\n\none\ntwo");
    #[cfg(feature = "markdown")]
    assert_eq!(dom.to_markdown(), "# New\n\nKeep *this*.\n\n- one\n- two");
    #[cfg(feature = "json")]
    {
        let json = dom.to_json();
        assert!(json.contains(r#""tag":"em""#));
        assert!(!json.contains("buy"));
        assert!(!json.contains(r#""tag":"b""#));
    }
    // Round-trips through the parser.
    assert_eq!(parse(&dom.to_html()).to_html(), dom.to_html());
    assert_consistent(&dom, &[ad, b]);
}

#[test]
fn editing_deep_trees_does_not_recurse() {
    let depth = 50_000;
    let mut html = String::from("<body></body>");
    html.push_str(&"<div>".repeat(depth));
    let mut dom = parse(&html);
    let body = dom.root().unwrap().id();
    let divs = tag_ids(&dom, "div");
    let (outer, innermost) = (divs[0], divs[depth - 1]);

    // Every step walks or climbs a 50k-deep chain; all of it is iterative.
    let main = dom.wrap(outer, "main").unwrap();
    assert!(!dom.append_child(innermost, main)); // the cycle check climbs it
    dom.remove(main); // flags the chain detached
    assert!(dom.get(innermost).unwrap().is_detached());
    assert_eq!(dom.nodes().count(), 1);
    assert_eq!(dom.append_html(innermost, "<span>x</span>").len(), 1);
    assert!(dom.append_child(body, main)); // and attached again
    assert_eq!(dom.find_by_tag("span").count(), 1);
    assert_eq!(dom.nodes().count(), depth + 4); // body, main, divs, span, "x"
    assert!(dom.to_html().len() > depth);
    dom.set_text(outer, "flat"); // detaches a 50k-deep subtree
    assert_eq!(dom.to_html(), "<body><main><div>flat</div></main></body>");
}
