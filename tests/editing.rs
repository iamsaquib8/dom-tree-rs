//! Tests for the editing API: `Dom::remove`, `set_attr`, `remove_attr`.

use domtree::parse;

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
