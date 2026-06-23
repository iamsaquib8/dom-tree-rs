//! A small, hand-rolled CSS-selector engine (behind the `query` feature).
//!
//! Supports the subset that covers the vast majority of real queries with zero
//! dependencies (no `selectors`/`cssparser`):
//!
//! - type selectors `div`, the universal `*`
//! - `#id`, `.class` (compound: `div.a.b#c`)
//! - attribute presence `[attr]` and equality `[attr=value]` / `[attr="value"]`
//! - descendant (` `) and child (`>`) combinators
//! - selector lists separated by `,`
//!
//! It does **not** implement pseudo-classes, `~`/`+` combinators, or the more
//! exotic attribute operators — by design, to stay tiny.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::iter::Peekable;
use core::str::Chars;

use crate::tree::{Dom, NodeRef};

impl Dom {
    /// Find every element matching a CSS selector, in document order.
    ///
    /// ```
    /// let dom = domtree::parse(
    ///     "<ul><li class='x'>a</li><li>b</li></ul><ol><li>c</li></ol>",
    /// );
    /// // descendant + class
    /// assert_eq!(dom.select("ul li.x").count(), 1);
    /// // child combinator
    /// assert_eq!(dom.select("ol > li").count(), 1);
    /// // selector list
    /// assert_eq!(dom.select("ul, ol").count(), 2);
    /// ```
    pub fn select<'a>(&'a self, selector: &str) -> impl Iterator<Item = NodeRef<'a>> + 'a {
        let groups = parse_selector(selector);
        self.nodes()
            .filter(move |n| n.is_element() && groups.iter().any(|g| matches_complex(*n, g)))
    }
}

#[derive(Clone, Copy)]
enum Comb {
    Descendant,
    Child,
}

struct Compound {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    attrs: Vec<AttrPred>,
}

struct AttrPred {
    name: String,
    value: Option<String>,
}

struct Complex {
    /// Each step's `Comb` is the combinator to its *left*; `steps[0]`'s is unused.
    steps: Vec<(Comb, Compound)>,
}

// --- parsing -------------------------------------------------------------

fn parse_selector(input: &str) -> Vec<Complex> {
    split_groups(input)
        .iter()
        .filter_map(|g| parse_complex(g))
        .collect()
}

/// Split a selector list on top-level commas (ignoring commas inside `[...]`).
fn split_groups(input: &str) -> Vec<String> {
    let mut groups = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    for c in input.chars() {
        match c {
            '[' => {
                depth += 1;
                cur.push(c);
            }
            ']' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth <= 0 => {
                groups.push(cur.clone());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    groups.push(cur);
    groups
}

fn parse_complex(group: &str) -> Option<Complex> {
    let mut steps: Vec<(Comb, Compound)> = Vec::new();
    let mut chars = group.chars().peekable();
    let mut comb = Comb::Descendant;

    loop {
        skip_ws(&mut chars);
        if matches!(chars.peek(), Some('>')) {
            chars.next();
            comb = Comb::Child;
            skip_ws(&mut chars);
        }

        let mut buf = String::new();
        let mut in_bracket = false;
        while let Some(&c) = chars.peek() {
            if in_bracket {
                buf.push(c);
                chars.next();
                if c == ']' {
                    in_bracket = false;
                }
            } else if c == '[' {
                in_bracket = true;
                buf.push(c);
                chars.next();
            } else if c.is_whitespace() || c == '>' {
                break;
            } else {
                buf.push(c);
                chars.next();
            }
        }

        if buf.is_empty() {
            break;
        }
        let compound = parse_compound(&buf)?;
        steps.push((comb, compound));
        comb = Comb::Descendant;
        if chars.peek().is_none() {
            break;
        }
    }

    if steps.is_empty() {
        None
    } else {
        Some(Complex { steps })
    }
}

fn parse_compound(s: &str) -> Option<Compound> {
    let mut tag = None;
    let mut id = None;
    let mut classes = Vec::new();
    let mut attrs = Vec::new();
    let mut chars = s.chars().peekable();

    // Optional leading type selector.
    match chars.peek() {
        Some('*') => {
            chars.next();
        }
        Some(&c) if is_ident_start(c) => {
            let mut t = String::new();
            while let Some(&c) = chars.peek() {
                if is_ident_char(c) {
                    t.push(c.to_ascii_lowercase());
                    chars.next();
                } else {
                    break;
                }
            }
            tag = Some(t);
        }
        _ => {}
    }

    while let Some(&c) = chars.peek() {
        match c {
            '#' => {
                chars.next();
                id = Some(read_ident(&mut chars));
            }
            '.' => {
                chars.next();
                classes.push(read_ident(&mut chars));
            }
            '[' => {
                chars.next();
                attrs.push(read_attr(&mut chars));
            }
            _ => {
                chars.next();
            }
        }
    }

    Some(Compound {
        tag,
        id,
        classes,
        attrs,
    })
}

fn read_ident(chars: &mut Peekable<Chars<'_>>) -> String {
    let mut s = String::new();
    while let Some(&c) = chars.peek() {
        if is_ident_char(c) {
            s.push(c);
            chars.next();
        } else {
            break;
        }
    }
    s
}

fn read_attr(chars: &mut Peekable<Chars<'_>>) -> AttrPred {
    let mut inner = String::new();
    for c in chars.by_ref() {
        if c == ']' {
            break;
        }
        inner.push(c);
    }
    let inner = inner.trim();
    match inner.find('=') {
        Some(eq) => {
            let name = inner.get(..eq).unwrap_or("").trim().to_ascii_lowercase();
            let mut val = inner.get(eq + 1..).unwrap_or("").trim();
            val = strip_quotes(val);
            AttrPred {
                name,
                value: Some(val.to_string()),
            }
        }
        None => AttrPred {
            name: inner.to_ascii_lowercase(),
            value: None,
        },
    }
}

fn strip_quotes(v: &str) -> &str {
    let bytes = v.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes.first().copied();
        let last = bytes.last().copied();
        if (first == Some(b'"') && last == Some(b'"'))
            || (first == Some(b'\'') && last == Some(b'\''))
        {
            return v.get(1..v.len() - 1).unwrap_or(v);
        }
    }
    v
}

fn skip_ws(chars: &mut Peekable<Chars<'_>>) {
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else {
            break;
        }
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '-'
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

// --- matching ------------------------------------------------------------

fn matches_complex(node: NodeRef<'_>, complex: &Complex) -> bool {
    let n = complex.steps.len();
    if n == 0 {
        return false;
    }
    matches_from(node, &complex.steps, n - 1)
}

fn matches_from(node: NodeRef<'_>, steps: &[(Comb, Compound)], i: usize) -> bool {
    let (comb, compound) = match steps.get(i) {
        Some(s) => s,
        None => return false,
    };
    if !simple_matches(node, compound) {
        return false;
    }
    if i == 0 {
        return true;
    }
    match comb {
        Comb::Child => node.parent().is_some_and(|p| matches_from(p, steps, i - 1)),
        Comb::Descendant => {
            let mut anc = node.parent();
            while let Some(a) = anc {
                if matches_from(a, steps, i - 1) {
                    return true;
                }
                anc = a.parent();
            }
            false
        }
    }
}

fn simple_matches(node: NodeRef<'_>, c: &Compound) -> bool {
    if let Some(tag) = &c.tag {
        match node.tag_name() {
            Some(t) if t.eq_ignore_ascii_case(tag) => {}
            _ => return false,
        }
    }
    if let Some(id) = &c.id {
        if node.attr("id") != Some(id.as_str()) {
            return false;
        }
    }
    for class in &c.classes {
        if !node.has_class(class) {
            return false;
        }
    }
    for attr in &c.attrs {
        match node.attr(&attr.name) {
            None => return false,
            Some(v) => {
                if let Some(want) = &attr.value {
                    if v != want {
                        return false;
                    }
                }
            }
        }
    }
    true
}
