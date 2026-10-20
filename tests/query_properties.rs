//! Property tests for the selector engine: it never panics on any selector or
//! document, the `&str` and pre-parsed paths agree, and its non-backtracking
//! matcher agrees with a naive backtracking reference implementation.
#![cfg(feature = "query")]

use domtree::{NodeId, NodeRef, Selector, parse};
use proptest::prelude::*;
use proptest::sample::select;

/// Selector-ish soup: single structural characters plus whole tokens, so a
/// useful share of the strings actually parse.
const SELECTOR_ATOMS: &[&str] = &[
    "[",
    "]",
    "(",
    ")",
    ":",
    ">",
    "+",
    "~",
    ",",
    ".",
    "#",
    "*",
    "=",
    "\"",
    "'",
    " ",
    "\\",
    "^",
    "$",
    "|",
    "-",
    "_",
    "n",
    "1",
    "2",
    "i",
    "s",
    "of",
    "div",
    "p",
    "li",
    "a",
    "span",
    "x",
    "id",
    "class",
    "é",
    "🦀",
    "\0",
    "\n",
    ":not(",
    ":is(",
    ":where(",
    ":has(",
    ":nth-child(",
    ":nth-last-of-type(",
    ":contains(",
    "odd",
    "even",
    "2n+1",
    "-n+3",
    ":first-child",
    ":last-child",
    ":only-child",
    ":empty",
    ":root",
    ":scope",
    ":first-of-type",
    ":checked",
    "::",
    ":hover",
    "[class~=",
    "[id^=",
    " i]",
    "\\31 ",
    "\\:",
];

const HTML_ATOMS: &[&str] = &[
    "<div>",
    "<div class=x>",
    "</div>",
    "<p class='x y'>",
    "</p>",
    "<ul>",
    "</ul>",
    "<li id=a>",
    "<li>",
    "</li>",
    "<span id=x>",
    "</span>",
    "<a href='/x' class=X>",
    "</a>",
    "<input checked>",
    "<br>",
    "<!-- c -->",
    "text",
    " ",
    "<option selected disabled>",
    "x(1)",
    "<",
    ">",
    "\"",
];

/// Element soup over the tags/classes the reference selectors use.
const TREE_ATOMS: &[&str] = &[
    "<div>",
    "<div class=a>",
    "<p class=b>",
    "<span>",
    "<span class='a b'>",
    "<i>",
    "<i class=b>",
    "</div>",
    "</p>",
    "</span>",
    "</i>",
    "t",
    "<!--c-->",
    "<i class='a'/>",
    "<span class='b'/>",
    "<p/>",
    "<div class='a b'/>",
    "<i/>",
];

/// Simple selectors covering the whole grammar, for well-formed selectors.
const SIMPLE_ATOMS: &[&str] = &[
    ".x",
    ".y",
    "#a",
    "#x",
    "[id]",
    "[class~=x]",
    "[href^='/']",
    "[class*=X i]",
    "[id|=a]",
    ":first-child",
    ":last-child",
    ":only-child",
    ":nth-child(2n+1)",
    ":nth-child(-n+2 of .x)",
    ":nth-last-child(odd)",
    ":first-of-type",
    ":nth-of-type(2)",
    ":only-of-type",
    ":empty",
    ":root",
    ":scope",
    ":checked",
    ":disabled",
    ":enabled",
    ":contains(text)",
    ":not(.x)",
    ":is(li, p)",
    ":has(> span)",
    ":has(+ li)",
    ":has(~ *)",
    ":where(ul *)",
    ":not(:has(a))",
];

/// Childless siblings, for long flat sibling lists.
const SIBLING_ATOMS: &[&str] = &[
    "<i class='a'/>",
    "<i class='b'/>",
    "<i/>",
    "<p class='a'/>",
    "t",
];

fn soup(atoms: &'static [&'static str], max: usize) -> impl Strategy<Value = String> {
    proptest::collection::vec(select(atoms), 0..max).prop_map(|v| v.concat())
}

/// A wide list of siblings (where `+`/`~` get interesting).
fn siblings() -> impl Strategy<Value = String> {
    soup(SIBLING_ATOMS, 24).prop_map(|s| format!("<div class=a><span>{s}</span></div>"))
}

/// A well-formed selector list built from [`SIMPLE_ATOMS`] and combinators.
fn well_formed() -> impl Strategy<Value = String> {
    let tag = select(&["", "*", "div", "p", "li", "span", "a", "UL"][..]);
    let compound =
        (tag, proptest::collection::vec(select(SIMPLE_ATOMS), 0..3)).prop_map(|(tag, simples)| {
            match (tag, simples.concat()) {
                ("", s) if s.is_empty() => "*".to_string(),
                (tag, s) => format!("{tag}{s}"),
            }
        });
    let comb = select(&[" ", " > ", " + ", " ~ ", ">", "~", "\n"][..]);
    let complex = proptest::collection::vec((comb, compound), 1..4).prop_map(|parts| {
        let mut s = String::new();
        for (i, (comb, compound)) in parts.into_iter().enumerate() {
            if i > 0 {
                s.push_str(comb);
            }
            s.push_str(&compound);
        }
        s
    });
    proptest::collection::vec(complex, 1..3).prop_map(|v| v.join(", "))
}

fn node_ids<'a>(nodes: impl Iterator<Item = NodeRef<'a>>) -> Vec<NodeId> {
    nodes.map(|n| n.id()).collect()
}

// --- a naive reference matcher ---------------------------------------------

/// One compound of a generated selector; `comb` links it to the part on its
/// left (ignored for the first part).
#[derive(Clone, Debug)]
struct Part {
    tag: Option<&'static str>,
    class: Option<&'static str>,
    nth: Option<(i32, i32)>,
    comb: char,
}

fn part(tags: &'static [&'static str], combs: &'static [char]) -> impl Strategy<Value = Part> {
    (
        proptest::option::of(select(tags)),
        proptest::option::of(select(&["a", "b"][..])),
        proptest::option::weighted(0.2, (-3i32..=3, -3i32..=4)),
        select(combs),
    )
        .prop_map(|(tag, class, nth, comb)| Part {
            tag,
            class,
            nth,
            comb,
        })
}

fn render(parts: &[Part]) -> String {
    let mut s = String::new();
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            s.push(' ');
            if p.comb != ' ' {
                s.push(p.comb);
                s.push(' ');
            }
        }
        s.push_str(p.tag.unwrap_or("*"));
        if let Some(class) = p.class {
            s.push('.');
            s.push_str(class);
        }
        if let Some((a, b)) = p.nth {
            s.push_str(&format!(":nth-child({a}n{b:+})"));
        }
    }
    s
}

fn prev_elements<'a>(el: NodeRef<'a>) -> impl Iterator<Item = NodeRef<'a>> {
    std::iter::successors(el.prev_sibling(), |n| n.prev_sibling()).filter(|n| n.is_element())
}

fn compound_matches(p: &Part, el: NodeRef<'_>) -> bool {
    el.is_element()
        && p.tag.is_none_or(|t| el.tag_name() == Some(t))
        && p.class.is_none_or(|c| el.has_class(c))
        && p.nth.is_none_or(|(a, b)| {
            let pos = prev_elements(el).count() as i32 + 1;
            (0..=100).any(|n| a * n + b == pos)
        })
}

/// Textbook right-to-left matching with full backtracking.
fn naive(parts: &[Part], el: NodeRef<'_>) -> bool {
    let Some((last, rest)) = parts.split_last() else {
        return true;
    };
    if !compound_matches(last, el) {
        return false;
    }
    if rest.is_empty() {
        return true;
    }
    match last.comb {
        '>' => el.parent().is_some_and(|p| naive(rest, p)),
        '+' => prev_elements(el).next().is_some_and(|p| naive(rest, p)),
        '~' => prev_elements(el).any(|p| naive(rest, p)),
        _ => el.ancestors().any(|a| naive(rest, a)),
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 2048, ..ProptestConfig::default() })]

    #[test]
    fn never_panics(
        s in prop_oneof![soup(SELECTOR_ATOMS, 24), ".{0,40}", well_formed()],
        html in soup(HTML_ATOMS, 40),
    ) {
        let dom = parse(&html);
        if let Err(e) = Selector::parse(&s) {
            prop_assert!(e.position() <= s.len());
            prop_assert!(s.is_char_boundary(e.position()));
            prop_assert!(!e.message().is_empty());
        }
        let _ = dom.select(s.as_str()).count();
        let _ = dom.select_first(s.as_str());
        for node in dom.nodes() {
            let _ = node.matches(s.as_str());
            let _ = node.closest(s.as_str());
            let _ = node.select(s.as_str()).count();
        }
    }

    #[test]
    fn well_formed_selectors_parse(s in well_formed()) {
        let parsed = Selector::parse(&s);
        prop_assert!(parsed.is_ok(), "{:?}: {:?}", s, parsed);
    }

    #[test]
    fn str_and_parsed_selectors_agree(
        s in prop_oneof![soup(SELECTOR_ATOMS, 24), well_formed()],
        html in soup(HTML_ATOMS, 40),
    ) {
        let dom = parse(&html);
        let by_str = node_ids(dom.select(s.as_str()));
        match Selector::parse(&s) {
            Ok(sel) => {
                prop_assert_eq!(&by_str, &node_ids(dom.select(&sel)));
                prop_assert_eq!(&by_str, &node_ids(dom.nodes().filter(|n| sel.matches(*n))));

                // `:is(S)` is `S`, and `:not(S)` is its complement.
                let is = Selector::parse(&format!(":is({s})"));
                let not = Selector::parse(&format!(":not({s})"));
                if let (Ok(is), Ok(not)) = (is, not) {
                    prop_assert_eq!(&by_str, &node_ids(dom.select(&is)));
                    let negated = node_ids(dom.select(&not));
                    let elements = dom.nodes().filter(|n| n.is_element()).count();
                    prop_assert_eq!(by_str.len() + negated.len(), elements);
                    prop_assert!(negated.iter().all(|id| !by_str.contains(id)));
                }
            }
            Err(_) => prop_assert!(by_str.is_empty()),
        }
    }

    #[test]
    fn agrees_with_a_naive_backtracking_matcher(
        parts in proptest::collection::vec(part(&["div", "p", "span", "i"], &[' ', '>', '+', '~']), 1..6),
        html in prop_oneof![soup(TREE_ATOMS, 48), siblings()],
    ) {
        agrees_with_naive(&parts, &html)?;
    }

    #[test]
    fn agrees_with_a_naive_backtracking_matcher_on_siblings(
        parts in proptest::collection::vec(part(&["i", "p"], &['+', '~']), 1..6),
        html in siblings(),
    ) {
        agrees_with_naive(&parts, &html)?;
    }
}

fn agrees_with_naive(parts: &[Part], html: &str) -> Result<(), TestCaseError> {
    let dom = parse(html);
    let s = render(parts);
    let sel = Selector::parse(&s);
    prop_assert!(sel.is_ok(), "{} failed to parse: {:?}", s, sel);
    let sel = sel.unwrap();
    for node in dom.nodes() {
        prop_assert_eq!(sel.matches(node), naive(parts, node), "{} on {:?}", s, node);
    }
    Ok(())
}
