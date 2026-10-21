//! `-e/--extract NAME=SPEC`: named fields out of a page, as a JSON object.
//!
//! `SPEC` is `SELECTOR` (the readable text of the first match),
//! `SELECTOR@ATTR` (the first match's `ATTR`), and either one with a trailing
//! `[]` (an array over all matches). Only the **last** `@NAME` and a trailing
//! `[]` are special, so `a[href^="mailto:me@x.org"]@href` works; write `\@`
//! for a literal `@` before an attribute-name-like suffix (it is also the CSS
//! escape, so the selector engine reads it as `@`). An empty `SELECTOR` is the
//! scope element itself (the whole document without a scope).

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

/// The JSON object for `fields`, evaluated inside `scope` (the whole document
/// when `None`). With `skip_blank`, empty or whitespace-only values are
/// skipped (`-w`).
pub fn object(
    dom: &Dom,
    scope: Option<NodeRef<'_>>,
    fields: &[Compiled<'_>],
    skip_blank: bool,
) -> String {
    let mut obj = json::Object::default();
    for f in fields {
        let mut values = values(dom, scope, f).filter(|v| !(skip_blank && v.trim().is_empty()));
        let value = if f.field.all {
            json::array(values.map(|v| json::string(&v)))
        } else {
            json::opt_string(values.next().as_deref())
        };
        obj.field(&f.field.name, &value);
    }
    obj.finish()
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
}
