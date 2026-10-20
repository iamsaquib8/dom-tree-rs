//! Hyperlinks ([`Dom::links`]) and embedded resources ([`Dom::resources`]),
//! with URLs resolved against the page's base URL.

use alloc::string::String;
use alloc::vec::Vec;

use super::{collapse, document_order, non_empty};
use crate::node::NodeKind;
use crate::serialize::{is_space, walk};
use crate::tags::{self, Display};
use crate::tree::{Dom, NodeRef};
use crate::url::{clean, is_javascript, resolve_url, srcset_candidates};

/// A hyperlink — an `<a href>` or `<area href>` — found by [`Dom::links`].
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct Link<'a> {
    /// The `<a>` or `<area>` element.
    pub node: NodeRef<'a>,
    /// The `href` attribute exactly as written.
    pub href: &'a str,
    /// `href` resolved against the base URL (see [`Dom::base_url`]). Without
    /// an absolute base this is `href` with surrounding whitespace removed,
    /// and may be relative.
    pub url: String,
    /// The readable link text, on one line: the element's visible text with
    /// whitespace collapsed. When that is empty (an icon link), the first of
    /// `aria-label`, `alt` (for `<area>`), the `alt` of an image inside, or
    /// `title`; otherwise empty.
    pub text: String,
}

/// A URL-valued attribute that loads something into the page — an image,
/// script, stylesheet, media file or frame — found by [`Dom::resources`].
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct Resource<'a> {
    /// The element carrying the URL.
    pub node: NodeRef<'a>,
    /// The attribute it came from: `"href"` (on `<link>`), `"src"`,
    /// `"srcset"`, `"poster"` or `"data"` (on `<object>`).
    pub attr: &'static str,
    /// The URL as written. For `srcset`, one candidate's URL (without its
    /// descriptor); a `srcset` yields one `Resource` per candidate.
    pub value: &'a str,
    /// `value` resolved against the base URL, as for [`Link::url`].
    pub url: String,
}

impl Dom {
    /// The URL relative links in this document resolve against.
    ///
    /// That is the `href` of the first `<base href>` element, resolved against
    /// `base` (typically the URL the page was fetched from); without a
    /// `<base>` element, `base` itself. `None` when neither is available.
    ///
    /// ```
    /// let dom = domtree::parse(r#"<head><base href="/docs/"></head>"#);
    /// assert_eq!(
    ///     dom.base_url(Some("https://example.com/index.html")).as_deref(),
    ///     Some("https://example.com/docs/"),
    /// );
    /// assert_eq!(dom.base_url(None).as_deref(), Some("/docs/"));
    /// assert_eq!(domtree::parse("<p>x</p>").base_url(None), None);
    /// ```
    pub fn base_url(&self, base: Option<&str>) -> Option<String> {
        let tag = document_order(self)
            .filter(|n| n.tag_name() == Some("base"))
            .find_map(|n| n.attr("href"));
        match (tag, base) {
            (Some(href), Some(base)) => Some(resolve_url(base, href)),
            (Some(href), None) => Some(clean(href)).filter(|h| !h.is_empty()),
            (None, Some(base)) => Some(clean(base)),
            (None, None) => None,
        }
    }

    /// Every hyperlink in the document — each `<a href>` and `<area href>` —
    /// in document order, with its URL resolved and its readable text.
    ///
    /// URLs resolve against [`Dom::base_url`]`(base)`, so a `<base href>` in
    /// the page is honoured. `javascript:` links are skipped: they run code
    /// rather than navigate. Fragment-only (`#top`), `mailto:` and other links
    /// are kept; filter on [`Link::url`] as needed. Embedded resources (`<img
    /// src>`, `<link href>`, `<script src>`, …) are not links; see
    /// [`Dom::resources`].
    ///
    /// Link text comes from one pass over the document; in invalid nested
    /// links (`<a><a>…</a></a>`), the inner text belongs to the inner link
    /// only.
    ///
    /// ```
    /// let dom = domtree::parse(
    ///     r#"<nav><a href="/docs/">Docs</a> <a href="javascript:void(0)">Menu</a></nav>
    ///        <p>See <a href="../faq.html#top">the
    ///        FAQ</a> or <a href="https://example.org"><img src=logo.png alt="Example"></a>.</p>"#,
    /// );
    /// let links = dom.links(Some("https://example.com/en/guide/"));
    /// let found: Vec<_> = links.iter().map(|l| (l.url.as_str(), l.text.as_str())).collect();
    /// assert_eq!(found, [
    ///     ("https://example.com/docs/", "Docs"),
    ///     ("https://example.com/en/faq.html#top", "the FAQ"),
    ///     ("https://example.org", "Example"),
    /// ]);
    /// assert_eq!(links[1].href, "../faq.html#top");
    /// ```
    pub fn links(&self, base: Option<&str>) -> Vec<Link<'_>> {
        let base = self.base_url(base);
        let base = base.as_deref().unwrap_or("");
        let mut links: Vec<Link<'_>> = Vec::new();
        let mut collector = TextCollector::default();
        for root in self.roots() {
            walk(root, |node, enter| {
                collector.visit(node, enter, |node, href| {
                    links.push(Link {
                        node,
                        href,
                        url: resolve_url(base, href),
                        text: String::new(),
                    });
                    links.len() - 1
                })
            });
        }
        for (i, state) in collector.done.into_iter().enumerate() {
            if let Some(link) = links.get_mut(i) {
                link.text = state.finish(link.node);
            }
        }
        links
    }

    /// Every embedded resource URL in the document, in document order:
    /// `<link href>` (stylesheets, icons, feeds, …), every `src` (images,
    /// scripts, frames, media, …), each candidate of a `srcset`, `poster` and
    /// `<object data>`, resolved against [`Dom::base_url`]`(base)`.
    ///
    /// Empty values and `javascript:` URLs are skipped. For each element the
    /// attributes come in the order `href`, `src`, `srcset`, `poster`, `data`;
    /// if an attribute is repeated, the first one counts (as with
    /// [`NodeRef::attr`]).
    ///
    /// ```
    /// let dom = domtree::parse(
    ///     r#"<link rel=stylesheet href=/main.css>
    ///        <img src="a.png" srcset="a.png 1x, a@2x.png 2x">
    ///        <script src="https://cdn.example.com/x.js"></script>"#,
    /// );
    /// let urls: Vec<_> = dom
    ///     .resources(Some("https://example.com/blog/"))
    ///     .into_iter()
    ///     .map(|r| (r.attr, r.url))
    ///     .collect();
    /// assert_eq!(urls, [
    ///     ("href", "https://example.com/main.css".to_string()),
    ///     ("src", "https://example.com/blog/a.png".to_string()),
    ///     ("srcset", "https://example.com/blog/a.png".to_string()),
    ///     ("srcset", "https://example.com/blog/a@2x.png".to_string()),
    ///     ("src", "https://cdn.example.com/x.js".to_string()),
    /// ]);
    /// ```
    pub fn resources(&self, base: Option<&str>) -> Vec<Resource<'_>> {
        let base = self.base_url(base);
        let base = base.as_deref().unwrap_or("");
        let mut out = Vec::new();
        for node in document_order(self) {
            let Some(tag) = node.tag_name() else {
                continue;
            };
            let mut attrs: [Option<&'static str>; 5] =
                [None, Some("src"), Some("srcset"), Some("poster"), None];
            if tag == "link" {
                attrs[0] = Some("href");
            }
            if tag == "object" {
                attrs[4] = Some("data");
            }
            for attr in attrs.into_iter().flatten() {
                let Some(value) = node.attr(attr) else {
                    continue;
                };
                if attr == "srcset" {
                    for (url, _) in srcset_candidates(value) {
                        add_resource(&mut out, base, node, attr, url);
                    }
                } else {
                    add_resource(&mut out, base, node, attr, value);
                }
            }
        }
        out
    }
}

fn add_resource<'a>(
    out: &mut Vec<Resource<'a>>,
    base: &str,
    node: NodeRef<'a>,
    attr: &'static str,
    value: &'a str,
) {
    if non_empty(value).is_some() && !is_javascript(value) {
        out.push(Resource {
            node,
            attr,
            value,
            url: resolve_url(base, value),
        });
    }
}

/// Collects the readable text of every link in one walk (so nested links cost
/// no extra passes): text goes to the innermost open link.
#[derive(Default)]
struct TextCollector<'a> {
    /// Open links: (element, index into `done`).
    open: Vec<(NodeRef<'a>, usize)>,
    done: Vec<LinkText<'a>>,
}

#[derive(Default)]
struct LinkText<'a> {
    text: String,
    space: bool,
    /// The `alt` of the first image inside, as a fallback.
    alt: Option<&'a str>,
}

impl<'a> TextCollector<'a> {
    /// Visit a node during `walk`; `found` registers a new link and returns its
    /// index. Returns whether to descend.
    fn visit(
        &mut self,
        node: NodeRef<'a>,
        enter: bool,
        mut found: impl FnMut(NodeRef<'a>, &'a str) -> usize,
    ) -> bool {
        let tag = match node.kind() {
            NodeKind::Element { tag } => tag,
            NodeKind::Text(t) => {
                if let Some(link) = self.current() {
                    link.push(t);
                }
                return false;
            }
            NodeKind::Comment(_) | NodeKind::Doctype(_) => return false,
        };
        if !enter {
            if self.open.last().is_some_and(|(n, _)| n.id() == node.id()) {
                self.open.pop();
            }
            if tags::display(tag) != Display::Inline {
                if let Some(link) = self.current() {
                    link.space = true;
                }
            }
            return false;
        }
        let display = tags::display(tag);
        if display == Display::Hidden {
            return false;
        }
        if let Some(link) = self.current() {
            if display != Display::Inline || tag == "br" {
                link.space = true;
            }
            if tag == "img" && link.alt.is_none() {
                link.alt = node.attr("alt").and_then(non_empty);
            }
        }
        if matches!(tag, "a" | "area") {
            if let Some(href) = node.attr("href").filter(|h| !is_javascript(h)) {
                let index = found(node, href);
                // `found` pushes links in order, so `index == done.len()`.
                self.done.push(LinkText::default());
                self.open.push((node, index));
            }
        }
        true
    }

    fn current(&mut self) -> Option<&mut LinkText<'a>> {
        let &(_, index) = self.open.last()?;
        self.done.get_mut(index)
    }
}

impl<'a> LinkText<'a> {
    fn push(&mut self, text: &str) {
        for (i, word) in text.split(is_space).enumerate() {
            if i > 0 {
                self.space = true;
            }
            if !word.is_empty() {
                if self.space && !self.text.is_empty() {
                    self.text.push(' ');
                }
                self.space = false;
                self.text.push_str(word);
            }
        }
    }

    fn finish(self, node: NodeRef<'a>) -> String {
        if !self.text.is_empty() {
            return self.text;
        }
        let attr = |name| node.attr(name).and_then(non_empty);
        attr("aria-label")
            .or_else(|| attr("alt"))
            .or(self.alt)
            .or_else(|| attr("title"))
            .map(collapse)
            .unwrap_or_default()
    }
}
