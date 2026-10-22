//! Indented HTML output for `--pretty`.
//!
//! Uses an explicit stack, so arbitrarily deep documents can't overflow it.
//! Whitespace-sensitive elements (`<pre>`, `<textarea>`, `<script>`, `<style>`)
//! and elements holding only a short run of text are kept on one line, as-is.

use domq::{NodeKind, NodeRef};

const INDENT: &str = "  ";
const INLINE_MAX: usize = 80;
/// Indentation stops growing past this depth, so pathological nesting can't
/// make the output quadratic in size.
const MAX_INDENT: usize = 32;

enum Step<'a> {
    Open(NodeRef<'a>, usize),
    Close(&'a str, usize),
}

/// Append an indented rendering of `node` (and its subtree) to `out`.
pub fn write(node: NodeRef<'_>, out: &mut String) {
    let mut stack = vec![Step::Open(node, 0)];
    while let Some(step) = stack.pop() {
        match step {
            Step::Close(tag, depth) => line(out, depth, &format!("</{tag}>")),
            Step::Open(n, depth) => match n.kind() {
                NodeKind::Element { tag } => {
                    if keep_verbatim(tag) || is_short_text_only(n) {
                        line(out, depth, &n.to_html());
                        continue;
                    }
                    line(out, depth, &start_tag(n, tag));
                    if n.first_child().is_none() {
                        // `<div></div>` or a void element: nothing to indent.
                        if !is_void(tag) {
                            out.truncate(out.len() - 1);
                            out.push_str(&format!("</{tag}>\n"));
                        }
                        continue;
                    }
                    stack.push(Step::Close(tag, depth));
                    let mut child = n.last_child();
                    while let Some(c) = child {
                        stack.push(Step::Open(c, depth + 1));
                        child = c.prev_sibling();
                    }
                }
                NodeKind::Text(_) => {
                    let text = collapse(&n.to_html());
                    if !text.is_empty() {
                        line(out, depth, &text);
                    }
                }
                NodeKind::Comment(_) | NodeKind::Doctype(_) => line(out, depth, &n.to_html()),
            },
        }
    }
}

fn line(out: &mut String, depth: usize, s: &str) {
    for _ in 0..depth.min(MAX_INDENT) {
        out.push_str(INDENT);
    }
    out.push_str(s);
    out.push('\n');
}

fn start_tag(n: NodeRef<'_>, tag: &str) -> String {
    let mut s = format!("<{tag}");
    for (k, v) in n.attributes() {
        s.push(' ');
        s.push_str(k);
        if !v.is_empty() {
            s.push_str("=\"");
            s.push_str(&v.replace('&', "&amp;").replace('"', "&quot;"));
            s.push('"');
        }
    }
    s.push('>');
    s
}

fn keep_verbatim(tag: &str) -> bool {
    matches!(tag, "pre" | "textarea" | "script" | "style")
}

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

fn is_short_text_only(n: NodeRef<'_>) -> bool {
    let mut len = 0;
    for c in n.children() {
        match c.text() {
            Some(t) if !t.contains('\n') => len += t.len(),
            _ => return false,
        }
    }
    len > 0 && len <= INLINE_MAX
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pretty(html: &str) -> String {
        let dom = domq::parse(html);
        let mut out = String::new();
        for r in dom.roots() {
            write(r, &mut out);
        }
        out
    }

    #[test]
    fn indents_nested_elements() {
        assert_eq!(
            pretty("<ul class=x><li>a<li><b>b</b> c</ul>"),
            "<ul class=\"x\">\n  <li>a</li>\n  <li>\n    <b>b</b>\n    c\n  </li>\n</ul>\n"
        );
    }

    #[test]
    fn indentation_is_capped() {
        let html = "<div>".repeat(100) + "x";
        let out = pretty(&html);
        let widest = out.lines().map(|l| l.len() - l.trim_start().len()).max();
        assert_eq!(widest, Some(MAX_INDENT * INDENT.len()));
    }

    #[test]
    fn keeps_pre_and_empty_and_void() {
        assert_eq!(
            pretty("<div><pre> a\n  b</pre><span></span><br></div>"),
            "<div>\n  <pre> a\n  b</pre>\n  <span></span>\n  <br>\n</div>\n"
        );
    }
}
