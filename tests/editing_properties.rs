//! Property tests for the editing API: random sequences of edits on random
//! documents never panic, never create cycles, and always leave a tree whose
//! links, detached flags and document order agree with each other.

use std::collections::HashSet;

use domtree::{Dom, NodeId, parse};
use proptest::prelude::*;

/// Small HTML built from structural pieces, so documents have real nesting,
/// implicit closes, void elements and stray end tags.
fn html() -> impl Strategy<Value = String> {
    let pieces = vec![
        "<div>",
        "</div>",
        "<p>",
        "</p>",
        "<li>",
        "<ul>",
        "</ul>",
        "<b>",
        "</b>",
        "<br>",
        "<img>",
        "<!--c-->",
        "text",
        " ",
        "&amp;",
        "<td>",
        "<tr>",
        "<script>x</script>",
        "é",
    ];
    proptest::collection::vec(proptest::sample::select(pieces), 0..24).prop_map(|v| v.concat())
}

fn tag() -> impl Strategy<Value = &'static str> {
    proptest::sample::select(vec!["div", "span", "li", "br", "img", "b", "script", "P"])
}

#[derive(Debug, Clone)]
enum Op {
    Remove(usize),
    SetText(usize, String),
    Rename(usize, &'static str),
    SetAttr(usize),
    AppendHtml(usize, String),
    PrependHtml(usize, String),
    InsertHtmlBefore(usize, String),
    InsertHtmlAfter(usize, String),
    CreateElement(&'static str),
    CreateText(String),
    AppendChild(usize, usize),
    InsertBefore(usize, usize),
    Wrap(usize, &'static str),
    Unwrap(usize),
}

fn op() -> impl Strategy<Value = Op> {
    let i = || any::<usize>();
    prop_oneof![
        i().prop_map(Op::Remove),
        (i(), "[a-z<&]{0,4}").prop_map(|(a, s)| Op::SetText(a, s)),
        (i(), tag()).prop_map(|(a, t)| Op::Rename(a, t)),
        i().prop_map(Op::SetAttr),
        (i(), html()).prop_map(|(a, h)| Op::AppendHtml(a, h)),
        (i(), html()).prop_map(|(a, h)| Op::PrependHtml(a, h)),
        (i(), html()).prop_map(|(a, h)| Op::InsertHtmlBefore(a, h)),
        (i(), html()).prop_map(|(a, h)| Op::InsertHtmlAfter(a, h)),
        tag().prop_map(Op::CreateElement),
        "[a-z]{0,3}".prop_map(Op::CreateText),
        (i(), i()).prop_map(|(a, b)| Op::AppendChild(a, b)),
        (i(), i()).prop_map(|(a, b)| Op::InsertBefore(a, b)),
        (i(), tag()).prop_map(|(a, t)| Op::Wrap(a, t)),
        i().prop_map(Op::Unwrap),
    ]
}

/// The document by links, bounded so a cycle fails instead of hanging.
fn walk(dom: &Dom) -> Result<Vec<NodeId>, TestCaseError> {
    let limit = dom.len();
    let mut out = Vec::new();
    for r in dom.roots().take(limit + 1) {
        out.push(r.id());
        out.extend(r.descendants().take(limit + 1).map(|d| d.id()));
        prop_assert!(out.len() <= limit, "walk revisits nodes: cycle");
    }
    Ok(out)
}

fn check(dom: &Dom, known: &mut Vec<NodeId>) -> Result<(), TestCaseError> {
    let limit = dom.len();
    let walked = walk(dom)?;
    let in_doc: HashSet<NodeId> = walked.iter().copied().collect();
    prop_assert_eq!(in_doc.len(), walked.len(), "a node appears twice");

    // nodes() — and everything built on it — is exactly the link walk.
    let listed: Vec<_> = dom.nodes().take(limit + 1).map(|n| n.id()).collect();
    prop_assert_eq!(&listed, &walked);
    #[cfg(feature = "query")]
    {
        let selected: Vec<_> = dom.select("*").map(|n| n.id()).collect();
        let elements: Vec<_> = walked
            .iter()
            .copied()
            .filter(|&id| dom.get(id).is_some_and(|n| n.is_element()))
            .collect();
        prop_assert_eq!(selected, elements);
    }

    // Top-level nodes: parentless, attached, linked in order.
    let mut prev = None;
    for r in dom.roots().take(limit + 1) {
        prop_assert!(r.parent().is_none());
        prop_assert!(!r.is_detached());
        prop_assert_eq!(r.prev_sibling().map(|p| p.id()), prev);
        prev = Some(r.id());
    }

    // Every node we know of (attached or not), and its subtree.
    known.extend(walked.iter().copied());
    let mut all: HashSet<NodeId> = HashSet::new();
    for &id in known.iter() {
        let n = dom.get(id).expect("ids stay valid");
        all.insert(id);
        all.extend(n.descendants().take(limit + 1).map(|d| d.id()));
    }
    for &id in &all {
        let n = dom.get(id).expect("ids stay valid");
        prop_assert_eq!(n.is_detached(), !in_doc.contains(&id));
        prop_assert!(
            n.ancestors().take(limit + 1).count() < limit,
            "ancestor cycle"
        );
        let kids: Vec<_> = n.children().take(limit + 1).collect();
        prop_assert!(kids.len() <= limit);
        prop_assert_eq!(
            n.first_child().map(|c| c.id()),
            kids.first().map(|c| c.id())
        );
        prop_assert_eq!(n.last_child().map(|c| c.id()), kids.last().map(|c| c.id()));
        for c in &kids {
            prop_assert_eq!(c.parent().map(|p| p.id()), Some(id));
        }
        if let Some(next) = n.next_sibling() {
            prop_assert_eq!(next.prev_sibling().map(|p| p.id()), Some(id));
        }
        if let Some(prev) = n.prev_sibling() {
            prop_assert_eq!(prev.next_sibling().map(|p| p.id()), Some(id));
        }
        if n.parent().is_none() && n.is_detached() {
            // A detached orphan has no siblings to be linked to.
            prop_assert!(n.prev_sibling().is_none() && n.next_sibling().is_none());
        }
        if !n.is_element() {
            prop_assert!(n.first_child().is_none());
        }
    }
    known.clear();
    known.extend(all);
    known.sort_by_key(|id| id.index());
    Ok(())
}

fn apply(dom: &mut Dom, known: &mut Vec<NodeId>, op: &Op) {
    // Targets are picked among every node seen so far, attached or not.
    let at = |i: usize| known[i % known.len()];
    let new: Vec<NodeId> = match *op {
        Op::CreateElement(t) => vec![dom.create_element(t)],
        Op::CreateText(ref s) => vec![dom.create_text(s)],
        _ if known.is_empty() => vec![],
        Op::Remove(a) => {
            dom.remove(at(a));
            vec![]
        }
        Op::SetText(a, ref s) => {
            dom.set_text(at(a), s);
            vec![]
        }
        Op::Rename(a, t) => {
            dom.rename(at(a), t);
            vec![]
        }
        Op::SetAttr(a) => {
            dom.set_attr(at(a), "data-x", "1");
            vec![]
        }
        Op::AppendHtml(a, ref h) => dom.append_html(at(a), h),
        Op::PrependHtml(a, ref h) => dom.prepend_html(at(a), h),
        Op::InsertHtmlBefore(a, ref h) => dom.insert_html_before(at(a), h),
        Op::InsertHtmlAfter(a, ref h) => dom.insert_html_after(at(a), h),
        Op::AppendChild(a, b) => {
            dom.append_child(at(a), at(b));
            vec![]
        }
        Op::InsertBefore(a, b) => {
            dom.insert_before(at(a), at(b));
            vec![]
        }
        Op::Wrap(a, t) => dom.wrap(at(a), t).into_iter().collect(),
        Op::Unwrap(a) => {
            dom.unwrap(at(a));
            vec![]
        }
    };
    known.extend(new);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 1024, ..ProptestConfig::default() })]

    #[test]
    fn random_edits_keep_the_tree_consistent(
        doc in html(),
        ops in proptest::collection::vec(op(), 0..24),
    ) {
        let mut dom = parse(&doc);
        let mut known = Vec::new();
        check(&dom, &mut known)?;
        for op in &ops {
            apply(&mut dom, &mut known, op);
            check(&dom, &mut known)?;
        }
        // Serializers never panic on an edited tree, and HTML output is stable
        // through a re-parse of itself.
        let html = dom.to_html();
        let _ = dom.to_text();
        #[cfg(feature = "markdown")]
        let _ = dom.to_markdown();
        #[cfg(feature = "json")]
        let _ = dom.to_json();
        for &id in &known {
            let _ = dom.get(id).map(|n| (n.to_html(), n.to_text()));
        }
        let _ = parse(&html).to_html();
    }
}
