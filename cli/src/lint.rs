//! `domtree lint`: fast HTML checks with `file:line:col` positions.
//!
//! One parse and a few linear passes per file: element checks walk the tree
//! once in document order (accessible names are computed bottom-up, so nested
//! links cannot make it quadratic), and positions come from a single forward
//! scan over the source for all findings at once.
//!
//! Recovered parse errors are all reported, but classified with the source
//! ranges of the elements they closed: omitting an *optional* end tag
//! (`<li>a<li>b`, `<p>a</div>`) is valid HTML and only a note, while a
//! required one (`<div>` never closed, `<b><i>x</b>`) is a warning.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::ops::Range;

use domtree::{Dom, ErrorKind, NodeRef, ParseError};

use crate::args::LintOptions;
use crate::json;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Error,
    Warning,
    /// Reported, but does not fail the run.
    Note,
}

impl Level {
    fn as_str(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warning => "warning",
            Level::Note => "note",
        }
    }
}

/// A finding with its position resolved.
#[derive(Debug, PartialEq, Eq)]
pub struct Finding {
    pub line: usize,
    pub column: usize,
    pub level: Level,
    pub rule: &'static str,
    pub message: String,
}

/// A finding at a byte offset, before positions are resolved.
struct Raw {
    at: usize,
    level: Level,
    rule: &'static str,
    message: String,
    /// A related offset and its label, appended to the message as
    /// "(LABEL LINE:COL)".
    related: Option<(usize, String)>,
}

impl Raw {
    fn new(at: usize, level: Level, rule: &'static str, message: String) -> Raw {
        Raw {
            at,
            level,
            rule,
            message,
            related: None,
        }
    }

    fn related(mut self, at: usize, label: impl Into<String>) -> Raw {
        self.related = Some((at, label.into()));
        self
    }
}

/// Lint every input; returns the report and the exit code.
pub fn run(opts: &LintOptions) -> (String, u8) {
    let stdin = ["-".to_string()];
    let files = if opts.files.is_empty() {
        &stdin[..]
    } else {
        &opts.files[..]
    };
    let (mut text, mut objects) = (String::new(), Vec::new());
    let (mut failed, mut unreadable) = (false, false);
    for file in files {
        let name = if file == "-" {
            "<stdin>"
        } else {
            file.as_str()
        };
        let html = match crate::read_input(Some(file), opts.encoding.as_deref()) {
            Ok(html) => html,
            Err(e) => {
                eprintln!("domtree: {e}");
                unreadable = true;
                continue;
            }
        };
        for f in check(&html) {
            if opts.quiet && f.level == Level::Note {
                continue;
            }
            failed |= f.level != Level::Note;
            if opts.json {
                let mut obj = json::Object::default();
                obj.field("file", &json::string(name))
                    .field("line", &f.line.to_string())
                    .field("column", &f.column.to_string())
                    .field("level", &json::string(f.level.as_str()))
                    .field("rule", &json::string(f.rule))
                    .field("message", &json::string(&f.message));
                objects.push(obj.finish());
            } else {
                text.push_str(&format!(
                    "{name}:{}:{}: {}: {} [{}]\n",
                    f.line,
                    f.column,
                    f.level.as_str(),
                    f.message,
                    f.rule
                ));
            }
        }
    }
    if opts.json {
        text = format!("{}\n", json::array(objects));
    }
    let code = if unreadable {
        2
    } else if failed {
        1
    } else {
        0
    };
    (text, code)
}

/// Every finding in `src`, in document order.
pub fn check(src: &str) -> Vec<Finding> {
    let dom = domtree::parse(src);
    let order: Vec<NodeRef<'_>> = dom
        .roots()
        .flat_map(|root| std::iter::once(root).chain(root.descendants()))
        .collect();
    let mut raw = Vec::new();
    check_elements(&dom, &order, &mut raw);
    check_parse_errors(src, &dom, &order, &mut raw);
    // Stable: findings at one offset keep their check order.
    raw.sort_by_key(|f| f.at);

    let mut offsets: Vec<usize> = raw
        .iter()
        .flat_map(|f| std::iter::once(f.at).chain(f.related.as_ref().map(|r| r.0)))
        .collect();
    offsets.sort_unstable();
    offsets.dedup();
    let positions = positions(src, &offsets);
    let pos = |at: usize| {
        offsets
            .binary_search(&at)
            .ok()
            .and_then(|i| positions.get(i).copied())
            .unwrap_or((1, 1))
    };

    raw.into_iter()
        .map(|f| {
            let (line, column) = pos(f.at);
            let message = match f.related {
                Some((at, label)) => {
                    let (l, c) = pos(at);
                    format!("{} ({label} {l}:{c})", f.message)
                }
                None => f.message,
            };
            Finding {
                line,
                column,
                level: f.level,
                rule: f.rule,
                message,
            }
        })
        .collect()
}

fn start(node: NodeRef<'_>) -> usize {
    node.source_range().map_or(0, |r| r.start)
}

// --- element checks ---------------------------------------------------------

fn check_elements(dom: &Dom, order: &[NodeRef<'_>], out: &mut Vec<Raw>) {
    // Whether each node has (or contains) something that names it: visible
    // text, an image `alt`, an `aria-label`, … — bottom-up, in one pass.
    let mut named = vec![false; dom.len()];
    for node in order.iter().rev() {
        let i = node.id().index();
        let has = named.get(i).copied().unwrap_or(false) || names_itself(*node);
        if let Some(slot) = named.get_mut(i) {
            *slot = has;
        }
        if has && !is_hidden(*node) {
            if let Some(slot) = node.parent().and_then(|p| named.get_mut(p.id().index())) {
                *slot = true;
            }
        }
    }

    let mut ids: HashMap<&str, usize> = HashMap::new();
    let mut last_heading: Option<u8> = None;
    for node in order {
        let Some(tag) = node.tag_name() else {
            continue;
        };
        let at = start(*node);
        let has_name = named.get(node.id().index()).copied().unwrap_or(false);
        let empty = match tag {
            "img" if !img_has_alternative(*node) => Some((
                "img-alt",
                "<img> has no alt attribute (use alt=\"\" for a decorative image)".to_string(),
            )),
            "a" | "area" if node.has_attr("href") && !has_name => Some((
                "empty-link",
                format!("<{tag}> link has no text or accessible name"),
            )),
            "button" if !has_name => Some((
                "empty-button",
                "<button> has no text or accessible name".to_string(),
            )),
            "input" if input_type(*node) == "button" && !has_name => Some((
                "empty-button",
                "<input type=button> has no value or accessible name".to_string(),
            )),
            _ => None,
        };
        if let Some((rule, message)) = empty {
            out.push(Raw::new(at, Level::Error, rule, message));
        }
        if let Some(level) = heading_level(tag) {
            if let Some(prev) = last_heading.filter(|&prev| level > prev + 1) {
                out.push(Raw::new(
                    at,
                    Level::Warning,
                    "heading-order",
                    format!("heading level skipped: <h{level}> after <h{prev}>"),
                ));
            }
            last_heading = Some(level);
        }
        if let Some(id) = node.attr("id").filter(|id| !id.is_empty()) {
            match ids.entry(id) {
                Entry::Occupied(first) => out.push(
                    Raw::new(
                        at,
                        Level::Error,
                        "duplicate-id",
                        format!("duplicate id {id:?}"),
                    )
                    .related(*first.get(), "first used at"),
                ),
                Entry::Vacant(slot) => {
                    slot.insert(at);
                }
            }
        }
    }
}

fn non_blank(value: Option<&str>) -> bool {
    value.is_some_and(|v| !v.trim().is_empty())
}

/// Whether `node` itself supplies an accessible name or visible text.
fn names_itself(node: NodeRef<'_>) -> bool {
    if let Some(text) = node.text() {
        return !text.trim().is_empty();
    }
    let Some(tag) = node.tag_name() else {
        return false;
    };
    ["aria-label", "aria-labelledby", "title"]
        .iter()
        .any(|a| non_blank(node.attr(a)))
        || match tag {
            "img" | "area" => non_blank(node.attr("alt")),
            "input" => {
                matches!(input_type(node).as_str(), "submit" | "reset")
                    || non_blank(node.attr("value"))
            }
            _ => false,
        }
}

/// Elements whose content is never shown (or hidden from assistive tech).
fn is_hidden(node: NodeRef<'_>) -> bool {
    matches!(
        node.tag_name(),
        Some("script" | "style" | "template" | "head")
    ) || node.has_attr("hidden")
        || node
            .attr("aria-hidden")
            .is_some_and(|v| v.trim().eq_ignore_ascii_case("true"))
}

fn img_has_alternative(img: NodeRef<'_>) -> bool {
    img.has_attr("alt")
        || ["aria-label", "aria-labelledby", "title"]
            .iter()
            .any(|a| non_blank(img.attr(a)))
        || img.attr("role").is_some_and(|r| {
            r.split_ascii_whitespace()
                .any(|r| r.eq_ignore_ascii_case("presentation") || r.eq_ignore_ascii_case("none"))
        })
}

fn input_type(input: NodeRef<'_>) -> String {
    input
        .attr("type")
        .unwrap_or("text")
        .trim()
        .to_ascii_lowercase()
}

fn heading_level(tag: &str) -> Option<u8> {
    match tag.as_bytes() {
        [b'h', d @ b'1'..=b'6'] => Some(d - b'0'),
        _ => None,
    }
}

// --- parse errors -----------------------------------------------------------

fn check_parse_errors(src: &str, dom: &Dom, order: &[NodeRef<'_>], out: &mut Vec<Raw>) {
    // Elements closed by recovery end exactly where the error is (the start
    // of the closing tag, or the end of the input) and lack their own end
    // tag. Find them for every such error position, in one pass.
    let closing: HashSet<usize> = dom
        .errors()
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                ErrorKind::ImplicitlyClosed | ErrorKind::MismatchedEndTag
            )
        })
        .map(|e| e.position)
        .collect();
    let mut closed: HashMap<usize, Vec<NodeRef<'_>>> = HashMap::new();
    for node in order {
        let (Some(tag), Some(range)) = (node.tag_name(), node.source_range()) else {
            continue;
        };
        let end = range.end;
        if closing.contains(&end) && can_be_open(src, *node, range, tag) {
            closed.entry(end).or_default().push(*node);
        }
    }
    // The elements ending at one position are nested, so only the innermost
    // (last in document order) can end with an end tag of its own: in
    // `<div><div>x</div>` at the end of input, the `</div>` is the inner one's.
    let opens = end_tag_opens(src);
    for elements in closed.values_mut() {
        let innermost = elements
            .last()
            .and_then(|n| Some((n.tag_name()?, n.source_range()?)));
        if innermost.is_some_and(|(tag, range)| ends_with_end_tag(src, &opens, range, tag)) {
            elements.pop();
        }
    }

    let mut done: HashSet<usize> = HashSet::new();
    for e in dom.errors() {
        match e.kind {
            ErrorKind::ImplicitlyClosed | ErrorKind::MismatchedEndTag => {
                // The builder records one error per element it closes.
                if done.insert(e.position) {
                    let elements = closed.get(&e.position).map_or(&[][..], Vec::as_slice);
                    recovered_close(src, e, elements, out);
                }
            }
            _ => out.push(other_parse_error(src, e)),
        }
    }
}

/// Findings for elements closed by recovery at `e` (outermost first).
fn recovered_close(src: &str, e: &ParseError, elements: &[NodeRef<'_>], out: &mut Vec<Raw>) {
    let at = e.position;
    let by = match tag_at(src, at) {
        Some(tag) => tag,
        None if at >= src.len() => "the end of input".to_string(),
        None => "text".to_string(),
    };
    let required: Vec<NodeRef<'_>> = elements
        .iter()
        .copied()
        .filter(|n| !n.tag_name().is_some_and(optional_end_tag))
        .collect();
    // Innermost first, at most a few (a stray `</body>` can close thousands).
    let tags = |nodes: &[NodeRef<'_>], end: &str| -> String {
        const SHOWN: usize = 3;
        let mut s: String = nodes
            .iter()
            .rev()
            .take(SHOWN)
            .filter_map(|n| n.tag_name())
            .map(|t| format!("<{end}{t}>"))
            .collect();
        if nodes.len() > SHOWN {
            s.push_str(&format!(" and {} more", nodes.len() - SHOWN));
        }
        s
    };
    let Some(&innermost) = required.last() else {
        // Only optional end tags were left out: valid HTML.
        let level = match (elements.is_empty(), &e.kind) {
            // Nothing to show for it: keep the parser's verdict.
            (true, ErrorKind::MismatchedEndTag) => Level::Warning,
            _ => Level::Note,
        };
        let what = match elements {
            [] => "element".to_string(),
            _ => tags(elements, ""),
        };
        let rule = if level == Level::Note {
            "implicitly-closed"
        } else {
            "mismatched-end-tag"
        };
        out.push(Raw::new(
            at,
            level,
            rule,
            format!("{what} implicitly closed by {by}"),
        ));
        return;
    };
    let opened = |n: NodeRef<'_>| format!("<{}> opened at", n.tag_name().unwrap_or("element"));
    match e.kind {
        ErrorKind::MismatchedEndTag => out.push(
            Raw::new(
                at,
                Level::Warning,
                "mismatched-end-tag",
                format!("missing {} before {by}", tags(&required, "/")),
            )
            .related(start(innermost), opened(innermost)),
        ),
        _ if at >= src.len() => {
            for n in required {
                let tag = n.tag_name().unwrap_or("element");
                out.push(Raw::new(
                    start(n),
                    Level::Warning,
                    "unclosed-element",
                    format!("<{tag}> is never closed"),
                ));
            }
        }
        _ => out.push(
            Raw::new(
                at,
                Level::Warning,
                "unclosed-element",
                format!("missing {} before {by}", tags(&required, "/")),
            )
            .related(start(innermost), opened(innermost)),
        ),
    }
}

/// Elements whose end tag HTML lets you omit.
fn optional_end_tag(tag: &str) -> bool {
    matches!(
        tag,
        "html"
            | "head"
            | "body"
            | "li"
            | "dt"
            | "dd"
            | "p"
            | "rb"
            | "rt"
            | "rtc"
            | "rp"
            | "optgroup"
            | "option"
            | "colgroup"
            | "caption"
            | "thead"
            | "tbody"
            | "tfoot"
            | "tr"
            | "td"
            | "th"
    )
}

/// Whether an element can have been left open (it still needs an end tag of
/// its own to be closed): not void, and not self-closing (`<path/>`: no
/// children, and its source ends in `/>`).
fn can_be_open(src: &str, node: NodeRef<'_>, range: Range<usize>, tag: &str) -> bool {
    let self_closing =
        || node.first_child().is_none() && src.get(range).is_some_and(|s| s.ends_with("/>"));
    !is_void(tag) && !self_closing()
}

/// Elements that never have content or an end tag.
fn is_void(tag: &str) -> bool {
    matches!(
        tag,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

/// The byte offset of every `</` in `src`, in order.
fn end_tag_opens(src: &str) -> Vec<usize> {
    src.as_bytes()
        .windows(2)
        .enumerate()
        .filter(|(_, pair)| pair == b"</")
        .map(|(i, _)| i)
        .collect()
}

/// Whether the source of an element (`range`) ends with its own end tag:
/// its last `</` (from `opens`, see [`end_tag_opens`]) is followed by `tag`
/// (ASCII case-insensitive), then by anything up to a `>` that ends the
/// range — whitespace, attributes, a `/` — or by the end of the input. So
/// `</p>`, `</P\n\n  >`, `</p class=x>`, `</p/>` and `</p` at the very end
/// all count. A binary search, so O(log n) per element.
fn ends_with_end_tag(src: &str, opens: &[usize], range: Range<usize>, tag: &str) -> bool {
    let last = opens.partition_point(|&at| at + 2 <= range.end);
    let Some(&at) = last.checked_sub(1).and_then(|i| opens.get(i)) else {
        return false;
    };
    let Some(tail) = src.get(at + 2..range.end).filter(|_| at >= range.start) else {
        return false;
    };
    let name_len = tail
        .find(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>')
        .unwrap_or(tail.len());
    let (Some(name), Some(rest)) = (tail.get(..name_len), tail.get(name_len..)) else {
        return false;
    };
    name.eq_ignore_ascii_case(tag)
        && match tag_end(rest) {
            Some(gt) => gt + 1 == rest.len(),
            None => range.end >= src.len(),
        }
}

/// The offset of the `>` that ends a tag, in `rest` (what follows the tag
/// name), skipping quoted attribute values; `None` if the tag never ends.
fn tag_end(rest: &str) -> Option<usize> {
    let (mut quote, mut after_eq) = (None, false);
    for (i, c) in rest.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '>' => return Some(i),
                '"' | '\'' if after_eq => quote = Some(c),
                '=' => after_eq = true,
                c if c.is_ascii_whitespace() => {}
                _ => after_eq = false,
            },
        }
    }
    None
}

/// Any other recovered parse error.
fn other_parse_error(src: &str, e: &ParseError) -> Raw {
    let (level, rule) = match e.kind {
        ErrorKind::UnknownEntity => (Level::Note, "unknown-entity"),
        ErrorKind::StrayEndTag => (Level::Warning, "stray-end-tag"),
        ErrorKind::UnterminatedTag => (Level::Warning, "unterminated-tag"),
        ErrorKind::UnterminatedComment => (Level::Warning, "unterminated-comment"),
        ErrorKind::UnterminatedCdata => (Level::Warning, "unterminated-cdata"),
        ErrorKind::UnexpectedEof => (Level::Warning, "unexpected-eof"),
        ErrorKind::BogusComment => (Level::Warning, "bogus-comment"),
        _ => (Level::Warning, "parse-error"),
    };
    let message = match (&e.kind, tag_at(src, e.position)) {
        (ErrorKind::StrayEndTag, Some(tag)) => format!("{}: {tag}", e.message()),
        _ => e.message().to_string(),
    };
    Raw::new(e.position, level, rule, message)
}

/// The tag (`<li>`, `</div>`) that starts at byte `at` of `src`, if any.
fn tag_at(src: &str, at: usize) -> Option<String> {
    let rest = src.get(at..)?.strip_prefix('<')?;
    let (slash, rest) = match rest.strip_prefix('/') {
        Some(rest) => ("/", rest),
        None => ("", rest),
    };
    let len = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .unwrap_or(rest.len());
    let name = rest
        .get(..len)
        .filter(|n| n.starts_with(|c: char| c.is_ascii_alphabetic()))?;
    Some(format!("<{slash}{}>", name.to_ascii_lowercase()))
}

/// The 1-based `(line, column)` of each of the sorted byte `offsets`, as
/// [`domtree::line_col`] computes them (lines end at `\n`, `\r\n` or a lone
/// `\r`; columns count `char`s), in one forward scan over `src`.
fn positions(src: &str, offsets: &[usize]) -> Vec<(usize, usize)> {
    let bytes = src.as_bytes();
    let (mut line, mut column) = (1, 1);
    let mut chars = src.char_indices().peekable();
    let mut out = Vec::with_capacity(offsets.len());
    for &offset in offsets {
        while let Some(&(i, c)) = chars.peek() {
            // Offsets inside a multi-byte character belong to that character.
            if i + c.len_utf8() > offset {
                break;
            }
            chars.next();
            let breaks = c == '\n' || (c == '\r' && bytes.get(i + 1) != Some(&b'\n'));
            if breaks {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
        }
        out.push((line, column));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(html: &str) -> Vec<(usize, usize, &'static str)> {
        check(html)
            .into_iter()
            .map(|f| (f.line, f.column, f.rule))
            .collect()
    }

    fn report(html: &str) -> Vec<(usize, usize, Level, &'static str, String)> {
        check(html)
            .into_iter()
            .map(|f| (f.line, f.column, f.level, f.rule, f.message))
            .collect()
    }

    #[test]
    fn positions_match_line_col() {
        let src = "a\r\nbé\rc\n\n😀x\r\n";
        let offsets: Vec<usize> = (0..=src.len() + 2).collect();
        let fast = positions(src, &offsets);
        for (&offset, &got) in offsets.iter().zip(&fast) {
            assert_eq!(got, domtree::line_col(src, offset), "offset {offset}");
        }
        assert_eq!(positions("", &[0, 5]), [(1, 1), (1, 1)]);
    }

    #[test]
    fn images() {
        assert_eq!(rules("<img src=a>"), [(1, 1, "img-alt")]);
        for ok in [
            "<img alt>",
            "<img alt=''>",
            "<img aria-label=x>",
            "<img title=x>",
            "<img role=presentation>",
        ] {
            assert_eq!(rules(ok), [], "{ok}");
        }
    }

    #[test]
    fn links_and_buttons() {
        assert_eq!(
            rules("<a href=/>\n</a> <a href=/><img alt=''></a> <button></button>"),
            [
                (1, 1, "empty-link"),
                (2, 6, "empty-link"),
                (2, 33, "empty-button")
            ]
        );
        for ok in [
            "<a href=/>x</a>",
            "<a href=/ aria-label=Home></a>",
            "<a href=/ title=Home><svg></svg></a>",
            "<a href=/><img alt=Home></a>",
            "<a href=/><span><b>deep</b></span></a>",
            "<a href=/><svg><title>Home</title></svg></a>",
            "<a name=top></a>",
            "<button aria-labelledby=l></button>",
            "<input type=button value=Go>",
            "<input type=submit>",
            "<area href=/ alt=Map>",
        ] {
            assert_eq!(rules(ok), [], "{ok}");
        }
        // Hidden content does not name a link.
        assert_eq!(
            rules("<a href=/><span aria-hidden=true>x</span><script>y</script></a>"),
            [(1, 1, "empty-link")]
        );
        assert_eq!(rules("<input type=button>"), [(1, 1, "empty-button")]);
        assert_eq!(rules("<area href=/>"), [(1, 1, "empty-link")]);
    }

    #[test]
    fn nested_links_are_linear() {
        // `<a>` does not close an open `<a>`, so these nest 50,000 deep.
        let html = "<a href=/>".repeat(50_000) + "x";
        let findings = check(&html);
        assert_eq!(findings.len(), 50_000);
        assert!(findings.iter().all(|f| f.rule == "unclosed-element"));
    }

    #[test]
    fn duplicate_ids() {
        let findings = check("<p id=a></p>\n<p id=b><span id=a></span><i id=a></i></p>");
        let dups: Vec<_> = findings
            .iter()
            .map(|f| (f.line, f.column, f.rule, f.message.as_str()))
            .collect();
        assert_eq!(
            dups,
            [
                (
                    2,
                    9,
                    "duplicate-id",
                    "duplicate id \"a\" (first used at 1:1)"
                ),
                (
                    2,
                    27,
                    "duplicate-id",
                    "duplicate id \"a\" (first used at 1:1)"
                ),
            ]
        );
        assert_eq!(rules("<p id></p><p id></p>"), []);
    }

    #[test]
    fn heading_order() {
        let findings = check("<h1>a</h1><h3>b</h3><h2>c</h2><h3>d</h3><h6>e</h6>");
        let skips: Vec<_> = findings
            .iter()
            .map(|f| (f.column, f.message.as_str()))
            .collect();
        assert_eq!(
            skips,
            [
                (11, "heading level skipped: <h3> after <h1>"),
                (41, "heading level skipped: <h6> after <h3>"),
            ]
        );
    }

    #[test]
    fn omitted_optional_end_tags_are_notes() {
        let html = "<ul><li>a<li>b</ul>\n<table><tr><td>1<td>2</table><p>x<div></div>";
        let got = report(html);
        assert!(got.iter().all(|f| f.2 == Level::Note), "{got:?}");
        let got: Vec<_> = got.iter().map(|f| (f.0, f.1, f.4.as_str())).collect();
        assert_eq!(
            got,
            [
                (1, 10, "<li> implicitly closed by <li>"),
                (1, 15, "<li> implicitly closed by </ul>"),
                (2, 17, "<td> implicitly closed by <td>"),
                (2, 22, "<td><tr> implicitly closed by </table>"),
                (2, 34, "<p> implicitly closed by <div>"),
            ]
        );
        assert_eq!(
            report("<html><body><p>x"),
            [(
                1,
                17,
                Level::Note,
                "implicitly-closed",
                "<p><body><html> implicitly closed by the end of input".to_string()
            )]
        );
    }

    #[test]
    fn missing_required_end_tags_are_warnings() {
        assert_eq!(
            report("<div>\n  <b><i>x</b>\n<h1>T<h2>U</h2>"),
            [
                (
                    1,
                    1,
                    Level::Warning,
                    "unclosed-element",
                    "<div> is never closed".to_string()
                ),
                (
                    2,
                    10,
                    Level::Warning,
                    "mismatched-end-tag",
                    "missing </i> before </b> (<i> opened at 2:6)".to_string()
                ),
                (
                    3,
                    6,
                    Level::Warning,
                    "unclosed-element",
                    "missing </h1> before <h2> (<h1> opened at 3:1)".to_string()
                ),
            ]
        );
        // Void and self-closing elements were never open.
        assert_eq!(
            rules("<p>x<img>"),
            [(1, 5, "img-alt"), (1, 10, "implicitly-closed")]
        );
        assert_eq!(rules("<svg><path d=M0/>"), [(1, 1, "unclosed-element")]);
        // An ancestor ending in a self-closing child is still open.
        assert_eq!(rules("<div><br/>"), [(1, 1, "unclosed-element")]);
        assert_eq!(rules("<div><x/>"), [(1, 1, "unclosed-element")]);
        // A mix: only the required end tags are named.
        assert_eq!(
            report("<ol><li><span>x</ol>"),
            [(
                1,
                16,
                Level::Warning,
                "mismatched-end-tag",
                "missing </span> before </ol> (<span> opened at 1:9)".to_string()
            )]
        );
    }

    #[test]
    fn other_parse_errors() {
        assert_eq!(
            report("<p>a</p></div>\n<p>&bogus;</p><!x>"),
            [
                (
                    1,
                    9,
                    Level::Warning,
                    "stray-end-tag",
                    "stray end tag with no matching open element: </div>".to_string()
                ),
                (
                    2,
                    4,
                    Level::Note,
                    "unknown-entity",
                    "unknown character reference left verbatim".to_string()
                ),
                (
                    2,
                    15,
                    Level::Warning,
                    "bogus-comment",
                    "bogus comment".to_string()
                ),
            ]
        );
        assert_eq!(
            rules("<p>x<!-- open"),
            [(1, 5, "unterminated-comment"), (1, 14, "implicitly-closed")]
        );
    }

    #[test]
    fn long_closing_lists_are_capped() {
        let html = format!("<body>{}x</body>", "<b>".repeat(10_000));
        let findings = check(&html);
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].message,
            "missing </b></b></b> and 9997 more before </body> (<b> opened at 1:30004)"
        );
    }

    #[test]
    fn never_panics_on_prefixes() {
        let html = "<!doctype html>\r<html><body><h3 id=a>é<a href=x><img></a>\n<div id=a>\
                    <b><i>&bogus;</b><table><tr><td>1<td></table><!-- c --><svg><path/>\
                    </div></p><button><input type=button value=''><h1>x<h2>y</h2><!x><x/";
        for (end, _) in html.char_indices() {
            let _ = check(&html[..end]);
        }
        let _ = check(html);
    }

    #[test]
    fn clean_document() {
        assert_eq!(
            check(
                "<!doctype html><html lang=en><head><title>x</title></head>\
                 <body><h1>Hi</h1><p>ok</p></body></html>"
            ),
            []
        );
    }

    #[test]
    fn end_tag_detection() {
        let ends = |src: &str, range: Range<usize>, tag: &str| {
            ends_with_end_tag(src, &end_tag_opens(src), range, tag)
        };
        let src = "<p>x</P ><div>y</div>é<b>z";
        assert!(ends(src, 0..9, "p"));
        assert!(ends(src, 9..21, "div"));
        assert!(!ends(src, 23..src.len(), "b"));
        assert!(!ends(src, 3..4, "p"));
        assert!(!ends(src, 0..500, "p"));
        // The `</` must be inside the range.
        assert!(!ends(src, 5..9, "p"));
        // Any amount of whitespace, attributes, a `/`, quoted `>`s.
        let spaced = format!("<p>x</p{}>", " \n\t".repeat(20));
        assert!(ends(&spaced, 0..spaced.len(), "p"));
        for src in [
            "<p>x</p class=a>",
            "<p>x</p/>",
            "<p>x</p title=\"a>b\" x='>'>",
            "<p>x</p foo's>",
        ] {
            assert!(ends(src, 0..src.len(), "p"), "{src}");
        }
        // A `>` missing at the end of input, but not elsewhere.
        assert!(ends("<p>x</p", 0..7, "p"));
        assert!(ends("<p>x</p  ", 0..9, "p"));
        assert!(!ends("<p>x</p<b>", 0..7, "p"));
        // Another name, or text after the end tag.
        for src in [
            "<p>x</pre>",
            "<p>x</px>",
            "<p>x</ p>",
            "<p>x</p><br>",
            "<p>x</p>y",
        ] {
            assert!(!ends(src, 0..src.len(), "p"), "{src}");
        }
    }

    #[test]
    fn end_tags_with_junk_close_their_element() {
        // An end tag cut off by the end of input still closes its element.
        assert_eq!(
            report("<div><span>x</span"),
            [
                (
                    1,
                    1,
                    Level::Warning,
                    "unclosed-element",
                    "<div> is never closed".to_string()
                ),
                (
                    1,
                    13,
                    Level::Warning,
                    "unterminated-tag",
                    "unterminated tag".to_string()
                ),
            ]
        );
        // Attributes, a `/` or lots of whitespace in an end tag.
        let spaces = " ".repeat(40);
        for end_tag in [
            "</span foo>",
            "</span/>",
            "</SPAN\n>",
            &format!("</span{spaces}>"),
        ] {
            let html = format!("<ul><li><span>x{end_tag}<li>y</ul>");
            let got = report(&html);
            assert!(got.iter().all(|f| f.2 == Level::Note), "{html}: {got:?}");
        }
        // Of nested elements ending together, the end tag is the inner one's.
        assert_eq!(
            report("<div><div>x</div>"),
            [(
                1,
                1,
                Level::Warning,
                "unclosed-element",
                "<div> is never closed".to_string()
            )]
        );
        assert_eq!(
            rules("<div><div>x</div"),
            [(1, 1, "unclosed-element"), (1, 12, "unterminated-tag")]
        );
    }

    #[test]
    fn tags_at_offsets() {
        assert_eq!(tag_at("x<LI class=a>", 1).as_deref(), Some("<li>"));
        assert_eq!(tag_at("</my-el>", 0).as_deref(), Some("</my-el>"));
        assert_eq!(tag_at("< b", 0), None);
        assert_eq!(tag_at("<", 0), None);
        assert_eq!(tag_at("é", 1), None);
    }
}
