//! `-e/--extract NAME=SPEC`: named fields out of a page, as a JSON object.
//!
//! `SPEC` is `SELECTOR` (the readable text of the first match),
//! `SELECTOR@ATTR` (the first match's `ATTR`), and either one with a trailing
//! `[]` (an array over all matches). Only the **last** `@NAME` and a trailing
//! `[]` are special, so `a[href^="mailto:me@x.org"]@href` works; write `\@`
//! for a literal `@` before an attribute-name-like suffix (it is also the CSS
//! escape, so the selector engine reads it as `@`). An empty `SELECTOR` is the
//! scope element itself (the whole document without a scope).

use std::ops::Range;

use domtree::{Dom, NodeRef, Selector};

use crate::json;

/// One parsed `-e NAME=SPEC`.
#[derive(Debug, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    /// `""` means the scope element itself.
    pub selector: String,
    /// Read this attribute instead of the text.
    pub attr: Option<String>,
    /// `[]`: every match, as an array.
    pub all: bool,
}

impl Field {
    /// Parse `NAME=SPEC`.
    pub fn parse(arg: &str) -> Result<Field, String> {
        let Some((name, spec)) = arg.split_once('=') else {
            return Err(format!(
                "--extract '{arg}': expected NAME=SELECTOR (e.g. title=h1)"
            ));
        };
        if name.is_empty() {
            return Err(format!("--extract '{arg}': the NAME before '=' is empty"));
        }
        let spec = spec.trim();
        let (spec, all) = match spec.strip_suffix("[]") {
            Some(rest) => (rest.trim_end(), true),
            None => (spec, false),
        };
        let (selector, attr) = match spec.rsplit_once('@') {
            Some((selector, attr)) if is_attr_name(attr) && !is_escaped(selector) => {
                (selector.trim(), Some(attr.to_ascii_lowercase()))
            }
            _ => (spec, None),
        };
        Ok(Field {
            name: name.to_string(),
            selector: selector.to_string(),
            attr,
            all,
        })
    }
}

/// A [`Field`] with its selector compiled (`None` = the scope itself).
pub struct Compiled<'f> {
    field: &'f Field,
    selector: Option<Selector>,
}

/// Compile every field's selector, naming the field in errors.
pub fn compile(fields: &[Field]) -> Result<Vec<Compiled<'_>>, String> {
    fields
        .iter()
        .map(|field| {
            let selector = if field.selector.is_empty() {
                None
            } else {
                Some(
                    crate::compile(&field.selector)
                        .map_err(|e| format!("--extract {}: {e}", field.name))?,
                )
            };
            Ok(Compiled { field, selector })
        })
        .collect()
}

/// The JSON object for `fields`, evaluated over the whole document. With
/// `skip_blank`, empty or whitespace-only values are skipped (`-w`).
pub fn object(dom: &Dom, fields: &[Compiled<'_>], skip_blank: bool) -> String {
    let mut obj = json::Object::default();
    for f in fields {
        obj.field(&f.field.name, &field_json(dom, None, f, skip_blank));
    }
    obj.finish()
}

/// One JSON object per scope element, with `fields` evaluated inside it
/// (see [`object`]).
///
/// Scopes may nest (`div` on nested `<div>`s), so a field's selector does not
/// search each scope's subtree — that is O(nodes × depth). It runs once over
/// the whole document instead, and the matches inside a scope, a contiguous
/// run in document order, are found by binary search on pre-order positions;
/// each match's value is computed at most once. Only a selector that uses
/// `:scope` (whose matches depend on the scope) is run in each scope.
pub fn scoped_objects<'d>(
    dom: &'d Dom,
    scopes: &[NodeRef<'d>],
    fields: &[Compiled<'_>],
    skip_blank: bool,
) -> Vec<String> {
    if scopes.is_empty() {
        return Vec::new();
    }
    let layout = Layout::new(dom);
    let mut indexes: Vec<Option<Index<'d>>> = fields
        .iter()
        .map(|f| match &f.selector {
            Some(sel) if !mentions_scope(&f.field.selector) => Some(Index::new(dom, sel, &layout)),
            _ => None,
        })
        .collect();
    scopes
        .iter()
        .map(|&scope| {
            let mut obj = json::Object::default();
            for (f, index) in fields.iter().zip(indexes.iter_mut()) {
                let value = match index {
                    Some(index) => index.json(layout.inside(scope), f, skip_blank),
                    None => field_json(dom, Some(scope), f, skip_blank),
                };
                obj.field(&f.field.name, &value);
            }
            obj.finish()
        })
        .collect()
}

/// One field's JSON value inside `scope` (the whole document when `None`).
fn field_json(dom: &Dom, scope: Option<NodeRef<'_>>, f: &Compiled<'_>, skip_blank: bool) -> String {
    let mut values = values(dom, scope, f).filter(|v| !(skip_blank && v.trim().is_empty()));
    if f.field.all {
        json::array(values.map(|v| json::string(&v)))
    } else {
        json::opt_string(values.next().as_deref())
    }
}

/// Every node's position in document (pre-order) order and the size of its
/// subtree, so "is inside" becomes a range test.
struct Layout {
    /// Indexed by `NodeId::index()`; `usize::MAX` for detached nodes.
    pos: Vec<usize>,
    /// Indexed by `NodeId::index()`: the node and its descendants.
    size: Vec<usize>,
}

impl Layout {
    fn new(dom: &Dom) -> Layout {
        let order: Vec<NodeRef<'_>> = dom.nodes().collect();
        let mut pos = vec![usize::MAX; dom.len()];
        for (i, node) in order.iter().enumerate() {
            if let Some(slot) = pos.get_mut(node.id().index()) {
                *slot = i;
            }
        }
        // Descendants come after their ancestors, so in reverse every subtree
        // is complete before it is added to its parent.
        let mut size = vec![1; dom.len()];
        for node in order.iter().rev() {
            let own = size.get(node.id().index()).copied().unwrap_or(1);
            if let Some(slot) = node.parent().and_then(|p| size.get_mut(p.id().index())) {
                *slot += own;
            }
        }
        Layout { pos, size }
    }

    fn position(&self, node: NodeRef<'_>) -> usize {
        self.pos
            .get(node.id().index())
            .copied()
            .unwrap_or(usize::MAX)
    }

    /// The positions of `node`'s descendants (not `node` itself).
    fn inside(&self, node: NodeRef<'_>) -> Range<usize> {
        let at = self.position(node);
        let size = self.size.get(node.id().index()).copied().unwrap_or(1);
        match at.checked_add(size) {
            Some(end) => at + 1..end,
            None => 0..0,
        }
    }
}

/// A field selector's matches in the whole document, for looking up the
/// ones inside each scope.
struct Index<'d> {
    /// Each match's pre-order position, ascending, and the match.
    hits: Vec<(usize, NodeRef<'d>)>,
    /// Each match's value as a JSON string once computed: `Some(None)` when it
    /// has none (no such attribute, or blank with `-w`).
    values: Vec<Option<Option<String>>>,
    /// `skip[i] > i`: no match in `i..skip[i]` has a value. Path-compressed,
    /// so runs of value-less matches shared by nested scopes are crossed in
    /// amortized near-constant time instead of once per scope.
    skip: Vec<usize>,
}

impl<'d> Index<'d> {
    fn new(dom: &'d Dom, sel: &Selector, layout: &Layout) -> Index<'d> {
        let mut hits: Vec<(usize, NodeRef<'d>)> =
            dom.select(sel).map(|n| (layout.position(n), n)).collect();
        // Already in document order; this keeps the binary search sound.
        hits.sort_by_key(|&(at, _)| at);
        Index {
            values: vec![None; hits.len()],
            skip: (0..hits.len()).collect(),
            hits,
        }
    }

    /// The field's JSON value for the matches at positions `inside`.
    fn json(&mut self, inside: Range<usize>, f: &Compiled<'_>, skip_blank: bool) -> String {
        let lo = self.hits.partition_point(|&(at, _)| at < inside.start);
        let hi = self.hits.partition_point(|&(at, _)| at < inside.end);
        let attr = f.field.attr.as_deref();
        if !f.field.all {
            return match self.next_value(lo, hi, attr, skip_blank) {
                Some(i) => self.value(i).unwrap_or("null").to_string(),
                None => "null".to_string(),
            };
        }
        let mut out = String::from("[");
        let mut from = lo;
        while let Some(i) = self.next_value(from, hi, attr, skip_blank) {
            if out.len() > 1 {
                out.push(',');
            }
            out.push_str(self.value(i).unwrap_or("null"));
            from = i + 1;
        }
        out.push(']');
        out
    }

    /// The first match in `from..to` that has a value.
    fn next_value(
        &mut self,
        from: usize,
        to: usize,
        attr: Option<&str>,
        skip_blank: bool,
    ) -> Option<usize> {
        let mut i = from;
        let mut crossed = Vec::new();
        while i < to {
            let next = self.skip.get(i).copied().unwrap_or(to);
            if next > i {
                crossed.push(i);
                i = next;
            } else if self.compute(i, attr, skip_blank) {
                break;
            } else {
                crossed.push(i);
                i += 1;
            }
        }
        // Nothing in `j..i` has a value, for every `j` crossed.
        for j in crossed {
            if let Some(slot) = self.skip.get_mut(j) {
                *slot = i;
            }
        }
        (i < to).then_some(i)
    }

    /// Compute (once) whether match `i` has a value.
    fn compute(&mut self, i: usize, attr: Option<&str>, skip_blank: bool) -> bool {
        if let Some(Some(known)) = self.values.get(i) {
            return known.is_some();
        }
        let Some(&(_, node)) = self.hits.get(i) else {
            return false;
        };
        let keep = |v: &str| !(skip_blank && v.trim().is_empty());
        let value = match attr {
            Some(attr) => node.attr(attr).filter(|v| keep(v)).map(json::string),
            None => Some(node.to_text())
                .filter(|v| keep(v))
                .map(|v| json::string(&v)),
        };
        let has = value.is_some();
        if let Some(slot) = self.values.get_mut(i) {
            *slot = Some(value);
        }
        has
    }

    /// Match `i`'s value, if computed and present.
    fn value(&self, i: usize) -> Option<&str> {
        self.values.get(i)?.as_ref()?.as_deref()
    }
}

/// Whether `selector` may use `:scope`, whose matches depend on the scope
/// element: it mentions `scope` in any letter case once CSS escapes are
/// decoded (so `:SCOPE` and `:\73 cope` count). Conservative — `.scope-x`
/// also counts, and is then merely evaluated per scope.
fn mentions_scope(selector: &str) -> bool {
    let mut decoded = String::with_capacity(selector.len());
    let mut chars = selector.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            decoded.push(c.to_ascii_lowercase());
            continue;
        }
        let mut hex = String::new();
        while let Some(h) = chars.next_if(|h| h.is_ascii_hexdigit() && hex.len() < 6) {
            hex.push(h);
        }
        if hex.is_empty() {
            decoded.extend(chars.next().map(|c| c.to_ascii_lowercase()));
        } else {
            let c = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32);
            decoded.extend(c.map(|c| c.to_ascii_lowercase()));
            chars.next_if(|c| c.is_ascii_whitespace());
        }
    }
    decoded.contains("scope")
}

/// The values of one field, lazily (so a single value stops at the first).
fn values<'d>(
    dom: &'d Dom,
    scope: Option<NodeRef<'d>>,
    f: &'d Compiled<'_>,
) -> Box<dyn Iterator<Item = String> + 'd> {
    let nodes: Box<dyn Iterator<Item = NodeRef<'d>> + 'd> = match (&f.selector, scope) {
        (Some(sel), Some(scope)) => Box::new(scope.select(sel)),
        (Some(sel), None) => Box::new(dom.select(sel)),
        (None, Some(scope)) => Box::new(std::iter::once(scope)),
        (None, None) => {
            // The document itself: it has text, but no attributes.
            return match f.field.attr {
                Some(_) => Box::new(std::iter::empty()),
                None => Box::new(std::iter::once(dom.to_text())),
            };
        }
    };
    match &f.field.attr {
        Some(attr) => Box::new(nodes.filter_map(move |n| n.attr(attr).map(str::to_string))),
        None => Box::new(nodes.map(|n| n.to_text())),
    }
}

/// An HTML attribute name as people write them in a spec (`href`,
/// `data-id`, `xlink:href`).
fn is_attr_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.'))
}

/// Whether the character after `before` is escaped by a backslash (an odd
/// number of trailing backslashes).
fn is_escaped(before: &str) -> bool {
    before.bytes().rev().take_while(|&b| b == b'\\').count() % 2 == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(selector: &str, attr: Option<&str>, all: bool) -> Field {
        Field {
            name: "x".into(),
            selector: selector.into(),
            attr: attr.map(Into::into),
            all,
        }
    }

    #[test]
    fn specs() {
        assert_eq!(Field::parse("x=h1"), Ok(field("h1", None, false)));
        assert_eq!(
            Field::parse("x=a@href"),
            Ok(field("a", Some("href"), false))
        );
        assert_eq!(Field::parse("x=.tag[]"), Ok(field(".tag", None, true)));
        assert_eq!(
            Field::parse("x= img @SRC [] "),
            Ok(field("img", Some("src"), true))
        );
        assert_eq!(
            Field::parse("x=@data-id"),
            Ok(field("", Some("data-id"), false))
        );
        assert_eq!(Field::parse("x="), Ok(field("", None, false)));
        // Only the last `@NAME` is special.
        assert_eq!(
            Field::parse(r#"x=a[href^="mailto:me@x.org"]@href"#),
            Ok(field(r#"a[href^="mailto:me@x.org"]"#, Some("href"), false))
        );
        assert_eq!(
            Field::parse(r#"x=a[title="a@b"]"#),
            Ok(field(r#"a[title="a@b"]"#, None, false))
        );
        // `\@` is a literal (CSS-escaped) `@`.
        assert_eq!(
            Field::parse(r"x=.w-1\@sm"),
            Ok(field(r".w-1\@sm", None, false))
        );
        assert_eq!(
            Field::parse(r"x=.w-1\\@title"),
            Ok(field(r".w-1\\", Some("title"), false))
        );
        // The name may hold anything but `=`.
        assert_eq!(Field::parse("a b=p").map(|f| f.name), Ok("a b".into()));
    }

    #[test]
    fn bad_specs() {
        assert!(Field::parse("title").is_err());
        assert!(Field::parse("=h1").is_err());
    }

    #[test]
    fn scope_mentions() {
        for yes in [
            ":scope > a",
            ":SCOPE a",
            r":\73 cope a",
            r":sc\ope",
            r":\53\43OPE",
            ".scope",
        ] {
            assert!(mentions_scope(yes), "{yes}");
        }
        for no in ["a", "div > span@x", r".w-1\@sm", r":\73 ", "sco pe", r"\"] {
            assert!(!mentions_scope(no), "{no}");
        }
    }

    /// The per-scope search that [`scoped_objects`] replaces.
    fn per_scope(
        dom: &Dom,
        scopes: &[NodeRef<'_>],
        fields: &[Compiled<'_>],
        ws: bool,
    ) -> Vec<String> {
        let object = |scope| {
            let mut obj = json::Object::default();
            for f in fields {
                obj.field(&f.field.name, &field_json(dom, Some(scope), f, ws));
            }
            obj.finish()
        };
        scopes.iter().map(|&s| object(s)).collect()
    }

    #[test]
    fn indexed_fields_match_the_per_scope_search() {
        const TOKENS: &[&str] = &[
            "<div>",
            "</div>",
            "<div class=a>",
            "<span>",
            "</span>",
            "<span id=s> </span>",
            "<span id=''>t</span>",
            "<p>",
            "</p>",
            "<b>",
            "</b>",
            "x",
            " ",
            "<li>",
            "<ul>",
            "</ul>",
            "<a href=h>",
            "</a>",
            "<img alt=i>",
            "<br>",
        ];
        let mut docs: Vec<String> = vec![
            "<div><span>a</span><div><span id=x> </span><div><span>c</span></div></div></div>"
                .into(),
            format!("{}<span>deep</span>", "<div>".repeat(50)),
            "<div><span> </span>".repeat(30),
        ];
        // A fixed pseudo-random mix of nested and flat markup.
        let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
        for _ in 0..150 {
            let mut doc = String::new();
            for _ in 0..40 {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                doc.push_str(
                    TOKENS
                        .get(seed as usize % TOKENS.len())
                        .copied()
                        .unwrap_or(""),
                );
            }
            docs.push(doc);
        }
        let specs = [
            "x=span",
            "x=span[]",
            "x=span@id",
            "x=span@id[]",
            "x=div span",
            "x=div > span[]",
            "x=:not(b)[]",
            "x=*",
            "x=b, span[]",
            "x=div:has(span)@class",
            "x=",
            "x=@class",
            "x=:scope > span[]",
            "x=:scope span",
        ];
        for doc in &docs {
            let mut dom = domtree::parse(doc);
            // Detached nodes have no position: remove some.
            let removed: Vec<_> = dom.select("b").map(|n| n.id()).collect();
            for id in removed {
                dom.remove(id);
            }
            for scope in ["div", "*", "span, p", "ul"] {
                let scopes: Vec<NodeRef<'_>> = dom.select(scope).collect();
                for spec in specs {
                    let fields = [Field::parse(spec).unwrap()];
                    let compiled = compile(&fields).unwrap();
                    for ws in [false, true] {
                        assert_eq!(
                            scoped_objects(&dom, &scopes, &compiled, ws),
                            per_scope(&dom, &scopes, &compiled, ws),
                            "{doc:?} {scope} {spec} ws={ws}"
                        );
                    }
                }
            }
        }
    }
}
